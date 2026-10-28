use super::import_ownership_tests::with_profile;
use super::*;

fn import_through_settings(settings: &mut Settings, path: &Path) {
    let ctx = egui::Context::default();
    settings.add_shortcuts(vec![path.to_path_buf()]);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        settings.poll_jobs(&ctx);
        if settings.job.is_none() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "real asynchronous import did not complete"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        settings.import_error.is_empty(),
        "unexpected import error: {}",
        settings.import_error
    );
    assert_eq!(settings.app_filter, 5);
}

fn visible_last_import(settings: &mut Settings) -> Vec<String> {
    // The same cache updater invoked by the actual Apps screen.
    settings.catalog.update(
        &settings.apps,
        &settings.ranking,
        &settings.filter,
        settings.app_filter,
        settings.app_sort,
        &settings.highlighted_apps,
    );
    settings
        .catalog
        .indices
        .iter()
        .map(|&index| settings.apps[index].id.clone())
        .collect()
}

#[test]
fn consecutive_noop_imports_display_the_actual_last_selection_without_a_database_write() {
    with_profile(|settings, anchor| {
        let first = settings.store.home.join("Project one.url");
        let second = settings.store.home.join("Project two.url");
        std::fs::write(
            &first,
            b"[InternetShortcut]\r\nURL=https://example.invalid/project-one\r\n",
        )
        .unwrap();
        std::fs::write(
            &second,
            b"[InternetShortcut]\r\nURL=https://example.invalid/project-two\r\n",
        )
        .unwrap();
        let first_id = native::read_link(&first).unwrap().id;
        let second_id = native::read_link(&second).unwrap().id;
        let original = std::fs::read(&anchor.source).unwrap();
        let anchor_before = settings.store.app(&anchor.id).unwrap();
        let history_before = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let state_before =
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap();
        let quick_copy: PathBuf = settings.store.get::<State>("state").unwrap().owned[0]
            .path
            .clone()
            .into();

        import_through_settings(settings, &first);
        assert_eq!(visible_last_import(settings), vec![first_id.clone()]);
        import_through_settings(settings, &second);
        assert_eq!(visible_last_import(settings), vec![second_id.clone()]);

        let epoch: u64 = settings.store.get("catalog_epoch").unwrap();
        // Both are already in the database. Changing the "Last import" set
        // must still update the filtered view even though SQLite stays idle.
        import_through_settings(settings, &first);
        assert_eq!(settings.store.get::<u64>("catalog_epoch").unwrap(), epoch);
        assert_eq!(
            visible_last_import(settings),
            vec![first_id],
            "Last import shows a previous selection after a no-op import"
        );

        import_through_settings(settings, &quick_copy);
        assert_eq!(settings.store.get::<u64>("catalog_epoch").unwrap(), epoch);
        assert_eq!(
            visible_last_import(settings),
            vec![anchor.id.clone()],
            "Reimporting a managed quick copy must show its existing app"
        );
        assert_eq!(settings.store.app(&anchor.id).unwrap(), anchor_before);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history_before
        );
        assert_eq!(
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
            state_before
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        let preview = engine::preview(&settings.store, false).unwrap();
        assert!(preview.added.is_empty() && preview.removed.is_empty());
        engine::command(&settings.store, "rollback").unwrap();
        assert!(std::path::Path::new(&anchor.source).exists());
        assert!(!quick_copy.exists());
    });
}
