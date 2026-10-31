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
        record_usage(
            store,
            &mut times,
            "current-project",
            12,
            true,
            at + 120 * DAY,
        )
        .unwrap();
        for name in ["rare-project", "weekly-tool"] {
            assert!(
                times.contains_key(name),
                "{name} was pruned from worker memory"
            );
            assert!(
                checkpoints(store).contains_key(name),
                "{name} was erased from SQLite"
            );
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

#[test]
fn correcting_the_clock_does_not_inflate_a_short_repeat_visit() {
    with_store(|store| {
        let at = now();
        let mut sessions = HashMap::from([("rare".to_owned(), at - 600)]);
        store.set("app_session_times", &sessions).unwrap();

        // A large forward adjustment occurs while using a different app.
        record_usage(store, &mut sessions, "other", 12, true, at + 120 * DAY).unwrap();
        // Fifteen minutes after the last real visit, the original app returns.
        // It must not be counted as a new session just because the other app
        // observed a bad system clock in between.
        record_usage(store, &mut sessions, "rare", 12, true, at + 300).unwrap();
        let counted: i64 = store
            .db
            .query_row(
                "SELECT COALESCE(SUM(sessions),0) FROM days WHERE app='rare'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(counted, 0);
        assert!(checkpoints(store).contains_key("rare"));
    });
}

#[test]
fn periodic_worker_housekeeping_refreshes_its_session_cache() {
    with_store(|store| {
        let at = now();
        let mut sessions = HashMap::from([
            ("expired".to_owned(), at - 95 * DAY),
            ("recent".to_owned(), at - 30 * DAY),
        ]);
        store.set("app_session_times", &sessions).unwrap();
        housekeeping_sessions(store, &mut sessions).unwrap();
        assert_eq!(sessions, checkpoints(store));
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions["recent"], at - 30 * DAY);
    });
}

#[test]
fn backward_clock_step_does_not_manufacture_a_new_session_for_the_same_app() {
    with_store(|store| {
        let at = now();
        let mut sessions = HashMap::from([("rare-project".to_owned(), at + 4 * 3600)]);
        store.set("app_session_times", &sessions).unwrap();

        // The previous observed wall time is four hours ahead because Windows
        // corrected its clock backwards. A fresh confirmed focus visit is not
        // proof that twenty minutes of real inactivity have elapsed.
        record_usage(store, &mut sessions, "rare-project", 12, true, at).unwrap();
        let counted = || -> i64 {
            store
                .db
                .query_row(
                    "SELECT COALESCE(SUM(sessions),0) FROM days WHERE app='rare-project'",
                    [],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert_eq!(
            counted(),
            0,
            "a clock rewind must not create a second session or inflate ranking"
        );
        assert_eq!(sessions["rare-project"], at);
        assert_eq!(checkpoints(store)["rare-project"], at);

        // Once the new wall-clock baseline advances, the ordinary inactivity
        // gate must still work: a ten-minute return is not a second session,
        // whereas the first confirmed visit after 21 minutes is.
        record_usage(store, &mut sessions, "rare-project", 12, true, at + 600).unwrap();
        assert_eq!(counted(), 0);
        record_usage(store, &mut sessions, "rare-project", 12, true, at + 1_861).unwrap();
        assert_eq!(counted(), 1);
        assert_eq!(checkpoints(store)["rare-project"], at + 1_861);
    });
}
