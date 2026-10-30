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

#[test]
fn stalled_queue_does_not_block_a_real_independent_settings_save() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let saved_app = settings.store.app(&anchor.id).unwrap();
        let state: State = settings.store.get("state").unwrap();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        settings.store.command("rescan").unwrap();
        let id = settings.store.db.last_insert_rowid();
        assert!(settings.store.claim_command(id).unwrap());
        settings.reload().unwrap();

        let initial: Config = settings.store.get("config").unwrap();
        settings.config.slots = initial.slots + 1;
        settings.refresh_dirty();
        settings.save();
        let job = settings.job.take().expect("real Settings save starts");
        assert!(matches!(
            job.receiver.recv_timeout(Duration::from_secs(15)).unwrap(),
            Ok(JobResult::Saved(_, None))
        ));
        let persisted: Config = settings.store.get("config").unwrap();
        assert_eq!(persisted.slots, initial.slots + 1);
        assert_eq!(persisted.automatic, initial.automatic);
        assert_eq!(persisted.collect, initial.collect);
        assert_eq!(settings.store.app(&anchor.id).unwrap(), saved_app);
        assert_eq!(
            settings.store.get::<State>("state").unwrap().owned,
            state.owned
        );
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        let status: String = settings
            .store
            .db
            .query_row("SELECT status FROM commands WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "running", "an unrelated save cannot silently resolve work");
    });
}

#[test]
fn explicit_cleanup_marks_abandoned_commands_failed_and_keeps_undo_intact() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let saved_app = settings.store.app(&anchor.id).unwrap();
        let state: State = settings.store.get("state").unwrap();
        let quick_copy = state.owned[0].path.clone();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        settings.store.command("rescan").unwrap();
        let id = settings.store.db.last_insert_rowid();
        assert!(settings.store.claim_command(id).unwrap());
        settings.pending_commands.push(PendingCommand {
            id,
            action: "rescan".into(),
            submitted: Instant::now(),
        });
        settings.reload().unwrap();
        assert!(!settings.background_alive);

        settings.recover_stalled_commands();
        let job = settings.job.take().expect("explicit cleanup job starts");
        assert!(matches!(
            job.receiver.recv_timeout(Duration::from_secs(15)).unwrap(),
            Ok(JobResult::Completed(_))
        ));
        settings.reload().unwrap();
        let status: String = settings
            .store
            .db
            .query_row("SELECT status FROM commands WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "error");
        assert_eq!(settings.active_commands, 0);
        assert!(settings.pending_commands.is_empty());
        assert!(!settings.busy());
        assert_eq!(settings.store.app(&anchor.id).unwrap(), saved_app);
        assert_eq!(
            settings.store.get::<State>("state").unwrap().owned,
            state.owned
        );
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        assert!(Path::new(&quick_copy).exists());
        engine::command(&settings.store, "rollback").unwrap();
        assert!(!Path::new(&quick_copy).exists());
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}

#[test]
fn cleanup_refuses_to_cancel_live_work_after_a_stale_offline_snapshot() {
    with_profile(|settings, anchor| {
        let original = std::fs::read(&anchor.source).unwrap();
        let previous = settings.store.app(&anchor.id).unwrap();
        settings.store.command("rescan").unwrap();
        let id = settings.store.db.last_insert_rowid();
        assert!(settings.store.claim_command(id).unwrap());
        settings.reload().unwrap();
        assert!(!settings.background_alive);
        let _new_monitor = native::exclusive("background", &settings.store.home).unwrap();

        // The UI observed the worker missing before it restarted. Recheck
        // mutex ownership instead of trusting that stale UI snapshot.
        settings.recover_stalled_commands();
        let job = settings.job.take().expect("recheck job starts");
        let outcome = job.receiver.recv_timeout(Duration::from_secs(15)).unwrap();
        assert!(outcome.is_err(), "live work cannot be cancelled");
        let status: String = settings
            .store
            .db
            .query_row("SELECT status FROM commands WHERE id=?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "running");
        assert_eq!(settings.store.app(&anchor.id).unwrap(), previous);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}
