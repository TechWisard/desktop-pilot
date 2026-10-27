use super::*;
use crate::{
    engine,
    model::{App, Config, Policy, State},
};
use std::sync::atomic::{AtomicU64, Ordering};

fn with_catalog(size: usize, test: impl FnOnce(&Store, &App)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-noop-catalog-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let desktop = home.join("Desktop");
    let sources = desktop.join("Projects");
    std::fs::create_dir_all(&sources).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    let sources = desktop.join("Projects");
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy().to_string())
        .unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                collect: true,
                slots: 2,
                ..Default::default()
            },
        )
        .unwrap();
    for index in 0..size {
        let path = sources.join(format!("Project-{index:04}.url"));
        let contents = format!(
            "[InternetShortcut]\r\nURL=https://example.invalid/project/{index:04}\r\nIconFile=C:\\Icons\\old.ico\r\nIconIndex=1\r\n"
        );
        std::fs::write(path, contents).unwrap();
    }
    engine::refresh_apps(&store).unwrap();
    let apps = store.apps().unwrap();
    assert_eq!(apps.len(), size);
    let important = apps[0].clone();
    store.policy(&important.id, Policy::Always).unwrap();
    store.observe(&important.id, 120, true, false).unwrap();
    engine::apply(&store, true).unwrap();
    test(&store, &important);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn writes(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn repeated_unchanged_real_shortcut_rescans_do_not_rewrite_the_catalog() {
    with_catalog(512, |store, important| {
        let catalog_before = serde_json::to_value(store.apps().unwrap()).unwrap();
        let days_before = serde_json::to_value(store.days().unwrap()).unwrap();
        let state_before: State = store.get("state").unwrap();
        assert_eq!(state_before.owned.len(), 1);
        let quick = state_before.owned[0].clone();
        let original_bytes = std::fs::read(&important.source).unwrap();
        let quick_bytes = std::fs::read(&quick.path).unwrap();
        let before = writes(store);
        for _ in 0..5 {
            engine::refresh_apps(store).unwrap();
            let plan = engine::preview(store, false).unwrap();
            assert!(plan.added.is_empty() && plan.removed.is_empty());
        }
        assert_eq!(
            writes(store) - before,
            0,
            "No-op rescans must not write SQLite rows"
        );
        assert_eq!(
            serde_json::to_value(store.apps().unwrap()).unwrap(),
            catalog_before
        );
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            days_before
        );
        assert_eq!(
            serde_json::to_value(store.get::<State>("state").unwrap()).unwrap(),
            serde_json::to_value(state_before).unwrap()
        );
        assert_eq!(std::fs::read(&important.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&quick.path).unwrap(), quick_bytes);
        engine::rollback(store).unwrap();
        assert!(std::path::Path::new(&important.source).exists());
        assert!(!std::path::Path::new(&quick.path).exists());
    });
}

#[test]
fn one_updated_icon_writes_only_its_catalog_row_and_preserves_undo() {
    with_catalog(64, |store, important| {
        engine::refresh_apps(store).unwrap();
        let state_before: State = store.get("state").unwrap();
        let quick = state_before.owned[0].clone();
        let quick_bytes = std::fs::read(&quick.path).unwrap();
        let days_before = serde_json::to_value(store.days().unwrap()).unwrap();
        let original = std::fs::read_to_string(&important.source).unwrap();
        let changed = original.replace(
            "IconFile=C:\\Icons\\old.ico\r\nIconIndex=1",
            "IconFile=C:\\Icons\\updated.ico\r\nIconIndex=7",
        );
        assert_ne!(changed, original);
        std::fs::write(&important.source, changed).unwrap();
        let before = writes(store);
        engine::refresh_apps(store).unwrap();
        assert_eq!(
            writes(store) - before,
            1,
            "Only the changed app row should be written"
        );
        let updated = store.app(&important.id).unwrap();
        assert_eq!(updated.icon, r"C:\Icons\updated.ico");
        assert_eq!(updated.icon_index, 7);
        assert_eq!(updated.policy, Policy::Always);
        assert_eq!(updated.source, important.source);
        assert_eq!(updated.id, important.id);
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            days_before
        );
        assert_eq!(
            serde_json::to_value(store.get::<State>("state").unwrap()).unwrap(),
            serde_json::to_value(state_before).unwrap()
        );
        assert_eq!(std::fs::read(&quick.path).unwrap(), quick_bytes);
        assert!(engine::preview(store, false).unwrap().added.is_empty());
        let again = writes(store);
        engine::refresh_apps(store).unwrap();
        assert_eq!(writes(store) - again, 0);
        engine::rollback(store).unwrap();
        assert!(!std::path::Path::new(&quick.path).exists());
        assert!(std::path::Path::new(&important.source).exists());
    });
}

#[test]
fn vanished_source_has_one_transition_then_stays_quiet_until_recovery() {
    with_catalog(32, |store, important| {
        engine::refresh_apps(store).unwrap();
        let original = std::fs::read(&important.source).unwrap();
        let owned: State = store.get("state").unwrap();
        let quick = owned.owned[0].clone();
        let quick_bytes = std::fs::read(&quick.path).unwrap();
        std::fs::remove_file(&important.source).unwrap();
        engine::refresh_apps(store).unwrap();
        let missing = store.app(&important.id).unwrap();
        assert!(!missing.valid && missing.unavailable_since > 0);
        assert_eq!(missing.policy, Policy::Always);
        let quiet = writes(store);
        for _ in 0..3 {
            engine::refresh_apps(store).unwrap();
        }
        assert_eq!(
            writes(store) - quiet,
            0,
            "An unchanged outage is not a new change"
        );
        assert_eq!(std::fs::read(&quick.path).unwrap(), quick_bytes);
        assert_eq!(
            store.app(&important.id).unwrap().unavailable_since,
            missing.unavailable_since
        );
        std::fs::write(&important.source, original).unwrap();
        engine::refresh_apps(store).unwrap();
        assert!(store.app(&important.id).unwrap().valid);
        let recovered = writes(store);
        engine::refresh_apps(store).unwrap();
        assert_eq!(writes(store) - recovered, 0);
        assert!(engine::preview(store, false).unwrap().added.is_empty());
        engine::rollback(store).unwrap();
        assert!(!std::path::Path::new(&quick.path).exists());
        assert!(std::path::Path::new(&important.source).exists());
    });
}
