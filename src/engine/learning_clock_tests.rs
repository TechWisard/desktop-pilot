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

    // Pausing collection cannot credit another sample or rewrite completion.
    store
        .set(
            "config",
            &Config {
                collect: false,
                ..config
            },
        )
        .unwrap();
    assert!(!record_learning_at(&store, 2, start + CalendarDuration::seconds(300)).unwrap());
    let unchanged: Progress = store.get("learning").unwrap();
    assert_eq!(unchanged.completed_at, progress.completed_at);
    assert_eq!(unchanged.day_seconds, progress.day_seconds);

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
