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
fn backward_clock_step_preserves_one_fresh_session_and_unrelated_history() {
    with_store(|store| {
        let at = now();
        let mut sessions = HashMap::from([
            ("rare-project".to_owned(), at + 4 * 3600),
            ("other-project".to_owned(), at - 600),
        ]);
        store.set("app_session_times", &sessions).unwrap();

        // A large backward clock correction may have happened while the
        // monitor was stopped. The first independently confirmed return
        // starts one session, not zero or several.
        record_usage(store, &mut sessions, "rare-project", 12, true, at).unwrap();
        let counted = |id: &str| -> i64 {
            store
                .db
                .query_row(
                    "SELECT COALESCE(SUM(sessions),0) FROM days WHERE app=?1",
                    [id],
                    |row| row.get(0),
                )
                .unwrap()
        };
        assert_eq!(counted("rare-project"), 1);
        assert_eq!(sessions["rare-project"], at);
        assert_eq!(sessions["other-project"], at - 600);
        assert_eq!(checkpoints(store)["other-project"], at - 600);

        // A quick visit to a different project must keep its previously earned
        // session checkpoint: the clock correction in the first application
        // does not manufacture evidence for a second.
        record_usage(store, &mut sessions, "other-project", 12, true, at + 30).unwrap();
        assert_eq!(counted("other-project"), 0);
        assert_eq!(checkpoints(store)["rare-project"], at);

        // The normal 20-minute gap applies after the correction. One brief
        // switch is not a second session; the later confirmed visit is.
        record_usage(store, &mut sessions, "rare-project", 12, true, at + 600).unwrap();
        assert_eq!(counted("rare-project"), 1);
        record_usage(store, &mut sessions, "rare-project", 12, true, at + 1_861).unwrap();
        assert_eq!(counted("rare-project"), 2);
        assert_eq!(counted("other-project"), 0);
        assert_eq!(checkpoints(store)["rare-project"], at + 1_861);
    });
}

#[test]
fn foreground_usage_keeps_sampled_local_day_across_midnight() {
    with_store(|store| {
        // Reproduce a delayed foreground write on the production SQLite path.
        // The sample belongs to yesterday even if the writer runs today.
        let today = chrono::Local::now().date_naive();
        let yesterday = today.pred_opt().unwrap();
        let last_yesterday = (0..24)
            .rev()
            .find_map(|hour| {
                yesterday
                    .and_hms_opt(hour, 59, 58)?
                    .and_local_timezone(chrono::Local)
                    .latest()
            })
            .unwrap();
        let first_today = (0..24)
            .find_map(|hour| {
                today
                    .and_hms_opt(hour, 0, 2)?
                    .and_local_timezone(chrono::Local)
                    .earliest()
            })
            .unwrap();
        let day_start = |day: chrono::NaiveDate| {
            (0..24)
                .find_map(|hour| {
                    day.and_hms_opt(hour, 0, 0)?
                        .and_local_timezone(chrono::Local)
                        .earliest()
                })
                .unwrap()
                .timestamp()
        };

        let mut sessions = HashMap::new();
        record_usage(
            store,
            &mut sessions,
            "overnight-editor",
            12,
            true,
            last_yesterday.timestamp(),
        )
        .unwrap();
        record_usage(
            store,
            &mut sessions,
            "overnight-editor",
            12,
            false,
            first_today.timestamp(),
        )
        .unwrap();

        let mut query = store
            .db
            .prepare(
                "SELECT date,sessions,seconds FROM days WHERE app='overnight-editor' ORDER BY date",
            )
            .unwrap();
        let rows: Vec<(i64, i64, i64)> = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![(day_start(yesterday), 1, 12), (day_start(today), 0, 12)]
        );
        assert_eq!(
            checkpoints(store)["overnight-editor"],
            first_today.timestamp()
        );
    });
}
