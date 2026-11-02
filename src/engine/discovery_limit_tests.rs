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
fn direct_desktop_shortcuts_win_the_shared_budget_independent_of_creation_order() {
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
                PathBuf::from("z-root.url"),
                PathBuf::from("01-nested/a.url"),
            ],
            "a large nested project folder must not hide a direct desktop shortcut"
        );
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("limit reached (2)"));
        if let Some(previous) = &previous {
            assert_eq!(&selected, previous);
        }
        previous = Some(selected);

        // Even with more capacity, direct shortcuts retain priority and
        // the nested directory remains alphabetically ordered.
        paths.clear();
        warnings.clear();
        visit_shortcut_paths(&desktop, 0, &mut paths, &mut warnings, 4).unwrap();
        let selected: Vec<_> = paths
            .iter()
            .map(|path| path.strip_prefix(&desktop).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            selected,
            [
                PathBuf::from("z-root.url"),
                PathBuf::from("01-nested/a.url"),
                PathBuf::from("01-nested/b.url"),
                PathBuf::from("01-nested/c.url"),
            ]
        );
        assert_eq!(warnings.len(), 1);
    }
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn unrelated_files_do_not_multiply_directory_metadata_work() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-filtered-discovery-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    // This folder intentionally ends in .url: it is a directory, not a URL
    // file, and must still be traversed after early filename filtering.
    let nested = root.join("Archive.url");
    std::fs::create_dir_all(&nested).unwrap();
    for i in 0..512 {
        std::fs::write(root.join(format!("notes-{i:04}.txt")), b"unrelated").unwrap();
        std::fs::write(nested.join(format!("asset-{i:04}.png")), b"unrelated").unwrap();
    }
    let contents = b"[InternetShortcut]\r\nURL=https://example.invalid/valid\r\n";
    std::fs::write(root.join("Visible.URL"), contents).unwrap();
    std::fs::write(nested.join("Nested.lnk"), contents).unwrap();

    DISCOVERY_METADATA_LOOKUPS.with(|calls| calls.set(0));
    let mut paths = Vec::new();
    let mut warnings = Vec::new();
    visit_shortcut_paths(&root, 0, &mut paths, &mut warnings, 6_000).unwrap();
    let selected: Vec<_> = paths
        .iter()
        .map(|path| path.strip_prefix(&root).unwrap().to_path_buf())
        .collect();
    assert_eq!(
        selected,
        [
            PathBuf::from("Visible.URL"),
            PathBuf::from("Archive.url").join("Nested.lnk"),
        ]
    );
    assert!(warnings.is_empty());
    // The directory and two candidate files need actual reparse-point and
    // regular-file metadata checks. The 1,024 unrelated files do not.
    DISCOVERY_METADATA_LOOKUPS.with(|calls| assert_eq!(calls.get(), 3));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_known_folder_roots_cannot_starve_shortcuts_in_later_roots() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let base = std::env::temp_dir().join(format!(
        "desktop-pilot-shared-roots-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let desktop = base.join("Desktop");
    let menu = base.join("StartMenu");
    std::fs::create_dir_all(&desktop).unwrap();
    std::fs::create_dir_all(&menu).unwrap();
    let visible = desktop.join("Visible.url");
    let important = menu.join("Important.url");
    let visible_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/visible\r\n";
    let important_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/important\r\n";
    std::fs::write(&visible, visible_bytes).unwrap();
    std::fs::write(&important, important_bytes).unwrap();

    // Before root deduplication, this fills the two-slot budget with the
    // same Desktop file twice; later Start Menu files disappear from results.
    let roots = [
        (desktop.clone(), "Desktop"),
        (desktop.clone(), "Desktop"),
        (menu.clone(), "Start menu"),
    ];
    let mut warnings = Vec::new();
    let discovered = discover_shortcut_roots(&roots, &mut warnings, 2).unwrap();
    assert_eq!(discovered.len(), 2);
    assert!(discovered.contains(&visible));
    assert!(discovered.contains(&important));
    assert_eq!(warnings.len(), 1, "The real budget remains enforced");

    warnings.clear();
    let distinct_roots = [(desktop.clone(), "Desktop"), (menu.clone(), "Start menu")];
    assert_eq!(
        discover_shortcut_roots(&distinct_roots, &mut warnings, 2).unwrap(),
        discovered
    );
    assert_eq!(
        std::fs::read(&visible).unwrap().as_slice(),
        &visible_bytes[..]
    );
    assert_eq!(
        std::fs::read(&important).unwrap().as_slice(),
        &important_bytes[..]
    );
    std::fs::remove_dir_all(base).unwrap();
}
