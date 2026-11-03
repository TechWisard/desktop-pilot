use super::*;
use chrono::{Duration as CalendarDuration, FixedOffset, TimeZone};

#[test]
fn confirmed_learning_across_midnight_persists_one_consistent_clock_sample() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-learning-midnight-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&home).unwrap();
    let config = Config {
        collect: true,
        learning_sessions: 1,
        ..Config::default()
    };
    store.set("config", &config).unwrap();

    // Synthetic local time with an explicit offset makes a midnight boundary
    // deterministic without altering the runner's Windows system clock.
    // Each observation still enters the real Store transaction and Progress.
    let zone = FixedOffset::east_opt(0).unwrap();
    let start = zone
        .with_ymd_and_hms(2026, 11, 3, 23, 58, 0)
        .single()
        .unwrap();
    for tick in 0..150 {
        let observed = start + CalendarDuration::seconds(2 * tick);
        if tick == 60 {
            // The first sample of the new date must be discarded when
            // Settings has paused collection, even at the midnight boundary.
            store
                .set(
                    "config",
                    &Config {
                        collect: false,
                        ..config.clone()
                    },
                )
                .unwrap();
            assert!(!record_learning_at(&store, 2, observed).unwrap());
            let paused: Progress = store.get("learning").unwrap();
            assert_eq!(paused.session_seconds, 120);
            assert!(!paused.day_seconds.contains_key("2026-11-04"));
            store.set("config", &config).unwrap();
        }
        assert!(record_learning_at(&store, 2, observed).unwrap());
    }
    let progress: Progress = store.get("learning").unwrap();
    assert_eq!(progress.started_at, start.timestamp());
    assert_eq!(progress.last_active_at, start.timestamp() + 298);
    assert_eq!(progress.completed_at, progress.last_active_at);
    assert_eq!(progress.session_seconds, 300);
    assert_eq!(progress.sessions, 1);
    assert_eq!(progress.day_seconds.get("2026-11-03"), Some(&120));
    assert_eq!(progress.day_seconds.get("2026-11-04"), Some(&180));
    assert!(progress.ready(&config));
    assert!(store.get::<State>("state").unwrap().owned.is_empty());

    drop(store);
    for attempt in 0..5 {
        match std::fs::remove_dir_all(&home) {
            Ok(()) => break,
            Err(_) if attempt < 4 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => panic!("Could not remove test profile: {error}"),
        }
    }
}

#[test]
fn completed_learning_never_needs_a_sqlite_writer_even_during_a_foreground_sample() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-completed-learning-readonly-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = Store::open(&home).unwrap();
    let config = Config {
        collect: true,
        learning_sessions: 1,
        ..Config::default()
    };
    store.set("config", &config).unwrap();
    let completed = Progress {
        started_at: 10_000,
        completed_at: 10_300,
        sessions: 1,
        ..Progress::default()
    };
    store.set("learning", &completed).unwrap();
    let observed = FixedOffset::east_opt(0)
        .unwrap()
        .with_ymd_and_hms(2026, 11, 3, 12, 0, 0)
        .single()
        .unwrap();

    // SQLite's query_only connection can read this saved profile, but BEGIN
    // IMMEDIATE would fail with SQLITE_READONLY. A completed gate must not
    // take that writer path, even if new confirmed input keeps arriving.
    store.db.execute_batch("PRAGMA query_only=ON").unwrap();
    for _ in 0..3 {
        assert!(!record_learning_at(&store, 2, observed).unwrap());
    }
    let unchanged: Progress = store.get("learning").unwrap();
    assert_eq!(unchanged.completed_at, completed.completed_at);
    assert_eq!(unchanged.sessions, completed.sessions);
    assert_eq!(unchanged.started_at, completed.started_at);

    // Do not accidentally apply the same no-write shortcut to an unfinished
    // gate: it still needs the transactional permission and persistence check.
    store.db.execute_batch("PRAGMA query_only=OFF").unwrap();
    store.set("learning", &Progress::default()).unwrap();
    store.db.execute_batch("PRAGMA query_only=ON").unwrap();
    assert!(record_learning_at(&store, 2, observed).is_err());
    assert_eq!(store.get::<Progress>("learning").unwrap().started_at, 0);
    store.db.execute_batch("PRAGMA query_only=OFF").unwrap();
    assert!(record_learning_at(&store, 2, observed).unwrap());
    let started: Progress = store.get("learning").unwrap();
    assert_eq!(started.started_at, observed.timestamp());
    assert_eq!(started.session_seconds, 2);
    assert_eq!(started.sessions, 0);
    assert_eq!(started.day_seconds.get("2026-11-03"), Some(&2));

    drop(store);
    for attempt in 0..5 {
        match std::fs::remove_dir_all(&home) {
            Ok(()) => break,
            Err(_) if attempt < 4 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => panic!("Could not remove test profile: {error}"),
        }
    }
}
