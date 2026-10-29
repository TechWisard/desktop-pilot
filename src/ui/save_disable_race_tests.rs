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
        let job = settings.job.take().expect("Settings uses the real asynchronous save");
        observed
            .recv_timeout(Duration::from_secs(15))
            .expect("save must reach the post-commit checkpoint");

        // This is the exact named Windows mutex Turn off acquires before it
        // cancels pending work. An available lock exposes the lost ordering.
        let startup_was_available = native::try_exclusive("startup", &home)
            .unwrap()
            .is_some();
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
        assert_eq!(serde_json::to_value(settings.store.days().unwrap()).unwrap(), history);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
    });
}
