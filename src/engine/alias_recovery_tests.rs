use super::*;
use std::os::windows::fs::OpenOptionsExt;

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-alias-recovery-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    store.set("test_desktop", &desktop.to_string_lossy()).unwrap();
    store.set("config", &Config {
        slots: 1,
        learning_enabled: false,
        respect_taskbar_pins: false,
        ..Default::default()
    }).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn original(store: &Store) -> App {
    let path = store.home.join("original.url");
    std::fs::write(&path, b"[InternetShortcut]\r\nURL=https://example.invalid/Workspace\r\n").unwrap();
    let mut app = native::read_link(&path).unwrap();
    app.policy = Policy::Always;
    app.seed = 17.0;
    app.seed_at = now();
    app.note = "Keep my deliberate choice".into();
    store.put_app(&app).unwrap();
    store.observe(&app.id, 90, true, false).unwrap();
    app
}

fn alias(store: &Store, app: &App, name: &str) -> App {
    let path = root(store).unwrap().join(name);
    std::fs::copy(&app.source, &path).unwrap();
    native::read_link(&path).unwrap()
}

fn deny_read(path: &str) -> std::fs::File {
    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).unwrap()
}

fn assert_user_data(store: &Store, before: &App, days: &serde_json::Value) {
    let app = store.app(&before.id).unwrap();
    assert_eq!(app.policy, before.policy);
    assert_eq!(app.seed, before.seed);
    assert_eq!(app.seed_at, before.seed_at);
    assert_eq!(app.note, before.note);
    assert_eq!(app.identity(), before.identity());
    assert_eq!(&serde_json::to_value(store.days().unwrap()).unwrap(), days);
}

#[test]
fn locked_source_recovers_via_alias_and_keeps_history_through_release() {
    with_profile(|store| {
        let app = original(store);
        let bytes = std::fs::read(&app.source).unwrap();
        let first = alias(store, &app, "a-copy.url");
        let second = alias(store, &app, "z-copy.url");
        let days = serde_json::to_value(store.days().unwrap()).unwrap();
        let config = store.get::<Config>("config").unwrap();
        let locked = deny_read(&app.source);
        assert!(native::read_link(Path::new(&app.source)).is_err());
        refresh_apps(store).unwrap();
        assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&first.source));
        assert_eq!(store.apps().unwrap().len(), 1);
        assert_user_data(store, &app, &days);
        apply(store, true).unwrap();
        let placed = store.get::<State>("state").unwrap().owned;
        assert_eq!(placed.len(), 1);
        assert_eq!(std::fs::read(&placed[0].path).unwrap(), bytes);
        let checkpoint = serde_json::to_value(&placed).unwrap();
        drop(locked);
        for _ in 0..5 {
            refresh_apps(store).unwrap();
            let proposed = preview(store, false).unwrap();
            assert!(proposed.added.is_empty() && proposed.removed.is_empty());
            apply(store, false).unwrap();
            assert_eq!(serde_json::to_value(store.get::<State>("state").unwrap().owned).unwrap(), checkpoint);
            assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&first.source));
            assert_user_data(store, &app, &days);
        }
        assert_eq!(store.get::<Config>("config").unwrap(), config);
        command(store, &format!("policy:{}:Never", app.id)).unwrap();
        refresh_apps(store).unwrap();
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
        apply(store, false).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert!(!Path::new(&placed[0].path).exists());
        for source in [&app.source, &first.source, &second.source] {
            assert_eq!(std::fs::read(source).unwrap(), bytes);
        }
        assert_eq!(serde_json::to_value(store.days().unwrap()).unwrap(), days);
    });
}

#[test]
fn partially_written_source_recovers_without_repairing_original_bytes() {
    with_profile(|store| {
        let app = original(store);
        let copy = alias(store, &app, "readable-copy.url");
        let bytes = std::fs::read(&copy.source).unwrap();
        let days = serde_json::to_value(store.days().unwrap()).unwrap();
        let partial = b"[InternetShortcut]\r\n; writer has not finished\r\n";
        std::fs::write(&app.source, partial).unwrap();
        assert!(native::read_link(Path::new(&app.source)).is_err());
        refresh_apps(store).unwrap();
        assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&copy.source));
        apply(store, true).unwrap();
        let state: State = store.get("state").unwrap();
        assert_eq!(state.owned.len(), 1);
        assert_eq!(std::fs::read(&state.owned[0].path).unwrap(), bytes);
        assert_eq!(std::fs::read(&app.source).unwrap(), partial);
        assert_user_data(store, &app, &days);
    });
}

#[test]
fn an_existing_copy_is_not_recreated_when_its_source_alias_changes() {
    with_profile(|store| {
        let app = original(store);
        apply(store, true).unwrap();
        let before: State = store.get("state").unwrap();
        let bytes = std::fs::read(&before.owned[0].path).unwrap();
        let copy = alias(store, &app, "alternate.url");
        let locked = deny_read(&app.source);
        refresh_apps(store).unwrap();
        assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&copy.source));
        let proposed = preview(store, false).unwrap();
        assert!(proposed.added.is_empty() && proposed.removed.is_empty());
        apply(store, false).unwrap();
        assert_eq!(serde_json::to_value(store.get::<State>("state").unwrap().owned).unwrap(), serde_json::to_value(&before.owned).unwrap());
        assert_eq!(std::fs::read(&before.owned[0].path).unwrap(), bytes);
        drop(locked);
    });
}

#[test]
fn a_healthy_source_does_not_flip_to_an_earlier_sorted_duplicate() {
    with_profile(|store| {
        let app = original(store);
        alias(store, &app, "a-earlier-copy.url");
        let days = serde_json::to_value(store.days().unwrap()).unwrap();
        for _ in 0..5 {
            refresh_apps(store).unwrap();
            assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&app.source));
            assert_eq!(store.apps().unwrap().len(), 1);
            assert_user_data(store, &app, &days);
        }
    });
}

#[test]
fn different_launch_arguments_are_not_a_recovery_alias() {
    let _com = native::Com::new().unwrap();
    with_profile(|store| {
        let executable = store.home.join("inert-target.exe");
        std::fs::write(&executable, b"inert test fixture; never launched").unwrap();
        let source = store.home.join("Work.lnk");
        let other = root(store).unwrap().join("Personal.lnk");
        native::make_link(&source, &executable, "--profile work", "", 0, "work").unwrap();
        native::make_link(&other, &executable, "--profile personal", "", 0, "personal").unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        let personal = native::read_link(&other).unwrap();
        assert_ne!(app.identity(), personal.identity());
        let locked = deny_read(&app.source);
        refresh_apps(store).unwrap();
        assert_eq!(norm(&store.app(&app.id).unwrap().source), norm(&app.source));
        assert_eq!(store.apps().unwrap().len(), 2);
        assert_eq!(store.app(&personal.id).unwrap().policy, Policy::Auto);
        assert!(apply(store, true).is_err());
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        drop(locked);
        apply(store, true).unwrap();
        let state: State = store.get("state").unwrap();
        assert_eq!(state.owned.len(), 1);
        assert_eq!(native::read_link(Path::new(&state.owned[0].path)).unwrap().arguments, "--profile work");
        assert!(source.exists() && other.exists());
    });
}
