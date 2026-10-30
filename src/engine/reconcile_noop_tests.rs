use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[test]
fn unchanged_managed_shortcuts_do_not_rewrite_state_or_require_sqlite_writer() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-noop-reconcile-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    let original = home.join("Chosen.url");
    let original_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Chosen\r\n";
    std::fs::write(&original, original_bytes).unwrap();
    let mut app = native::read_link(&original).unwrap();
    app.policy = Policy::Always;
    store.put_app(&app).unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Config::default()
            },
        )
        .unwrap();
    apply(&store, true).unwrap();
    let before: State = store.get("state").unwrap();
    assert_eq!(before.owned.len(), 1);
    let owned_path = before.owned[0].path.clone();
    let owned_bytes = std::fs::read(&owned_path).unwrap();

    // Count actual writes to the persisted state on the application's own WAL
    // database. The old implementation updated it on every idle reconciliation.
    store
        .db
        .execute_batch(
            "CREATE TABLE state_update_audit (id INTEGER);
             CREATE TRIGGER state_update_count AFTER UPDATE ON kv
             WHEN NEW.k='state'
             BEGIN INSERT INTO state_update_audit VALUES (1); END;",
        )
        .unwrap();
    for _ in 0..4 {
        reconcile_manual(&store).unwrap();
    }
    let updates: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM state_update_audit", [], |row| row.get(0))
        .unwrap();
    assert_eq!(updates, 0, "unchanged desktop must not rewrite its state");
    let still_owned: State = store.get("state").unwrap();
    assert_eq!(still_owned.owned, before.owned);
    assert_eq!(std::fs::read(&owned_path).unwrap(), owned_bytes);

    // A different process writing to the WAL must not block a read-only
    // reconciliation when no shortcuts or policies have changed.
    let second = rusqlite::Connection::open(home.join("usage.sqlite")).unwrap();
    second.execute_batch("BEGIN IMMEDIATE;").unwrap();
    reconcile_manual(&store).unwrap();
    second.execute_batch("ROLLBACK;").unwrap();
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);

    // A genuine manual deletion must still persist ownership release and
    // exclusion, rather than accidentally being skipped as a no-op.
    std::fs::remove_file(&owned_path).unwrap();
    reconcile_manual(&store).unwrap();
    let released: State = store.get("state").unwrap();
    assert!(released.owned.is_empty());
    assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
    let updates: i64 = store
        .db
        .query_row("SELECT COUNT(*) FROM state_update_audit", [], |row| row.get(0))
        .unwrap();
    assert_eq!(updates, 1, "real user changes still need one state write");
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);

    drop(second);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn repeating_a_monitor_status_does_not_write_during_unrelated_wal_work() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-unchanged-status-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    store.set("status", &"Учёт использования приостановлен").unwrap();
    store.db.execute_batch(
        "CREATE TABLE status_update_audit (id INTEGER);
         CREATE TRIGGER status_update_count AFTER UPDATE ON kv
         WHEN NEW.k='status'
         BEGIN INSERT INTO status_update_audit VALUES (1); END;",
    ).unwrap();
    for _ in 0..4 {
        assert!(!publish_monitor_status_if_changed(
            &store,
            "Учёт использования приостановлен"
        ).unwrap());
    }
    let other = rusqlite::Connection::open(home.join("usage.sqlite")).unwrap();
    other.execute_batch("BEGIN IMMEDIATE;").unwrap();
    assert!(!publish_monitor_status_if_changed(
        &store,
        "Учёт использования приостановлен"
    ).unwrap());
    other.execute_batch("ROLLBACK;").unwrap();
    assert!(publish_monitor_status_if_changed(&store, "Фоновый учёт работает").unwrap());
    assert!(!publish_monitor_status_if_changed(&store, "Фоновый учёт работает").unwrap());
    let changes: i64 = store.db.query_row(
        "SELECT COUNT(*) FROM status_update_audit",
        [],
        |row| row.get(0)
    ).unwrap();
    assert_eq!(changes, 1, "publish only the one real status transition");
    assert_eq!(store.get::<String>("status").unwrap(), "Фоновый учёт работает");
    drop(other);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
