use super::import_ownership_tests::with_profile;
use super::*;

fn total_changes(settings: &Settings) -> i64 {
    settings
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn clearing_large_unseeded_catalog_does_not_rewrite_every_shortcut() {
    with_profile(|settings, anchor| {
        let originals = settings.store.home.join("Many originals");
        std::fs::create_dir(&originals).unwrap();
        let mut seeded_id = String::new();
        let mut excluded_id = String::new();
        for i in 0..1_000 {
            let path = originals.join(format!("App-{i:04}.url"));
            std::fs::write(
                &path,
                format!("[InternetShortcut]\r\nURL=https://example.invalid/Project/{i}\r\n"),
            )
            .unwrap();
            let mut app = native::read_link(&path).unwrap();
            if i == 19 {
                app.seed = 8.0;
                app.seed_at = now();
                seeded_id = app.id.clone();
            }
            if i == 37 {
                app.policy = Policy::Never;
                app.note = "A deliberate exclusion".into();
                excluded_id = app.id.clone();
            }
            settings.store.put_app(&app).unwrap();
        }
        let before_state = serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap();
        let before_config = settings.store.get::<Config>("config").unwrap();
        let anchor_original = std::fs::read(&anchor.source).unwrap();
        let owned = settings.store.get::<State>("state").unwrap().owned[0].clone();
        let owned_original = std::fs::read(&owned.path).unwrap();
        let excluded = settings.store.app(&excluded_id).unwrap();
        assert!(!settings.store.days().unwrap().is_empty());

        let before = total_changes(settings);
        engine::command(&settings.store, "clear_stats").unwrap();
        let first_writes = total_changes(settings) - before;
        assert!(
            first_writes <= 12,
            "Clearing 1,001 saved shortcuts wrote {first_writes} rows instead of only changed history and seeds"
        );
        assert!(settings.store.days().unwrap().is_empty());
        assert_eq!(settings.store.apps().unwrap().len(), 1_001);
        assert_eq!(settings.store.app(&excluded_id).unwrap(), excluded);
        assert_eq!(settings.store.get::<Config>("config").unwrap(), before_config);
        assert_eq!(
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
            before_state
        );
        for id in [&anchor.id, &seeded_id] {
            let app = settings.store.app(id).unwrap();
            assert_eq!((app.seed, app.seed_at), (0.0, 0));
        }
        let current_anchor = settings.store.app(&anchor.id).unwrap();
        assert_eq!(current_anchor.policy, Policy::Always);
        assert_eq!(current_anchor.note, anchor.note);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), anchor_original);
        assert_eq!(std::fs::read(&owned.path).unwrap(), owned_original);

        let after_first = total_changes(settings);
        engine::command(&settings.store, "clear_stats").unwrap();
        let repeat_writes = total_changes(settings) - after_first;
        assert!(
            repeat_writes <= 4,
            "Repeating Clear history wrote {repeat_writes} rows even though no app has a seed"
        );
        assert_eq!(settings.store.app(&excluded_id).unwrap(), excluded);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), anchor_original);
        assert_eq!(
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
            before_state
        );
        let preview = engine::preview(&settings.store, false).unwrap();
        assert!(preview.added.is_empty() && preview.removed.is_empty());
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!std::path::Path::new(&owned.path).exists());
        assert_eq!(std::fs::read(&anchor.source).unwrap(), anchor_original);
    });
}

#[test]
fn failed_clear_history_keeps_existing_usage_and_allows_safe_retry() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let owned = settings.store.get::<State>("state").unwrap().owned[0].clone();
        let before_state = serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap();
        let before_config = settings.store.get::<Config>("config").unwrap();
        let before_days = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let before_app = settings.store.app(&anchor.id).unwrap();
        settings.store.db.execute_batch(&format!(
            "CREATE TRIGGER block_history_clear BEFORE UPDATE ON apps WHEN NEW.id='{}' BEGIN SELECT RAISE(ABORT, 'forced app update failure'); END;",
            anchor.id
        )).unwrap();

        assert!(engine::command(&settings.store, "clear_stats").is_err());
        assert_eq!(serde_json::to_value(settings.store.days().unwrap()).unwrap(), before_days);
        assert_eq!(settings.store.app(&anchor.id).unwrap(), before_app);
        assert_eq!(
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
            before_state
        );
        assert_eq!(settings.store.get::<Config>("config").unwrap(), before_config);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        assert!(std::path::Path::new(&owned.path).is_file());

        settings.store.db.execute_batch("DROP TRIGGER block_history_clear;").unwrap();
        engine::command(&settings.store, "clear_stats").unwrap();
        assert!(settings.store.days().unwrap().is_empty());
        assert_eq!(settings.store.app(&anchor.id).unwrap().policy, Policy::Always);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!std::path::Path::new(&owned.path).exists());
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}
