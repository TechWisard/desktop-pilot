use super::import_ownership_tests::with_profile;
use super::*;
use std::sync::Mutex;

// The pause happens on the actual Settings save worker after SQLite commit
// but before its decision to resume background work. The shipped binary has
// neither this pause nor a global synchronization registry.
struct SavePause {
    home: PathBuf,
    arrived: mpsc::Sender<()>,
    resume: mpsc::Receiver<()>,
}
static SAVE_PAUSE: Mutex<Option<SavePause>> = Mutex::new(None);

pub(super) fn pause_after_save(home: &Path) {
    let gate = SAVE_PAUSE.lock().unwrap();
    if let Some(gate) = gate.as_ref().filter(|gate| gate.home == home) {
        gate.arrived
            .send(())
            .expect("save reached the post-commit checkpoint");
        gate.resume
            .recv_timeout(Duration::from_secs(20))
            .expect("save checkpoint released");
    }
}

#[test]
fn saving_settings_cannot_interleave_turn_off_between_commit_and_restart_decision() {
    with_profile(|settings, anchor| {
        let home = settings.store.home.clone();
        let original = std::fs::read(&anchor.source).unwrap();
        let owned = settings.store.get::<State>("state").unwrap().owned;
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();

        // An explicitly disabled profile may still be configured manually.
        // Changing an unrelated setting must neither restart observation nor
        // allow Turn off to cross an in-flight save's restart decision.
        settings.store.set("background_disabled", &true).unwrap();
        settings.store.set("shutdown", &true).unwrap();
        settings.config.zone_col += 1;
        settings.refresh_dirty();
        assert!(settings.dirty);

        let (arrived, observed) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        *SAVE_PAUSE.lock().unwrap() = Some(SavePause {
            home: home.clone(),
            arrived,
            resume,
        });
        settings.save();
        let job = settings
            .job
            .take()
            .expect("Settings uses the real asynchronous save");
        observed
            .recv_timeout(Duration::from_secs(15))
            .expect("save must reach the post-commit checkpoint");

        // This is the exact named Windows mutex Turn off acquires before it
        // cancels pending work. An available lock exposes the lost ordering.
        let startup_was_available = native::try_exclusive("startup", &home).unwrap().is_some();
        release.send(()).unwrap();
        let result = job
            .receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("save worker completed");
        SAVE_PAUSE.lock().unwrap().take();
        match result {
            Ok(JobResult::Saved(config, None)) => {
                assert_eq!(config.zone_col, settings.config.zone_col);
            }
            Ok(_) => panic!("unexpected Settings save result"),
            Err(error) => panic!("Settings save failed: {error}"),
        }
        assert!(
            !startup_was_available,
            "Turn off can complete after Settings saves but before its restart decision"
        );

        let disabled = setup::disable_background(&home).unwrap();
        assert!(!disabled.collect && !disabled.automatic && !disabled.autostart);
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(!native::is_running("background", &home).unwrap());
        assert_eq!(settings.store.get::<State>("state").unwrap().owned, owned);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}

#[test]
fn paused_collection_save_waits_for_an_in_flight_placement_before_acknowledging_it() {
    with_profile(|settings, anchor| {
        let home = settings.store.home.clone();
        let mut adaptive: Config = settings.store.get("config").unwrap();
        adaptive.automatic = true;
        adaptive.collect = true;
        adaptive.learning_enabled = false;
        settings.store.set("config", &adaptive).unwrap();

        let original = std::fs::read(&anchor.source).unwrap();
        let before: State = settings.store.get("state").unwrap();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let mut editor = Settings::new(home.clone()).unwrap();
        assert_eq!(editor.saved_config, adaptive);
        editor.config.collect = false;
        editor.refresh_dirty();
        assert!(editor.dirty);

        // A real named placement mutex is held by a different thread from
        // the production asynchronous Settings save. Its persisted pause
        // cannot be acknowledged while automatic file changes are active.
        let placement = native::exclusive("changes", &home).unwrap();
        editor.save();
        let job = editor.job.take().expect("real asynchronous Settings save");
        assert!(
            matches!(
                job.receiver.recv_timeout(Duration::from_millis(500)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "Settings must not confirm observation pause while placement owns the mutex"
        );
        assert_eq!(settings.store.get::<Config>("config").unwrap(), adaptive);
        assert_eq!(settings.store.get::<State>("state").unwrap().owned, before.owned);
        drop(placement);

        match job.receiver.recv_timeout(Duration::from_secs(15)).unwrap() {
            Ok(JobResult::Saved(updated, None)) => {
                assert!(updated.automatic && !updated.collect);
            }
            Ok(_) => panic!("unexpected save result"),
            Err(error) => panic!("paused observation save failed: {error}"),
        }
        let persisted: Config = settings.store.get("config").unwrap();
        assert!(persisted.automatic && !persisted.collect);
        assert_eq!(settings.store.get::<State>("state").unwrap().owned, before.owned);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        drop(editor);
    });
}
