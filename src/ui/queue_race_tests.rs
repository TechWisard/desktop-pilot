use super::import_ownership_tests::with_profile;
use super::*;
use std::sync::mpsc;

#[test]
fn queued_manual_action_cannot_escape_a_concurrent_explicit_disable() {
    // Real Settings::queue and startup/background named mutexes, SQLite and
    // disable_background. A held background mutex represents an already-ready
    // worker without launching programs or modifying the user's desktop.
    with_profile(|settings, anchor| {
        let home = settings.store.home.clone();
        std::fs::copy(
            std::env::current_exe().unwrap(),
            setup::installed_exe(&home),
        )
        .unwrap();
        settings.store.set("shutdown", &false).unwrap();
        settings.store.set("heartbeat", &now()).unwrap();
        settings.store.set("background_disabled", &false).unwrap();
        let background = native::exclusive("background", &home).unwrap();

        let (arrived_tx, arrived_rx) = mpsc::channel();
        let (resume_tx, resume_rx) = mpsc::channel();
        *setup::QUEUE_TEST_PAUSE.lock().unwrap() = Some(setup::QueueTestPause {
            home: home.clone(),
            arrived: arrived_tx,
            resume: resume_rx,
        });
        let original_bytes = std::fs::read(&anchor.source).unwrap();
        let rule_before = settings.store.app(&anchor.id).unwrap().policy;
        settings.queue(&format!("policy:{}:Never", anchor.id));
        let job = settings.job.take().expect("Settings queued the action");
        arrived_rx
            .recv_timeout(Duration::from_secs(25))
            .expect("enabled command passed worker readiness check");

        // If a second Settings instance can acquire startup now, Turn off can
        // finish before the first instance inserts its action. That leaves a
        // new pending command with no running worker to execute it.
        let interleaving_possible = native::try_exclusive("startup", &home).unwrap().is_some();
        resume_tx.send(()).unwrap();
        let response = job.receiver.recv_timeout(Duration::from_secs(25)).unwrap();
        let id = match response.unwrap() {
            JobResult::Queued(id, _) => id,
            _ => panic!("expected the enabled background command path"),
        };
        *setup::QUEUE_TEST_PAUSE.lock().unwrap() = None;
        drop(background);
        setup::disable_background(&home).unwrap();

        let status: String = settings
            .store
            .db
            .query_row("SELECT status FROM commands WHERE id=?1", [id], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(status, "error", "Turn off must cancel queued work");
        assert!(settings.store.pending().unwrap().is_empty());
        assert!(settings.store.get::<bool>("background_disabled").unwrap());
        assert!(settings.store.get::<bool>("shutdown").unwrap());
        assert_eq!(settings.store.app(&anchor.id).unwrap().policy, rule_before);
        assert_eq!(std::fs::read(&anchor.source).unwrap(), original_bytes);
        assert!(
            !interleaving_possible,
            "a second Settings instance could disable the worker between readiness and enqueuing, leaving an unexecuted command"
        );
    });
}
