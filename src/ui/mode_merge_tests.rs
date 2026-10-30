use super::import_ownership_tests::with_profile;
use super::*;

// Mode changes are one user decision, even though the persisted representation
// has two booleans. Concurrent saves must not splice two different decisions.
#[test]
fn competing_mode_choices_cannot_synthesize_an_unrequested_mode() {
    let base = Config {
        automatic: false,
        collect: true,
        learning_enabled: true,
        ..Config::default()
    };
    let mut manual = base.clone();
    apply_mode(&mut manual, OperatingMode::Manual);
    let mut adaptive = base.clone();
    apply_mode(&mut adaptive, OperatingMode::Adaptive);

    assert!(
        merge_config(&base, &manual, &adaptive).is_err(),
        "a manual choice cannot become an unrequested adaptive pause"
    );
    assert!(
        merge_config(&base, &adaptive, &manual).is_err(),
        "two different operating modes are competing edits"
    );
    assert_eq!(merge_config(&base, &manual, &manual).unwrap(), manual);
    let unrelated = Config {
        slots: 6,
        ..base.clone()
    };
    let merged = merge_config(&base, &unrelated, &adaptive).unwrap();
    assert!(merged.automatic && merged.collect);
    assert_eq!(merged.slots, 6);

    let mut paused = adaptive.clone();
    paused.collect = false;
    let merged_pause = merge_config(
        &adaptive,
        &Config {
            slots: 5,
            ..adaptive.clone()
        },
        &paused,
    )
    .unwrap();
    assert!(merged_pause.automatic && !merged_pause.collect);
    assert_eq!(merged_pause.slots, 5);
}

#[test]
fn real_settings_save_rejects_competing_mode_without_touching_profile_or_undo() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let protected_app = settings.store.app(&anchor.id).unwrap();
        let before_days = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let before_state: State = settings.store.get("state").unwrap();
        let quick_copy = before_state.owned[0].path.clone();

        let base = Config {
            automatic: false,
            collect: true,
            learning_enabled: true,
            slots: 3,
            ..Config::default()
        };
        settings.store.set("config", &base).unwrap();
        let mut editor = Settings::new(settings.store.home.clone()).unwrap();
        assert_eq!(editor.saved_config, base);
        apply_mode(&mut editor.config, OperatingMode::Manual);
        editor.refresh_dirty();
        assert!(editor.dirty);

        // A concurrent Windows background/CLI configuration write chooses
        // adaptive mode while Settings still holds its older Observe draft.
        let mut latest = base.clone();
        apply_mode(&mut latest, OperatingMode::Adaptive);
        settings.store.set("config", &latest).unwrap();

        editor.save();
        let job = editor.job.take().expect("real asynchronous Settings save");
        let result = job
            .receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("save must not hang on a mode conflict");
        assert!(
            result.is_err(),
            "competing mode saves must not silently succeed"
        );
        assert_eq!(settings.store.get::<Config>("config").unwrap(), latest);
        assert_eq!(settings.store.app(&anchor.id).unwrap(), protected_app);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            before_days
        );
        assert_eq!(
            settings.store.get::<State>("state").unwrap().owned,
            before_state.owned
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        assert!(Path::new(&quick_copy).is_file());

        drop(editor);
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!Path::new(&quick_copy).exists());
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}

#[test]
fn a_collection_pause_and_a_concurrent_mode_change_cannot_invent_manual_mode() {
    let base = Config {
        automatic: true,
        collect: true,
        ..Config::default()
    };
    let mut paused = base.clone();
    paused.collect = false;
    let mut observing = base.clone();
    observing.automatic = false;
    assert!(merge_config(&base, &paused, &observing).is_err());
    assert!(merge_config(&base, &observing, &paused).is_err());
    let unrelated = Config {
        slots: 6,
        ..base.clone()
    };
    let merged = merge_config(&base, &unrelated, &paused).unwrap();
    assert!(merged.automatic && !merged.collect);
    assert_eq!(merged.slots, 6);
}

#[test]
fn explicit_turn_off_cannot_be_undone_by_an_older_adaptive_settings_draft() {
    with_profile(|settings, anchor| {
        let base = Config {
            automatic: false,
            collect: true,
            learning_enabled: true,
            slots: 3,
            ..Config::default()
        };
        settings.store.set("config", &base).unwrap();
        let mut editor = Settings::new(settings.store.home.clone()).unwrap();
        apply_mode(&mut editor.config, OperatingMode::Adaptive);
        editor.refresh_dirty();
        assert!(editor.dirty);

        let source_bytes = std::fs::read(&anchor.source).unwrap();
        let source_app = settings.store.app(&anchor.id).unwrap();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let original_state: State = settings.store.get("state").unwrap();
        setup::disable_background(&settings.store.home).unwrap();
        assert!(settings.store.get::<bool>("background_disabled").unwrap());

        editor.save();
        let job = editor.job.take().expect("asynchronous Settings save");
        let outcome = job
            .receiver
            .recv_timeout(Duration::from_secs(15))
            .expect("conflicting save completion");
        assert!(
            outcome.is_err(),
            "explicit Turn off must not be replaced by a stale mode"
        );
        let persisted: Config = settings.store.get("config").unwrap();
        assert!(!persisted.automatic && !persisted.collect && !persisted.autostart);
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert_eq!(settings.store.app(&anchor.id).unwrap(), source_app);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(
            settings.store.get::<State>("state").unwrap().owned,
            original_state.owned
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), source_bytes);
    });
}
