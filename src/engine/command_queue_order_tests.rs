use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_store(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-command-order-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn first_contended_claim_does_not_allow_later_manual_choice_to_overtake() {
    with_store(|store| {
        let source = store.home.join("Research.url");
        let original = b"[InternetShortcut]\r\nURL=https://example.com/research\r\n";
        std::fs::write(&source, original).unwrap();
        let app = native::read_link(&source).unwrap();
        store.put_app(&app).unwrap();

        // A user first chooses Always, then changes the same shortcut to
        // Never. A delayed first claim must not allow Never to run first.
        store.command(&format!("policy:{}:Always", app.id)).unwrap();
        store.command(&format!("policy:{}:Never", app.id)).unwrap();
        let queued = store.pending().unwrap();
        assert_eq!(queued.len(), 2);
        let (first, second) = (queued[0].0, queued[1].0);

        store.db.busy_timeout(Duration::from_millis(1)).unwrap();
        let writer = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE;").unwrap();

        let mut attempted = Vec::new();
        let mut executed = Vec::new();
        let mut first_attempt = true;
        assert!(
            dispatch_pending_commands_with_claim(
                store,
                |store, id| {
                    attempted.push(id);
                    // This is the real claim against a locked SQLite writer.
                    let outcome = claim_pending_command(store, id);
                    // The competing writer exits immediately after the first
                    // failed claim, before a hypothetical second attempt.
                    if first_attempt {
                        writer.execute_batch("ROLLBACK;")?;
                        first_attempt = false;
                    }
                    outcome
                },
                |id, action| {
                    executed.push(id);
                    let result = command(store, action);
                    store.finish(id, &result)?;
                    result.map(|_| ())
                },
            )
            .unwrap()
        );
        assert_eq!(attempted, [first]);
        assert!(
            executed.is_empty(),
            "a later action must not overtake a busy first claim"
        );
        assert_eq!(store.pending().unwrap(), queued);
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Auto);

        // Once contention ends, execute the exact same queued policy actions
        // through the production dispatcher, not a substitute state machine.
        store.db.busy_timeout(Duration::from_secs(5)).unwrap();
        assert!(
            dispatch_pending_commands(store, |id, action| {
                executed.push(id);
                let result = command(store, action);
                store.finish(id, &result)?;
                result.map(|_| ())
            })
            .unwrap()
        );
        assert_eq!(executed, [first, second]);
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
        assert!(store.pending().unwrap().is_empty());
        assert_eq!(store.events().unwrap().len(), 2);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        // No duplicate action on subsequent polls.
        assert!(dispatch_pending_commands(store, |_, _| panic!("no replay")).unwrap());
    });
}

#[test]
fn shutdown_during_dispatch_leaves_later_command_unclaimed() {
    with_store(|store| {
        store.command("apply").unwrap();
        store.command("rollback").unwrap();
        let queued = store.pending().unwrap();
        let mut visited = Vec::new();
        assert!(
            !dispatch_pending_commands(store, |id, _action| {
                visited.push(id);
                store.finish(id, &Ok("Handled before stop".into()))?;
                store.set("shutdown", &true)
            })
            .unwrap()
        );
        assert_eq!(visited, [queued[0].0]);
        assert_eq!(store.pending().unwrap(), vec![queued[1].clone()]);
        let state: String = store
            .db
            .query_row(
                "SELECT status FROM commands WHERE id=?1",
                [queued[1].0],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "pending");
    });
}
