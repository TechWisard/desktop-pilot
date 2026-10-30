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

fn at_discovery(callback: impl FnOnce(&Store) + 'static) {
    BEFORE_REFRESH_DISCOVERY.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(Box::new(callback));
    });
}

#[test]
fn a_new_quick_copy_during_scan_cannot_replace_its_offline_original() {
    with_profile(|store| {
        let mut original = url(store, &store.home, "Original", "Work");
        original.policy = Policy::Always;
        original.note = "Protected manually".into();
        store.put_app(&original).unwrap();
        store.observe(&original.id, 180, true, false).unwrap();
        let source = original.source.clone();
        let bytes = std::fs::read(&source).unwrap();
        let offline = store.home.join("Original.offline");
        let moved_to = offline.clone();
        at_discovery(move |store| {
            apply(store, true).unwrap();
            assert_eq!(store.get::<State>("state").unwrap().owned.len(), 1);
            std::fs::rename(&source, &moved_to).unwrap();
        });

        refresh_apps(store).unwrap();
        let saved = store.app(&original.id).unwrap();
        assert_eq!(
            saved.source, original.source,
            "A stale source scan adopted a newly managed desktop copy as its original"
        );
        assert!(!saved.valid, "An offline original must remain unavailable");
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(saved.note, original.note);
        let state: State = store.get("state").unwrap();
        assert_eq!(state.owned.len(), 1);
        let copy = state.owned[0].clone();
        assert_eq!(std::fs::read(&copy.path).unwrap(), bytes);
        assert!(copy.manual);
        let plan = preview(store, false).unwrap();
        assert!(plan.added.is_empty() && plan.removed.is_empty());
        assert_eq!(store.apps().unwrap().len(), 1);

        std::fs::rename(&offline, &original.source).unwrap();
        refresh_apps(store).unwrap();
        assert!(store.app(&original.id).unwrap().valid);
        rollback(store).unwrap();
        assert!(!Path::new(&copy.path).exists());
        assert_eq!(std::fs::read(&original.source).unwrap(), bytes);
        assert_eq!(store.app(&original.id).unwrap().policy, Policy::Always);
    });
}

#[test]
fn a_renamed_new_quick_copy_cannot_be_promoted_during_source_outage() {
    with_profile(|store| {
        let mut original = url(store, &store.home, "Original", "Work");
        original.policy = Policy::Always;
        original.note = "Preserve deliberate shortcut name".into();
        store.put_app(&original).unwrap();
        let original_path = original.source.clone();
        let original_bytes = std::fs::read(&original_path).unwrap();
        let renamed = root(store).unwrap().join("Renamed by user.url");
        let target = renamed.clone();
        let offline = store.home.join("Original.offline");
        let moved_to = offline.clone();
        at_discovery(move |store| {
            apply(store, true).unwrap();
            let owned: State = store.get("state").unwrap();
            assert_eq!(owned.owned.len(), 1);
            std::fs::rename(&owned.owned[0].path, &target).unwrap();
            std::fs::rename(&original_path, &moved_to).unwrap();
        });

        refresh_apps(store).unwrap();
        let saved = store.app(&original.id).unwrap();
        assert_eq!(
            saved.source, original.source,
            "Renaming after the ownership snapshot must not create a new original"
        );
        assert!(!saved.valid);
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(saved.note, original.note);
        assert_eq!(store.apps().unwrap().len(), 1);
        reconcile_manual(store).unwrap();
        let owned: State = store.get("state").unwrap();
        assert_eq!(owned.owned.len(), 1);
        assert_eq!(norm(&owned.owned[0].path), norm(&renamed.to_string_lossy()));
        assert!(owned.owned[0].manual);
        assert_eq!(std::fs::read(&renamed).unwrap(), original_bytes);

        std::fs::rename(&offline, &original.source).unwrap();
        refresh_apps(store).unwrap();
        rollback(store).unwrap();
        // The renamed shortcut is a user-controlled file. Undo intentionally
        // releases ownership without deleting the user's renamed copy.
        assert!(renamed.exists());
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(std::fs::read(&renamed).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&original.source).unwrap(), original_bytes);
    });
}

#[test]
fn independent_alias_is_still_allowed_after_a_concurrent_manual_placement() {
    with_profile(|store| {
        let mut original = url(store, &store.home, "Original", "Work");
        original.policy = Policy::Always;
        store.put_app(&original).unwrap();
        let source = original.source.clone();
        let alias = root(store).unwrap().join("Independent alias.url");
        let copy_alias = alias.clone();
        let offline = store.home.join("Original.offline");
        let moved_to = offline.clone();
        at_discovery(move |store| {
            apply(store, true).unwrap();
            std::fs::copy(&source, &copy_alias).unwrap();
            std::fs::rename(&source, &moved_to).unwrap();
        });
        refresh_apps(store).unwrap();
        let saved = store.app(&original.id).unwrap();
        assert_eq!(norm(&saved.source), norm(&alias.to_string_lossy()));
        assert!(saved.valid);
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(store.apps().unwrap().len(), 1);
        let owned: State = store.get("state").unwrap();
        assert_eq!(owned.owned.len(), 1);
        assert_ne!(file_identity(&alias).unwrap(), owned.owned[0].file_id);
        rollback(store).unwrap();
        assert!(alias.exists() && offline.exists());
    });
}

#[test]
fn same_source_manual_reimport_cannot_lose_newer_metadata_to_stale_scan() {
    with_profile(|store| {
        let source = store.home.join("Custom.url");
        let original_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Work\r\nIconFile=C:\\Icons\\old.ico\r\n";
        let new_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Work\r\nIconFile=C:\\Icons\\new.ico\r\n";
        std::fs::write(&source, original_bytes).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        app.note = "Keep this shortcut and position".into();
        store.put_app(&app).unwrap();
        store.observe(&app.id, 180, true, false).unwrap();
        apply(store, true).unwrap();

        let before: State = store.get("state").unwrap();
        let history = serde_json::to_value(store.days().unwrap()).unwrap();
        let copy = before.owned[0].clone();
        let copy_bytes = std::fs::read(&copy.path).unwrap();

        let changed_source = source.clone();
        at_commit(move |store| {
            // The real Settings import replaces the catalog row and advances
            // catalog_epoch. Reproduce those same durable writes after this
            // scan read the old original, but before its SQLite commit.
            std::fs::write(&changed_source, new_bytes).unwrap();
            let mut imported = native::read_link(&changed_source).unwrap();
            let saved = store.app(&imported.id).unwrap();
            imported.policy = saved.policy;
            imported.note = saved.note;
            imported.category = saved.category;
            imported.seed = saved.seed;
            imported.seed_at = saved.seed_at;
            imported.pinned = saved.pinned;
            imported.observed_roots = saved.observed_roots;
            store
                .transaction(|| {
                    store.put_app(&imported)?;
                    let epoch: u64 = store.get("catalog_epoch")?;
                    store.set("catalog_epoch", &(epoch + 1))
                })
                .unwrap();
        });

        refresh_apps(store).unwrap();
        let expected = native::read_link(&source).unwrap();
        let saved = store.app(&app.id).unwrap();
        assert_eq!(
            saved.icon, expected.icon,
            "an older scan must not revert a newer same-source import"
        );
        assert_eq!(saved.icon_index, expected.icon_index);
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(saved.note, app.note);
        assert_eq!(std::fs::read(&source).unwrap(), new_bytes);
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(store.get::<State>("state").unwrap().owned, before.owned);
        assert_eq!(std::fs::read(&copy.path).unwrap(), copy_bytes);

        refresh_apps(store).unwrap();
        assert_eq!(store.app(&app.id).unwrap().icon, expected.icon);
        rollback(store).unwrap();
        assert!(!Path::new(&copy.path).exists());
        assert_eq!(std::fs::read(&source).unwrap(), new_bytes);
    });
}

#[test]
fn concurrent_policy_only_edit_does_not_hide_verified_source_changes() {
    with_profile(|store| {
        let source = store.home.join("Verified.url");
        let old_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Stable\r\nIconFile=C:\\Icons\\old.ico\r\n";
        let new_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Stable\r\nIconFile=C:\\Icons\\new.ico\r\n";
        std::fs::write(&source, old_bytes).unwrap();
        let app = native::read_link(&source).unwrap();
        store.put_app(&app).unwrap();
        std::fs::write(&source, new_bytes).unwrap();

        let id = app.id.clone();
        at_commit(move |store| {
            store.policy(&id, Policy::Never).unwrap();
        });
        refresh_apps(store).unwrap();
        let saved = store.app(&app.id).unwrap();
        let actual = native::read_link(&source).unwrap();
        assert_eq!(saved.icon, actual.icon);
        assert_eq!(saved.policy, Policy::Never);
        assert_eq!(std::fs::read(&source).unwrap(), new_bytes);
    });
}
