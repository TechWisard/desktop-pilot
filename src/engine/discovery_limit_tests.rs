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

#[test]
fn nested_shortcut_budget_is_global_and_independent_of_creation_order() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    // The production visitor is run with a tiny limit so this regression
    // exercises the exact 6,000-entry boundary without creating thousands of
    // shortcuts on every CI run.
    let base = std::env::temp_dir().join(format!(
        "desktop-pilot-discovery-budget-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&base).unwrap();
    let mut previous = None;
    for reversed in [false, true] {
        let desktop = base.join(if reversed { "reverse" } else { "forward" });
        let mut relative = vec![
            "01-nested/c.url",
            "01-nested/a.url",
            "01-nested/b.url",
            "09-other/d.url",
            "z-root.url",
        ];
        if reversed {
            relative.reverse();
        }
        for name in relative {
            let file = desktop.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(
                &file,
                b"[InternetShortcut]\r\nURL=https://example.invalid/\r\n",
            )
            .unwrap();
        }
        let mut paths = Vec::new();
        let mut warnings = Vec::new();
        visit_shortcut_paths(&desktop, 0, &mut paths, &mut warnings, 2).unwrap();
        let selected: Vec<_> = paths
            .iter()
            .map(|path| path.strip_prefix(&desktop).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            selected,
            [
                PathBuf::from("01-nested/a.url"),
                PathBuf::from("01-nested/b.url"),
            ],
            "the nested budget must not allow a later root-level shortcut"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("limit reached (2)"));
        if let Some(previous) = &previous {
            assert_eq!(&selected, previous);
        }
        previous = Some(selected);

        // A larger budget still includes the next directory, but never
        // processes the remaining root-level entry once its nested child
        // reaches the limit.
        paths.clear();
        warnings.clear();
        visit_shortcut_paths(&desktop, 0, &mut paths, &mut warnings, 4).unwrap();
        assert_eq!(paths.len(), 4);
        assert!(
            paths
                .iter()
                .all(|path| path.file_name().unwrap() != "z-root.url")
        );
        assert_eq!(warnings.len(), 1);
    }
    std::fs::remove_dir_all(base).unwrap();
}
