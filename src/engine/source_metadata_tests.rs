use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-metadata-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let store = Store::open(&home).unwrap();
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Default::default()
            },
        )
        .unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn check_refresh_without_replacing_quick_copy(
    store: &Store,
    app: &App,
    expected_icon: &str,
    expected_index: i32,
) {
    let state_before: State = store.get("state").unwrap();
    assert_eq!(state_before.owned.len(), 1);
    let copy = &state_before.owned[0];
    let copy_bytes = std::fs::read(&copy.path).unwrap();
    let usage_before = serde_json::to_value(store.days().unwrap()).unwrap();
    refresh_apps(store).unwrap();
    let refreshed = store.app(&app.id).unwrap();
    assert_eq!(refreshed.icon, expected_icon);
    assert_eq!(refreshed.icon_index, expected_index);
    assert_eq!(refreshed.source, app.source);
    assert_eq!(refreshed.target, app.target);
    assert_eq!(refreshed.arguments, app.arguments);
    assert_eq!(refreshed.policy, Policy::Always);
    assert_eq!(refreshed.note, app.note);
    assert_eq!(refreshed.seed, app.seed);
    assert_eq!(refreshed.seed_at, app.seed_at);
    assert_eq!(
        serde_json::to_value(store.days().unwrap()).unwrap(),
        usage_before
    );
    assert_eq!(
        serde_json::to_value(store.get::<State>("state").unwrap()).unwrap(),
        serde_json::to_value(&state_before).unwrap(),
        "Changing the original shortcut's icon must not move or claim its quick copy"
    );
    assert_eq!(std::fs::read(&copy.path).unwrap(), copy_bytes);
    let plan = preview(store, false).unwrap();
    assert!(plan.added.is_empty() && plan.removed.is_empty());
    rollback(store).unwrap();
    assert!(!Path::new(&copy.path).exists());
    assert!(Path::new(&app.source).exists());
}

#[test]
fn changing_existing_internet_shortcut_icon_refreshes_catalog_but_not_manual_choices() {
    with_profile(|store| {
        let source = store.home.join("Research.url");
        std::fs::write(
            &source,
            b"[InternetShortcut]\r\nURL=https://example.invalid/Vault\r\nIconFile=C:\\Icons\\old.ico\r\nIconIndex=1\r\n",
        ).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        app.note = "Important work vault".into();
        app.seed = 17.0;
        app.seed_at = now();
        store.put_app(&app).unwrap();
        store.observe(&app.id, 120, true, false).unwrap();
        apply(store, true).unwrap();
        std::fs::write(
            &source,
            b"[InternetShortcut]\r\nURL=https://example.invalid/Vault\r\nIconFile=C:\\Icons\\new.ico\r\nIconIndex=8\r\n",
        ).unwrap();
        check_refresh_without_replacing_quick_copy(store, &app, r"C:\Icons\new.ico", 8);
    });
}

#[test]
fn changing_existing_windows_link_icon_refreshes_metadata_without_moving_its_copy() {
    let _com = native::Com::new().unwrap();
    with_profile(|store| {
        let executable = store.home.join("inert.exe");
        std::fs::write(&executable, b"test fixture only; never run").unwrap();
        let source = store.home.join("Editor.lnk");
        native::make_link(
            &source,
            &executable,
            "--profile work",
            r"C:\Icons\one.ico",
            2,
            "work",
        )
        .unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        app.note = "Separate working profile".into();
        app.seed = 13.0;
        app.seed_at = now();
        store.put_app(&app).unwrap();
        store.observe(&app.id, 60, true, false).unwrap();
        apply(store, true).unwrap();
        native::make_link(
            &source,
            &executable,
            "--profile work",
            r"C:\Icons\two.ico",
            4,
            "work",
        )
        .unwrap();
        check_refresh_without_replacing_quick_copy(store, &app, r"C:\Icons\two.ico", 4);
    });
}
