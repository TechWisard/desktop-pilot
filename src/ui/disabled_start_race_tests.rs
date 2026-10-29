use super::import_ownership_tests::with_profile;
use super::*;
use std::sync::mpsc;

#[test]
fn disabled_local_command_must_serialize_with_another_windows_startup() {
    // Actual Settings worker, Windows per-profile mutex, SQLite, source shortcut,
    // and manual rule command. Merely checking the disabled flag before
    // releasing the startup mutex cannot prevent a second window from starting.
    with_profile(|settings, anchor| {
        let home = settings.store.home.clone();
        let original = std::fs::read(&anchor.source).unwrap();
        let history = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let owned_before = settings.store.get::<State>("state").unwrap().owned;
        setup::disable_background(&home).unwrap();
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(!native::is_running("background", &home).unwrap());

        let (arrived_tx, arrived_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        *setup::DISABLED_LOCAL_TEST_PAUSE.lock().unwrap() = Some(setup::QueueTestPause {
            home: home.clone(),
            arrived: arrived_tx,
            resume: resume_rx,
        });
        settings.queue(&format!("policy:{}:Never", anchor.id));
        let job = settings.job.take().expect("real asynchronous Settings action");
        arrived_rx
            .recv_timeout(Duration::from_secs(15))
            .expect("disabled local action reached the command boundary");

        // The second window would own this exact named mutex before it tries
        // to enable tracking. A successful try-lock means it can start in the
        // middle of a previously authorized direct manual operation.
        let startup_available = native::try_exclusive("startup", &home).unwrap().is_some();
        resume_tx.send(()).unwrap();
        let outcome = job
            .receiver
            .recv_timeout(Duration::from_secs(20))
            .expect("manual command completed");
        setup::DISABLED_LOCAL_TEST_PAUSE.lock().unwrap().take();
        match outcome {
            Ok(JobResult::Completed(message)) => {
                assert!(message.contains("Правило сохранено"));
            }
            other => panic!("unexpected command response: {:?}", other.map(|_| ())),
        }
        assert_eq!(settings.store.app(&anchor.id).unwrap().policy, Policy::Never);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original);
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            history
        );
        let owned_after = settings.store.get::<State>("state").unwrap().owned;
        assert_eq!(owned_before.len(), owned_after.len());
        assert_eq!(owned_before[0].path, owned_after[0].path);
        assert_eq!(owned_before[0].position, owned_after[0].position);
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(settings.store.get::<bool>("shutdown").unwrap());
        assert!(!native::is_running("background", &home).unwrap());
        assert!(
            !startup_available,
            "a second Settings window can start background tracking while a disabled local command is in flight"
        );
    });
}
