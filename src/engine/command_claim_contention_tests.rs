use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_store(check: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-command-claim-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    check(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn writer_contention_defers_both_rescan_and_manual_actions_until_unlock() {
    with_store(|store| {
        // These are real commands submitted by Settings/CLI, not a separate
        // simulation of the queue's state machine.
        store.command("rescan").unwrap();
        store.command("policy:fixture:Always").unwrap();
        let pending = store.pending().unwrap();
        assert_eq!(pending.len(), 2);
        let first = pending[0].0;
        let second = pending[1].0;

        // Force the real Store::claim_command to exhaust its normal bounded
        // writer retries without holding the test for the five-second default
        // SQLite busy timeout on each attempt.
        store.db.busy_timeout(Duration::from_millis(1)).unwrap();
        let writer = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
        writer.execute_batch("BEGIN IMMEDIATE;").unwrap();

        assert!(!claim_pending_command(store, first).unwrap());
        assert!(!claim_pending_command(store, second).unwrap());
        assert_eq!(store.pending().unwrap(), pending);
        let running: i64 = store
            .db
            .query_row(
                "SELECT COUNT(*) FROM commands WHERE status='running'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(running, 0, "contended requests must not be claimed");

        writer.execute_batch("ROLLBACK;").unwrap();
        assert!(claim_pending_command(store, first).unwrap());
        assert!(claim_pending_command(store, second).unwrap());
        assert!(!claim_pending_command(store, first).unwrap());
        assert!(!claim_pending_command(store, second).unwrap());
        assert!(store.pending().unwrap().is_empty());

        store.finish(first, &Ok("scan finished".into())).unwrap();
        store.finish(second, &Ok("manual choice finished".into())).unwrap();
        let done: i64 = store
            .db
            .query_row(
                "SELECT COUNT(*) FROM commands WHERE status='done'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(done, 2);
        assert_eq!(store.events().unwrap().len(), 2);
    });
}

#[test]
fn invalid_command_storage_is_not_disguised_as_temporary_contention() {
    with_store(|store| {
        store.command("rescan").unwrap();
        let id = store.pending().unwrap()[0].0;
        store.db.execute_batch("DROP TABLE commands;").unwrap();
        assert!(claim_pending_command(store, id).is_err());
    });
}
