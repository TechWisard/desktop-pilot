use super::import_ownership_tests::with_profile;
use super::*;

fn save_result(settings: &mut Settings) -> std::result::Result<JobResult, String> {
    settings.save();
    let job = settings
        .job
        .take()
        .expect("real Settings save job must start");
    job.receiver
        .recv_timeout(Duration::from_secs(15))
        .expect("real Settings save job must complete")
}

#[test]
fn unrelated_preference_saves_when_windows_autostart_registration_is_unavailable() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let prior_app = settings.store.app(&anchor.id).unwrap();
        let prior_days = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let prior_state =
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap();
        let base: Config = settings.store.get("config").unwrap();
        assert!(!base.autostart && base.slots < 8);

        // This failure hook runs at the actual setup::save_config boundary in
        // Settings' asynchronous SQLite writer. It does not modify real Windows.
        *setup::TEST_REJECT_AUTOSTART_UPDATE.lock().unwrap() = Some(settings.store.home.clone());

        settings.config.slots = base.slots + 1;
        settings.refresh_dirty();
        let saved = match save_result(settings) {
            Ok(JobResult::Saved(config, None)) => config,
            Ok(_) => panic!("unexpected Settings save result"),
            Err(error) => panic!(
                "Changing desktop capacity must not depend on an unchanged Windows startup setting: {error}"
            ),
        };
        assert_eq!(saved.slots, base.slots + 1);
        assert_eq!(saved.autostart, base.autostart);
        assert_eq!(settings.store.get::<Config>("config").unwrap(), saved);
        assert_eq!(settings.store.app(&anchor.id).unwrap(), prior_app);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            prior_days
        );
        assert_eq!(
            serde_json::to_value(settings.store.get::<State>("state").unwrap()).unwrap(),
            prior_state
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);

        // A real change to startup permission must still invoke the protected
        // Windows-registration path and refuse to promise success if it fails.
        settings.config.autostart = true;
        settings.refresh_dirty();
        match save_result(settings) {
            Err(error) => assert!(error.contains("Simulated Windows startup registration refusal")),
            Ok(_) => panic!("An unavailable startup change was falsely reported as saved"),
        }
        assert!(!settings.store.get::<Config>("config").unwrap().autostart);
        assert_eq!(
            settings.store.get::<Config>("config").unwrap().slots,
            base.slots + 1
        );

        *setup::TEST_REJECT_AUTOSTART_UPDATE.lock().unwrap() = None;
        let restored = match save_result(settings) {
            Ok(JobResult::Saved(config, None)) => config,
            _ => panic!("Startup preference must save after the failure clears"),
        };
        assert!(restored.autostart);
        assert_eq!(restored.slots, base.slots + 1);
        assert_eq!(settings.store.app(&anchor.id).unwrap(), prior_app);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        engine::command(&settings.store, "rollback").unwrap();
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            prior_days
        );
    });
}
