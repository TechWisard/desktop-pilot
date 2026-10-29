use super::{import_ownership_tests::with_profile, *};

fn run_settings_action(settings: &mut Settings, action: &str) -> Result<()> {
    settings.queue(action);
    let job = settings
        .job
        .take()
        .expect("Settings action must start a job");
    let response = job
        .receiver
        .recv_timeout(Duration::from_secs(25))
        .expect("Settings action must complete");
    response.map(|_| ()).map_err(anyhow::Error::msg)
}

#[test]
fn disabled_profile_manual_rules_and_undo_do_not_restart_tracking() {
    // Actual Settings action queue, Store, engine command, source file and Undo.
    // A damaged installed executable must not prevent local manual management.
    with_profile(|settings, app| {
        let home = settings.store.home.clone();
        let quick = settings.store.get::<State>("state").unwrap().owned[0]
            .path
            .clone();
        let bytes = std::fs::read(&app.source).unwrap();
        let before_days = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        setup::disable_background(&home).unwrap();
        let installed = home.join("desktop-pilot.exe");
        std::fs::create_dir(&installed).unwrap(); // Simulate an obstructed installer path.
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(settings.store.get::<bool>("shutdown").unwrap());
        assert!(!native::is_running("background", &home).unwrap());

        run_settings_action(settings, &format!("policy:{}:Never", app.id))
            .expect("manual exclusion must work while background tracking is off");
        assert_eq!(settings.store.app(&app.id).unwrap().policy, Policy::Never);
        assert!(
            Path::new(&quick).exists(),
            "a rule change cannot silently delete a shortcut"
        );
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(settings.store.get::<bool>("shutdown").unwrap());
        assert!(!native::is_running("background", &home).unwrap());

        run_settings_action(settings, "apply")
            .expect("explicit manual Apply must not require the background worker");
        assert!(!Path::new(&quick).exists());
        run_settings_action(settings, "rollback")
            .expect("Undo must work while the program is turned off");
        assert!(Path::new(&quick).exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), bytes);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            before_days
        );
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(settings.store.get::<bool>("shutdown").unwrap());
        assert!(!native::is_running("background", &home).unwrap());
        assert!(
            installed.is_dir(),
            "the manual path must not touch the installer"
        );
    });
}
