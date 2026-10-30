use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-command-completion-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn command_state(store: &Store, id: i64) -> (String, String) {
    store
        .db
        .query_row(
            "SELECT status,message FROM commands WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap()
}

#[test]
fn late_worker_completion_cannot_resurrect_a_recovered_command() {
    with_profile(|store| {
        store.command("rescan").unwrap();
        let id = store.db.last_insert_rowid();
        assert!(store.claim_command(id).unwrap());

        // A second real SQLite connection represents a restarted monitor
        // that has canceled a command left running by the previous worker.
        let restarted = Store::open(&store.home).unwrap();
        restarted.recover_commands().unwrap();
        let canceled = command_state(&restarted, id);
        assert_eq!(canceled.0, "error");
        assert!(canceled.1.contains("Команда отменена"));
        let events = restarted.events().unwrap();

        // Delayed completion is not permission to overwrite cancellation or
        // tell Settings that the old worker updated the catalog successfully.
        store
            .finish(id, &Ok("A stale scan says the catalog is fresh".into()))
            .unwrap();
        assert_eq!(command_state(&restarted, id), canceled);
        assert_eq!(restarted.events().unwrap(), events);

        store
            .finish(id, &Err(anyhow::anyhow!("A stale scanner failed")))
            .unwrap();
        assert_eq!(command_state(&restarted, id), canceled);
        assert_eq!(restarted.events().unwrap(), events);
    });
}

#[test]
fn only_claimed_commands_finish_and_terminal_outcomes_are_final() {
    with_profile(|store| {
        store.command("apply").unwrap();
        let id = store.db.last_insert_rowid();
        let original = command_state(store, id);
        assert_eq!(original.0, "pending");

        store
            .finish(id, &Ok("Not actually executed".into()))
            .unwrap();
        assert_eq!(command_state(store, id), original);
        assert!(store.events().unwrap().is_empty());

        assert!(store.claim_command(id).unwrap());
        store.finish(id, &Ok("Applied once".into())).unwrap();
        assert_eq!(
            command_state(store, id),
            ("done".into(), "Applied once".into())
        );
        let events = store.events().unwrap();
        assert_eq!(events.len(), 1);

        store
            .finish(id, &Err(anyhow::anyhow!("Late conflicting answer")))
            .unwrap();
        store.finish(id, &Ok("Duplicate success".into())).unwrap();
        assert_eq!(
            command_state(store, id),
            ("done".into(), "Applied once".into())
        );
        assert_eq!(store.events().unwrap(), events);
    });
}

#[test]
fn turning_off_background_preserves_cancellation_against_late_completion() {
    with_profile(|store| {
        store.command("rescan").unwrap();
        let id = store.db.last_insert_rowid();
        assert!(store.claim_command(id).unwrap());

        // Exercise the actual settings-side Turn off transaction rather than
        // replacing the command state directly in a test model.
        let disabled = crate::setup::disable_background(&store.home).unwrap();
        assert!(!disabled.collect);
        let canceled = command_state(store, id);
        assert_eq!(canceled.0, "error");
        assert!(canceled.1.contains("background work was disabled"));
        let events = store.events().unwrap();

        store
            .finish(id, &Ok("Outdated success after Turn off".into()))
            .unwrap();
        assert_eq!(command_state(store, id), canceled);
        assert_eq!(store.events().unwrap(), events);
        assert!(store.get::<bool>("background_disabled").unwrap());
        assert!(store.get::<bool>("shutdown").unwrap());
    });
}
