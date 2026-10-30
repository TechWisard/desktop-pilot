use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

// This uses Settings' real pending-command acknowledgement and the same
// SQLite transitions as the worker. No alternative status model is involved.
#[test]
fn cancelled_refresh_is_never_reported_as_success_when_old_worker_replies() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "test-ui-terminal-command-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    drop(store);

    let mut settings = Settings::new(home.clone()).unwrap();
    let initial_config: Config = settings.store.get("config").unwrap();
    let initial_state = serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap();
    settings.store.command("rescan").unwrap();
    let id = settings.store.db.last_insert_rowid();
    assert!(settings.store.claim_command(id).unwrap());
    settings.pending_commands.push(PendingCommand {
        id,
        action: "rescan".into(),
        submitted: Instant::now(),
    });

    // An independent SQLite connection restarts/cancels the prior worker.
    let restarted = Store::open(&home).unwrap();
    restarted.recover_commands().unwrap();
    let canceled: (String, String) = restarted
        .db
        .query_row("SELECT status,message FROM commands WHERE id=?1", [id], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(canceled.0, "error");
    let events = restarted.events().unwrap();
    restarted
        .finish(id, &Ok("Stale successful refresh".into()))
        .unwrap();
    assert_eq!(restarted.events().unwrap(), events);

    settings.reload().unwrap();
    assert!(settings.pending_commands.is_empty());
    assert_eq!(settings.active_commands, 0);
    let notice = settings.notice.as_ref().expect("cancellation feedback");
    assert_eq!(notice.kind, NoticeKind::Error);
    assert_eq!(notice.text, canceled.1);
    assert!(!notice.text.contains("Stale successful refresh"));

    // A genuinely new claimed command must still reach the same UI and
    // show a successful result; cancellation must not poison later work.
    restarted.command("rescan").unwrap();
    let fresh_id = restarted.db.last_insert_rowid();
    assert!(restarted.claim_command(fresh_id).unwrap());
    settings.pending_commands.push(PendingCommand {
        id: fresh_id,
        action: "rescan".into(),
        submitted: Instant::now(),
    });
    restarted
        .finish(fresh_id, &Ok("Verified fresh catalog".into()))
        .unwrap();
    settings.reload().unwrap();
    assert!(settings.pending_commands.is_empty());
    let notice = settings.notice.as_ref().expect("fresh completion feedback");
    assert_eq!(notice.kind, NoticeKind::Success);
    assert_eq!(notice.text, "Verified fresh catalog");
    assert_eq!(settings.store.get::<Config>("config").unwrap(), initial_config);
    assert_eq!(
        serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
        initial_state
    );
    drop(restarted);
    drop(settings);
    std::fs::remove_dir_all(home).unwrap();
}
