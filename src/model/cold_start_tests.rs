use super::*;
use crate::{engine, store::Store};
use std::sync::atomic::{AtomicU64, Ordering};

fn app(id: &str, seed: f64, time: i64) -> App {
    App {
        id: id.into(),
        name: id.into(),
        seed,
        seed_at: if seed > 0.0 { time } else { 0 },
        valid: true,
        ..Default::default()
    }
}

fn day(id: &str, time: i64, sessions: u32, seconds: u32) -> Day {
    Day {
        app: id.into(),
        date: time,
        sessions,
        seconds,
        ..Default::default()
    }
}

#[test]
fn one_short_first_visit_is_not_a_confident_automatic_recommendation() {
    let time = 100 * 86400 + 12 * 3600;
    let candidate = app("incidental", 0.0, time);
    let cfg = Config::default();
    let evidence = [day("incidental", time - 12 * 3600, 1, 12)];
    let ranking = rank(&[candidate], &evidence, &cfg, time);
    assert!(ranking[0].score > 0.0);
    assert!(!ranking[0].eligible);
    assert!(
        select(
            &[app("incidental", 0.0, time)],
            &ranking,
            &State::default(),
            &cfg,
            true
        )
        .is_empty()
    );
}

#[test]
fn a_weak_legacy_seed_alone_does_not_fill_a_new_slot() {
    let time = 100 * 86400 + 12 * 3600;
    let candidate = app("old-signal", 1.0, time);
    let cfg = Config::default();
    let ranking = rank(&[candidate.clone()], &[], &cfg, time);
    assert!(!ranking[0].eligible);
    assert!(select(&[candidate], &ranking, &State::default(), &cfg, true).is_empty());
}

#[test]
fn repeated_short_visits_or_one_long_work_session_still_qualify() {
    let time = 100 * 86400 + 12 * 3600;
    let cfg = Config::default();
    let apps = [app("repeated", 0.0, time), app("focused", 0.0, time)];
    let days = [
        day("repeated", time - 12 * 3600, 2, 80),
        day("focused", time - 12 * 3600, 1, 3600),
    ];
    let ranking = rank(&apps, &days, &cfg, time);
    assert!(ranking.iter().all(|candidate| candidate.eligible));
    let selected = select(&apps, &ranking, &State::default(), &cfg, true);
    assert_eq!(selected.len(), 2);
}

#[test]
fn manual_rare_app_and_explicit_limits_take_precedence() {
    let time = 100 * 86400 + 12 * 3600;
    let mut rare = app("rare", 0.0, time);
    rare.policy = Policy::Always;
    let mut blocked = app("blocked", 100.0, time);
    blocked.policy = Policy::Never;
    let mut pinned = app("pinned", 100.0, time);
    pinned.pinned = true;
    let apps = [rare, blocked, pinned];
    let cfg = Config {
        slots: 2,
        ..Config::default()
    };
    let ranking = rank(&apps, &[], &cfg, time);
    assert_eq!(
        select(&apps, &ranking, &State::default(), &cfg, true),
        ["rare"]
    );
    let cfg = Config { min_days: 3, ..cfg };
    let regular = app("regular", 0.0, time);
    let evidence = [day("regular", time - 12 * 3600, 12, 3600)];
    assert!(!rank(&[regular], &evidence, &cfg, time)[0].eligible);
}

fn with_profile(test: impl FnOnce(&Store, &App, &std::path::Path)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-cold-start-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = root.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&root).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                slots: 2,
                min_days: 1,
                learning_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    let original = root.join("Original.url");
    std::fs::write(
        &original,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Catalog\r\n",
    )
    .unwrap();
    let app = crate::native::read_link(&original).unwrap();
    store.put_app(&app).unwrap();
    test(&store, &app, &desktop);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn actual_store_preview_apply_and_undo_wait_for_repeated_demand() {
    with_profile(|store, app, desktop| {
        let original = std::fs::read(&app.source).unwrap();
        store.observe(&app.id, 12, true, false).unwrap();
        let early = engine::preview(store, true).unwrap();
        assert!(
            early.selected.is_empty(),
            "one incidental visit must not occupy a desktop slot"
        );
        engine::apply(store, true).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert!(!store.get::<State>("state").unwrap().initialized);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 0);

        store.observe(&app.id, 180, true, false).unwrap();
        let meaningful = engine::preview(store, true).unwrap();
        assert_eq!(meaningful.added, [app.id.clone()]);
        engine::apply(store, true).unwrap();
        let before = store.get::<State>("state").unwrap();
        assert_eq!(before.owned.len(), 1);
        let quick = &before.owned[0];
        assert_eq!(std::fs::read(&quick.path).unwrap(), original);
        assert_eq!(std::fs::read(&app.source).unwrap(), original);

        for _ in 0..3 {
            let preview = engine::preview(store, false).unwrap();
            assert!(preview.added.is_empty() && preview.removed.is_empty());
            engine::apply(store, false).unwrap();
            let after = store.get::<State>("state").unwrap();
            assert_eq!(after.owned[0].path, quick.path);
            assert_eq!(after.owned[0].position, quick.position);
            assert_eq!(after.owned[0].file_id, quick.file_id);
        }
        engine::command(store, "rollback").unwrap();
        assert!(!std::path::Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), original);
    });
}

#[test]
fn an_explicit_manual_choice_bypasses_the_cold_start_gate() {
    with_profile(|store, app, _desktop| {
        store.policy(&app.id, Policy::Always).unwrap();
        let choice = engine::preview(store, true).unwrap();
        assert_eq!(choice.added, [app.id.clone()]);
        engine::apply(store, true).unwrap();
        assert_eq!(store.get::<State>("state").unwrap().owned.len(), 1);
        engine::command(store, "rollback").unwrap();
        assert!(std::path::Path::new(&app.source).exists());
    });
}
