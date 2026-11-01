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
