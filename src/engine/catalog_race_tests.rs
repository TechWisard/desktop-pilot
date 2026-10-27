use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-scan-race-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
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
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn url(store: &Store, folder: &Path, name: &str, target: &str) -> App {
    let path = folder.join(format!("{name}.url"));
    std::fs::write(
        &path,
        format!("[InternetShortcut]\r\nURL=https://example.invalid/{target}\r\n"),
    )
    .unwrap();
    let app = native::read_link(&path).unwrap();
    assert!(app.valid);
    assert!(path.starts_with(&store.home));
    app
}

fn at_commit(callback: impl FnOnce(&Store) + 'static) {
    BEFORE_REFRESH_COMMIT.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(callback));
    });
}

#[test]
fn a_source_imported_while_scanning_keeps_its_new_path_and_manual_copy() {
    with_profile(|store| {
        let mut original = url(store, &store.home, "Original", "Work");
        original.policy = Policy::Always;
        original.note = "Keep work profile".into();
        store.put_app(&original).unwrap();
        store.observe(&original.id, 120, true, false).unwrap();
        apply(store, true).unwrap();
        let before: State = store.get("state").unwrap();
        let history = serde_json::to_value(store.days().unwrap()).unwrap();
        let copy = before.owned[0].clone();
        let quick_bytes = std::fs::read(&copy.path).unwrap();
        let replacement = url(store, &store.home, "Preferred original", "Work");
        assert_eq!(original.id, replacement.id);
        let selected_path = replacement.source.clone();
        at_commit(move |store| {
            let mut imported = replacement;
            imported.policy = Policy::Always;
            imported.note = "Explicitly selected source".into();
            store.put_app(&imported).unwrap();
        });
        refresh_apps(store).unwrap();
        let saved = store.app(&original.id).unwrap();
        assert_eq!(saved.source, selected_path);
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(saved.note, "Explicitly selected source");
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(
            serde_json::to_value(store.get::<State>("state").unwrap()).unwrap(),
            serde_json::to_value(&before).unwrap()
        );
        assert_eq!(std::fs::read(&copy.path).unwrap(), quick_bytes);
        let plan = preview(store, false).unwrap();
        assert!(plan.added.is_empty() && plan.removed.is_empty());
        refresh_apps(store).unwrap();
        assert_eq!(store.app(&original.id).unwrap().source, selected_path);
        rollback(store).unwrap();
        assert!(!Path::new(&copy.path).exists());
        assert!(Path::new(&original.source).exists());
        assert!(Path::new(&selected_path).exists());
    });
}

#[test]
fn a_first_manual_import_wins_over_the_same_app_discovered_during_scan() {
    with_profile(|store| {
        let desktop = root(store).unwrap();
        let discovered = url(store, &desktop, "Discovered", "Special");
        let mut imported = url(store, &store.home, "Chosen source", "Special");
        imported.policy = Policy::Never;
        imported.note = "Never auto-place this profile".into();
        assert_eq!(discovered.id, imported.id);
        let selected_source = imported.source.clone();
        at_commit(move |store| {
            store.put_app(&imported).unwrap();
        });
        refresh_apps(store).unwrap();
        let saved = store.app(&discovered.id).unwrap();
        assert_eq!(saved.source, selected_source);
        assert_eq!(saved.policy, Policy::Never);
        assert_eq!(saved.note, "Never auto-place this profile");
        assert_eq!(store.apps().unwrap().len(), 1);
        let plan = preview(store, true).unwrap();
        assert!(plan.selected.is_empty());
        assert!(plan.added.is_empty() && plan.removed.is_empty());
        refresh_apps(store).unwrap();
        assert_eq!(store.app(&discovered.id).unwrap().source, selected_source);
    });
}

#[test]
fn a_transaction_started_during_scan_blocks_stale_catalog_changes() {
    with_profile(|store| {
        let source = store.home.join("Original.url");
        let old = b"[InternetShortcut]\r\nURL=https://example.invalid/Stable\r\nIconFile=C:\\Icons\\old.ico\r\n";
        std::fs::write(&source, old).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        std::fs::write(
            &source,
            b"[InternetShortcut]\r\nURL=https://example.invalid/Stable\r\nIconFile=C:\\Icons\\new.ico\r\n",
        )
        .unwrap();
        at_commit(|store| {
            store
                .db
                .execute(
                    "INSERT INTO transactions(id,at,status,data) VALUES('interrupted-scan-test',0,'prepared','{}')",
                    [],
                )
                .unwrap();
        });
        let before = store.app(&app.id).unwrap();
        let changed = refresh_apps(store);
        assert!(
            changed.is_err(),
            "A newly prepared placement must block the commit"
        );
        assert_eq!(
            serde_json::to_value(store.app(&app.id).unwrap()).unwrap(),
            serde_json::to_value(before).unwrap()
        );
        assert!(
            store
                .db
                .query_row(
                    "SELECT COUNT(*) FROM transactions WHERE status='prepared'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap()
                > 0
        );
        assert!(Path::new(&app.source).exists());
    });
}
