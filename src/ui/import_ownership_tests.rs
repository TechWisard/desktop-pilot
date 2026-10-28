use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&mut Settings, &App)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-import-ownership-{}-{}-{}",
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
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                slots: 3,
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Default::default()
            },
        )
        .unwrap();
    let source = home.join("Original.url");
    std::fs::write(
        &source,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Work\r\n",
    )
    .unwrap();
    let mut app = native::read_link(&source).unwrap();
    app.policy = Policy::Always;
    app.note = "My deliberate work shortcut".into();
    app.seed = 19.0;
    app.seed_at = now();
    store.put_app(&app).unwrap();
    store.observe(&app.id, 180, true, false).unwrap();
    engine::apply(&store, true).unwrap();
    drop(store);
    let mut settings = Settings::new(home.clone()).unwrap();
    test(&mut settings, &app);
    drop(settings);
    std::fs::remove_dir_all(home).unwrap();
}

// Exercise the actual asynchronous Settings import entry point, including its
// COM initialization, file parsing, SQLite transaction and production guards.
fn import(
    settings: &mut Settings,
    paths: Vec<PathBuf>,
) -> std::result::Result<ImportResult, String> {
    settings.add_shortcuts(paths);
    let job = settings.job.take().expect("import job must start");
    let result = job
        .receiver
        .recv_timeout(Duration::from_secs(15))
        .expect("import job must finish");
    match result? {
        JobResult::Added(result) => Ok(result),
        _ => panic!("unexpected import result"),
    }
}

fn owned(settings: &Settings) -> Owned {
    settings.store.get::<State>("state").unwrap().owned[0].clone()
}

fn data(settings: &Settings, app: &App) -> serde_json::Value {
    serde_json::json!({
        "app": settings.store.app(&app.id).unwrap(),
        "days": settings.store.days().unwrap(),
        "config": settings.store.get::<Config>("config").unwrap(),
        "owned": settings.store.get::<State>("state").unwrap().owned,
    })
}

#[test]
fn reimporting_a_quick_copy_preserves_the_original_rule_history_and_undo() {
    with_profile(|settings, app| {
        let before = data(settings, app);
        let quick = owned(settings);
        let bytes = std::fs::read(&app.source).unwrap();
        for _ in 0..5 {
            let result = import(
                settings,
                vec![quick.path.clone().into(), quick.path.clone().into()],
            )
            .unwrap();
            assert!(result.failed.is_empty());
            assert_eq!(result.added.len(), 1);
            assert_eq!(result.added[0].0, app.id);
            assert_eq!(
                data(settings, app),
                before,
                "Import must not turn a managed quick copy into an original"
            );
            engine::refresh_apps(&settings.store).unwrap();
            engine::reconcile_manual(&settings.store).unwrap();
            let preview = engine::preview(&settings.store, false).unwrap();
            assert!(preview.added.is_empty() && preview.removed.is_empty());
            engine::apply(&settings.store, false).unwrap();
            assert_eq!(data(settings, app), before);
            settings.reload().unwrap();
            assert!(settings.can_undo);
            assert_eq!(settings.apps.len(), 1);
            assert_eq!(std::fs::read(&quick.path).unwrap(), bytes);
        }
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), bytes);
        assert_eq!(settings.store.app(&app.id).unwrap().policy, Policy::Always);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            before["days"]
        );
    });
}

#[test]
fn reimporting_a_quick_copy_does_not_block_undo() {
    with_profile(|settings, app| {
        let quick = owned(settings);
        let bytes = std::fs::read(&app.source).unwrap();
        assert!(
            import(settings, vec![quick.path.clone().into()])
                .unwrap()
                .failed
                .is_empty()
        );
        let result = engine::command(&settings.store, "rollback");
        assert!(
            result.is_ok(),
            "Reimport must not make Undo reject its own copy: {result:?}"
        );
        assert!(!Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), bytes);
        assert!(
            settings
                .store
                .get::<State>("state")
                .unwrap()
                .owned
                .is_empty()
        );
    });
}

#[test]
fn renamed_quick_copy_is_recognized_before_reconciliation() {
    with_profile(|settings, app| {
        let before = data(settings, app);
        let quick = owned(settings);
        let renamed = engine::root(&settings.store)
            .unwrap()
            .join("My familiar name.url");
        std::fs::rename(&quick.path, &renamed).unwrap();
        let result = import(settings, vec![renamed.clone()]).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!(settings.store.app(&app.id).unwrap().source, app.source);
        engine::reconcile_manual(&settings.store).unwrap();
        let current = owned(settings);
        assert_eq!(norm(&current.path), norm(&renamed.to_string_lossy()));
        assert_eq!(current.file_id, quick.file_id);
        assert_eq!(current.position, quick.position);
        assert!(current.manual);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            before["days"]
        );
        engine::command(&settings.store, &format!("policy:{}:Never", app.id)).unwrap();
        engine::apply(&settings.store, false).unwrap();
        assert!(!renamed.exists());
        assert!(Path::new(&app.source).exists());
    });
}

#[test]
fn quick_copy_does_not_mask_a_missing_source_or_reverse_never() {
    with_profile(|settings, app| {
        let quick = owned(settings);
        let parked = settings.store.home.join("Original.offline");
        std::fs::rename(&app.source, &parked).unwrap();
        engine::refresh_apps(&settings.store).unwrap();
        let before = data(settings, app);
        assert!(!settings.store.app(&app.id).unwrap().valid);
        let result = import(settings, vec![quick.path.clone().into()]).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!(
            data(settings, app),
            before,
            "Import must not mask an unavailable original"
        );
        std::fs::rename(&parked, &app.source).unwrap();
        engine::refresh_apps(&settings.store).unwrap();
        engine::command(&settings.store, &format!("policy:{}:Never", app.id)).unwrap();
        let excluded = data(settings, app);
        assert!(
            import(settings, vec![quick.path.clone().into()])
                .unwrap()
                .failed
                .is_empty()
        );
        assert_eq!(data(settings, app), excluded);
        engine::apply(&settings.store, false).unwrap();
        assert!(!Path::new(&quick.path).exists());
        assert!(Path::new(&app.source).exists());
        assert_eq!(settings.store.app(&app.id).unwrap().policy, Policy::Never);
    });
}

#[test]
fn changed_managed_copy_is_rejected_without_blocking_other_imports() {
    with_profile(|settings, app| {
        let quick = owned(settings);
        let changed = b"[InternetShortcut]\r\nURL=https://example.invalid/ChangedByUser\r\n";
        std::fs::write(&quick.path, changed).unwrap();
        let other = settings.store.home.join("New original.url");
        std::fs::write(
            &other,
            b"[InternetShortcut]\r\nURL=https://example.invalid/Other\r\n",
        )
        .unwrap();
        let expected = native::read_link(&other).unwrap();
        let result = import(settings, vec![quick.path.clone().into(), other.clone()]).unwrap();
        assert_eq!(result.failed.len(), 1);
        assert!(
            result.failed[0]
                .1
                .contains("This quick shortcut was changed")
        );
        assert_eq!(result.added.len(), 1);
        assert_eq!(result.added[0].0, expected.id);
        assert_eq!(settings.store.apps().unwrap().len(), 2);
        assert_eq!(settings.store.app(&app.id).unwrap().source, app.source);
        engine::reconcile_manual(&settings.store).unwrap();
        assert!(
            settings
                .store
                .get::<State>("state")
                .unwrap()
                .owned
                .is_empty()
        );
        assert_eq!(std::fs::read(&quick.path).unwrap(), changed);
        assert!(Path::new(&app.source).exists() && other.exists());
    });
}

#[test]
fn changing_only_a_quick_copys_icon_cannot_be_silently_reimported() {
    // Both the original path and a just-renamed copy must be recognized as
    // managed even when their launch target and arguments are unchanged.
    for renamed in [false, true] {
        with_profile(|settings, app| {
            let copy = owned(settings);
            let original_bytes = std::fs::read(&app.source).unwrap();
            let before = data(settings, app);
            let changed = b"[InternetShortcut]\r\nURL=https://example.invalid/Work\r\nIconFile=C:\\Icons\\Personal.ico\r\nIconIndex=4\r\n";
            let changed_path = if renamed {
                let path = settings.store.home.join("Desktop").join("My icon.url");
                std::fs::rename(&copy.path, &path).unwrap();
                path
            } else {
                PathBuf::from(&copy.path)
            };
            std::fs::write(&changed_path, changed).unwrap();
            let parsed = native::read_link(&changed_path).unwrap();
            assert_eq!(
                parsed.identity(),
                app.identity(),
                "This fixture changes icon bytes, not launch identity"
            );
            let other = settings.store.home.join("Unrelated original.url");
            std::fs::write(
                &other,
                b"[InternetShortcut]\r\nURL=https://example.invalid/Unrelated\r\n",
            )
            .unwrap();
            let unrelated = native::read_link(&other).unwrap();

            let result = import(settings, vec![changed_path.clone(), other.clone()]).unwrap();
            assert_eq!(
                result.failed.len(),
                1,
                "Edited managed shortcut must not be accepted"
            );
            assert_eq!(
                norm(&result.failed[0].0.to_string_lossy()),
                norm(&changed_path.to_string_lossy())
            );
            assert!(
                result.failed[0]
                    .1
                    .contains("This quick shortcut was changed")
            );
            assert_eq!(
                result.added.len(),
                1,
                "Unrelated valid import must still work"
            );
            assert_eq!(result.added[0].0, unrelated.id);
            assert_eq!(data(settings, app), before);
            assert_eq!(settings.store.apps().unwrap().len(), 2);
            assert_eq!(std::fs::read(&app.source).unwrap(), original_bytes);
            assert_eq!(std::fs::read(&changed_path).unwrap(), changed);

            // The existing reconciliation is still responsible for releasing
            // a manually edited copy, never for overwriting the user's bytes.
            engine::reconcile_manual(&settings.store).unwrap();
            assert!(
                settings
                    .store
                    .get::<State>("state")
                    .unwrap()
                    .owned
                    .is_empty()
            );
            assert_eq!(settings.store.app(&app.id).unwrap().policy, Policy::Never);
            assert_eq!(std::fs::read(&changed_path).unwrap(), changed);
            assert_eq!(std::fs::read(&app.source).unwrap(), original_bytes);
        });
    }
}

#[test]
fn unfinished_changes_block_import_without_mutating_the_catalog() {
    with_profile(|settings, app| {
        settings
            .store
            .db
            .execute("UPDATE transactions SET status='prepared'", [])
            .unwrap();
        let before = data(settings, app);
        let other = settings.store.home.join("Unrelated original.url");
        let bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Unrelated\r\n";
        std::fs::write(&other, bytes).unwrap();
        let quick = owned(settings);
        let result = import(settings, vec![other.clone(), quick.path.clone().into()]);
        assert!(
            result.is_err(),
            "Import must wait for recovery of an unfinished change"
        );
        assert_eq!(data(settings, app), before);
        assert_eq!(settings.store.apps().unwrap().len(), 1);
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&other).unwrap(), bytes);
        assert!(Path::new(&app.source).exists());
    });
}

#[test]
fn ordinary_duplicate_import_still_refreshes_the_source() {
    with_profile(|settings, app| {
        let quick = owned(settings);
        let duplicate = settings.store.home.join("Replacement original.url");
        std::fs::copy(&app.source, &duplicate).unwrap();
        let result = import(settings, vec![duplicate.clone()]).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!(result.added.len(), 1);
        assert_eq!(
            norm(&settings.store.app(&app.id).unwrap().source),
            norm(&duplicate.to_string_lossy())
        );
        assert_eq!(settings.store.app(&app.id).unwrap().policy, Policy::Always);
        engine::reconcile_manual(&settings.store).unwrap();
        assert_eq!(owned(settings).file_id, quick.file_id);
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!Path::new(&quick.path).exists());
        assert!(duplicate.exists() && Path::new(&app.source).exists());
    });
}

#[test]
fn mixed_original_and_quick_copy_imports_are_order_independent() {
    with_profile(|settings, app| {
        let quick = owned(settings);
        let before = data(settings, app);
        for paths in [
            vec![app.source.clone().into(), quick.path.clone().into()],
            vec![quick.path.clone().into(), app.source.clone().into()],
        ] {
            let result = import(settings, paths).unwrap();
            assert!(result.failed.is_empty());
            assert_eq!(result.added.len(), 1);
            assert_eq!(data(settings, app), before);
        }
    });
}

#[test]
fn distinct_launch_profiles_remain_separate_during_import() {
    let _com = native::Com::new().unwrap();
    with_profile(|settings, app| {
        let executable = settings.store.home.join("inert.exe");
        std::fs::write(&executable, b"inert test fixture; never executed").unwrap();
        let work = settings.store.home.join("Work.lnk");
        let personal = settings.store.home.join("Personal.lnk");
        native::make_link(&work, &executable, "--profile work", "", 0, "work").unwrap();
        native::make_link(
            &personal,
            &executable,
            "--profile personal",
            "",
            0,
            "personal",
        )
        .unwrap();
        let work_app = native::read_link(&work).unwrap();
        let personal_app = native::read_link(&personal).unwrap();
        let result = import(settings, vec![work, personal]).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!(result.added.len(), 2);
        assert_ne!(work_app.id, personal_app.id);
        assert_eq!(settings.store.apps().unwrap().len(), 3);
        assert_eq!(
            settings.store.app(&work_app.id).unwrap().arguments,
            "--profile work"
        );
        assert_eq!(
            settings.store.app(&personal_app.id).unwrap().arguments,
            "--profile personal"
        );
        assert_eq!(settings.store.app(&app.id).unwrap().source, app.source);
    });
}

#[test]
fn importing_a_new_source_signals_the_live_catalog_once_without_reimport_noise() {
    with_profile(|settings, _app| {
        let before: u64 = settings.store.get("catalog_epoch").unwrap();
        let source = settings.store.home.join("Fresh project.url");
        std::fs::write(
            &source,
            b"[InternetShortcut]\r\nURL=https://example.invalid/FreshProject\r\n",
        )
        .unwrap();
        let added = import(settings, vec![source]).unwrap();
        assert!(added.failed.is_empty());
        assert_eq!(added.added.len(), 1);
        let changed: u64 = settings.store.get("catalog_epoch").unwrap();
        assert_eq!(changed, before + 1);

        // Re-selecting an unchanged managed copy does not change the catalog.
        let copy = owned(settings);
        let repeated = import(settings, vec![copy.path.into()]).unwrap();
        assert!(repeated.failed.is_empty());
        assert_eq!(settings.store.get::<u64>("catalog_epoch").unwrap(), changed);
    });
}
