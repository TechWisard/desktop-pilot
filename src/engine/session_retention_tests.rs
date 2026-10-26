use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const DAY: i64 = 86_400;

fn with_store(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-usage-retention-test-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn checkpoints(store: &Store) -> HashMap<String, i64> {
    store.get("app_session_times").unwrap()
}

#[test]
fn forward_jump_usage_must_not_delete_other_apps_recent_checkpoints() {
    with_store(|store| {
        let at = now();
        let mut times = HashMap::from([
            ("rare-project".to_owned(), at - 30 * DAY),
            ("weekly-tool".to_owned(), at - 60 * DAY),
        ]);
        store.set("app_session_times", &times).unwrap();

        // The foreground tick really occurred, but the wall clock was abruptly
        // advanced. A different app must not lose 30- or 60-day-old evidence.
        record_usage(store, &mut times, "current-project", 12, true, at + 120 * DAY).unwrap();
        for name in ["rare-project", "weekly-tool"] {
            assert!(times.contains_key(name), "{name} was pruned from worker memory");
            assert!(checkpoints(store).contains_key(name), "{name} was erased from SQLite");
        }

        // After the clock returns, a new real visit still records correctly
        // without clearing the unaffected application's checkpoint.
        record_usage(store, &mut times, "returning-project", 12, true, at + 3600).unwrap();
        for name in ["rare-project", "weekly-tool", "returning-project"] {
            assert!(times.contains_key(name));
            assert!(checkpoints(store).contains_key(name));
        }
        let rows: i64 = store
            .db
            .query_row("SELECT COUNT(*) FROM days", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 2);
    });
}

#[test]
fn ordinary_housekeeping_still_prunes_expired_session_checkpoints() {
    with_store(|store| {
        let at = now();
        let times = HashMap::from([
            ("expired".to_owned(), at - 95 * DAY),
            ("recent".to_owned(), at - 30 * DAY),
        ]);
        store.set("app_session_times", &times).unwrap();
        store.housekeeping().unwrap();
        let kept = checkpoints(store);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept["recent"], times["recent"]);
    });
}
