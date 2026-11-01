use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

fn first_qualified_sample(usage: &mut UsageAccumulator, app: &str) -> (String, u32, bool) {
    // The first poll after changing applications is deliberately ignored.
    assert!(usage.sample(Some(app), 2.0).is_none());
    for _ in 0..4 {
        assert!(usage.sample(Some(app), 2.0).is_none());
    }
    usage.sample(Some(app), 2.0).unwrap()
}

#[test]
fn paused_observation_cannot_consume_first_session_or_replay_old_activity() {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-paused-usage-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    let mut cfg = Config {
        collect: true,
        learning_enabled: false,
        ..Default::default()
    };
    store.set("config", &cfg).unwrap();
    store.set("background_disabled", &false).unwrap();
    store.set("shutdown", &false).unwrap();

    let mut usage = UsageAccumulator::default();
    let mut checkpoints = HashMap::new();

    // Settings pauses collection between qualifying input and its SQLite write.
    let (id, seconds, first) = first_qualified_sample(&mut usage, "editor");
    assert_eq!((seconds, first), (10, true));
    cfg.collect = false;
    store.set("config", &cfg).unwrap();
    assert!(
        !persist_confirmed_usage(
            &store,
            &mut checkpoints,
            &mut usage,
            &id,
            seconds,
            first,
            10_000,
        )
        .unwrap()
    );
    assert!(store.days().unwrap().is_empty());
    assert!(checkpoints.is_empty());
    assert!(
        usage.windows.is_empty(),
        "a refused sample cannot survive a pause"
    );

    // Re-enabling collection does not replay the old ten seconds, but a new
    // confirmed visit earns its own first session using the real Store writer.
    cfg.collect = true;
    store.set("config", &cfg).unwrap();
    let (id, seconds, first) = first_qualified_sample(&mut usage, "editor");
    assert_eq!((seconds, first), (10, true));
    assert!(
        persist_confirmed_usage(
            &store,
            &mut checkpoints,
            &mut usage,
            &id,
            seconds,
            first,
            10_020,
        )
        .unwrap()
    );
    assert_eq!(checkpoints.get("editor"), Some(&10_020));
    let days = store.days().unwrap();
    let editor = days.iter().find(|d| d.app == "editor").unwrap();
    assert_eq!((editor.seconds, editor.sessions), (10, 1));

    // A second project is independent. Its denied first visit must not erase
    // committed editor history or inflate the second project's session count.
    usage.sample(None, 2.0);
    let (id, seconds, first) = first_qualified_sample(&mut usage, "browser");
    cfg.collect = false;
    store.set("config", &cfg).unwrap();
    assert!(
        !persist_confirmed_usage(
            &store,
            &mut checkpoints,
            &mut usage,
            &id,
            seconds,
            first,
            10_030,
        )
        .unwrap()
    );
    assert_eq!(store.days().unwrap().len(), 1);
    assert_eq!(checkpoints.get("editor"), Some(&10_020));
    assert!(!checkpoints.contains_key("browser"));

    cfg.collect = true;
    store.set("config", &cfg).unwrap();
    let (id, seconds, first) = first_qualified_sample(&mut usage, "browser");
    assert!(first);
    persist_confirmed_usage(
        &store,
        &mut checkpoints,
        &mut usage,
        &id,
        seconds,
        first,
        10_050,
    )
    .unwrap();
    let days = store.days().unwrap();
    assert_eq!(days.len(), 2);
    for app in ["editor", "browser"] {
        let day = days.iter().find(|d| d.app == app).unwrap();
        assert_eq!((day.seconds, day.sessions), (10, 1));
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn temporary_sqlite_writer_lock_drops_uncommitted_samples_then_recovers_both_projects() {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-transient-writer-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                learning_enabled: true,
                ..Default::default()
            },
        )
        .unwrap();
    store
        .db
        .busy_timeout(std::time::Duration::from_millis(1))
        .unwrap();
    let mut usage = UsageAccumulator::default();
    let mut sessions = HashMap::new();

    // A different process (backup/Settings) briefly holds SQLite's only writer.
    // The sampler has already advanced its in-memory first-session marker.
    let (id, seconds, first) = first_qualified_sample(&mut usage, "editor");
    assert_eq!((seconds, first), (10, true));
    let blocker = rusqlite::Connection::open(root.join("usage.sqlite")).unwrap();
    blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
    assert!(
        !persist_confirmed_usage(
            &store,
            &mut sessions,
            &mut usage,
            &id,
            seconds,
            first,
            10_000,
        )
        .unwrap(),
        "a transient lock must not terminate foreground collection"
    );
    assert!(
        !record_learning_if_permitted(&store, 2).unwrap(),
        "a transient lock must not terminate learning either"
    );
    assert!(usage.windows.is_empty());
    assert!(sessions.is_empty());
    assert!(store.days().unwrap().is_empty());
    assert_eq!(store.get::<Progress>("learning").unwrap().started_at, 0);

    blocker.execute_batch("ROLLBACK;").unwrap();
    drop(blocker);
    // Old, uncommitted input must never be replayed. Fresh engagement can
    // still earn the application's first session after the lock is released.
    let (id, seconds, first) = first_qualified_sample(&mut usage, "editor");
    assert_eq!((seconds, first), (10, true));
    assert!(
        persist_confirmed_usage(
            &store,
            &mut sessions,
            &mut usage,
            &id,
            seconds,
            first,
            10_020,
        )
        .unwrap()
    );
    assert!(record_learning_if_permitted(&store, 2).unwrap());
    let mut progress: Progress = store.get("learning").unwrap();
    assert_eq!(progress.day_seconds.values().sum::<u32>(), 2);
    progress.completed_at = now();
    store.set("learning", &progress).unwrap();
    assert!(!record_learning_if_permitted(&store, 2).unwrap());

    // Learning completion must not stop usage collection. A separate project
    // earns one independent session. Returning quickly
    // to the editor must not invent a second session or shift its checkpoint.
    usage.sample(None, 2.0);
    let (id, seconds, first) = first_qualified_sample(&mut usage, "browser");
    assert_eq!((seconds, first), (10, true));
    assert!(
        persist_confirmed_usage(
            &store,
            &mut sessions,
            &mut usage,
            &id,
            seconds,
            first,
            10_040,
        )
        .unwrap()
    );
    usage.sample(None, 2.0);
    // Returning within the accumulator's short-visit memory is *not* a new
    // first-qualified visit: each confirmed 2-second poll can contribute a
    // continuation delta. Persist each production sample instead of expecting
    // five silent polls as on the first-ever visit.
    assert!(usage.sample(Some("editor"), 2.0).is_none());
    let mut return_seconds = 0;
    for poll in 1..=5 {
        let (id, seconds, first) = usage.sample(Some("editor"), 2.0).unwrap();
        assert_eq!(id, "editor");
        assert_eq!((seconds, first), (2, false));
        assert!(
            persist_confirmed_usage(
                &store,
                &mut sessions,
                &mut usage,
                &id,
                seconds,
                first,
                10_050 + poll * 2,
            )
            .unwrap()
        );
        return_seconds += seconds;
    }
    assert_eq!(return_seconds, 10);
    let days = store.days().unwrap();
    assert_eq!(days.len(), 2);
    let editor = days.iter().find(|day| day.app == "editor").unwrap();
    let browser = days.iter().find(|day| day.app == "browser").unwrap();
    assert_eq!((editor.seconds, editor.sessions), (20, 1));
    assert_eq!((browser.seconds, browser.sessions), (10, 1));
    assert_eq!(sessions["editor"], 10_060);
    assert_eq!(sessions["browser"], 10_040);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn corrupt_settings_errors_are_not_treated_as_transient_writer_locks() {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-invalid-evidence-settings-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    store.set("config", &"not a config").unwrap();
    let mut usage = UsageAccumulator::default();
    let mut sessions = HashMap::new();
    let (id, seconds, first) = first_qualified_sample(&mut usage, "editor");
    assert!(
        persist_confirmed_usage(
            &store,
            &mut sessions,
            &mut usage,
            &id,
            seconds,
            first,
            10_000,
        )
        .is_err()
    );
    assert!(record_learning_if_permitted(&store, 2).is_err());
    assert!(store.days().unwrap().is_empty());
    assert!(sessions.is_empty());
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
