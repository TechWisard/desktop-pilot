use super::import_ownership_tests::with_profile;
use super::*;

// An uncompleted queue row is not evidence of live work once the monitor's
// background mutex is gone. Settings must not remain permanently read-only.
#[test]
fn dead_background_worker_does_not_hold_configuration_hostage_to_old_commands() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let app = settings.store.app(&anchor.id).unwrap();
        let state: State = settings.store.get("state").unwrap();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();

        settings.store.command("rescan").unwrap();
        let id = settings.store.db.last_insert_rowid();
        assert!(settings.store.claim_command(id).unwrap());
        settings.reload().unwrap();

        assert!(!settings.background_alive, "no monitor process was started");
        assert_eq!(settings.active_commands, 1);
        assert!(
            !settings.busy(),
            "a dead worker must not indefinitely prevent saving independent settings"
        );
        assert_eq!(settings.store.app(&anchor.id).unwrap(), app);
        assert_eq!(
            settings.store.get::<State>("state").unwrap().owned,
            state.owned
        );
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}
