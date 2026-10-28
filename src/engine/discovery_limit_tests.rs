use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Exercise actual Windows .url parsing and persisted SQLite discovery.
/// A bounded scan must not silently drop the last two thousand sources from a
/// documented 5,000-shortcut cold-start scenario.
#[test]
fn five_thousand_real_shortcuts_remain_discoverable_and_manual_rules_survive_rescan() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let _com = native::Com::new().unwrap();
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-large-discovery-{}-{}-{}",
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
                respect_taskbar_pins: false,
                ..Config::default()
            },
        )
        .unwrap();

    for index in 0..5_000 {
        let path = desktop.join(format!("App-{index:04}.url"));
        std::fs::write(
            path,
            format!("[InternetShortcut]\r\nURL=https://example.invalid/catalog/{index:04}\r\n"),
        )
        .unwrap();
    }

    refresh_apps(&store).unwrap();
    let catalog = store.apps().unwrap();
    assert_eq!(
        catalog.len(),
        5_000,
        "The normal discovery scan must not hide part of a 5,000-link folder",
    );
    assert!(
        store.get::<String>("catalog_warning").unwrap().is_empty(),
        "A supported 5,000-link folder must not trigger a truncation warning",
    );

    let rare = catalog
        .iter()
        .find(|a| a.target.ends_with("/4999"))
        .unwrap();
    let excluded = catalog
        .iter()
        .find(|a| a.target.ends_with("/0001"))
        .unwrap();
    let rare_source = rare.source.clone();
    let rare_original = std::fs::read(&rare_source).unwrap();
    store.policy(&rare.id, Policy::Always).unwrap();
    store.policy(&excluded.id, Policy::Never).unwrap();

    // The second real filesystem scan must not drop saved entries or overwrite
    // explicit choices; it must not rewrite the original shortcut bytes.
    refresh_apps(&store).unwrap();
    assert_eq!(store.apps().unwrap().len(), 5_000);
    assert_eq!(store.app(&rare.id).unwrap().policy, Policy::Always);
    assert_eq!(store.app(&excluded.id).unwrap().policy, Policy::Never);
    assert_eq!(std::fs::read(&rare_source).unwrap(), rare_original);

    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
