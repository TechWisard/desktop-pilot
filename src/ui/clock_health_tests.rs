use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store)) {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-clock-health-{}-{}-{}",
        std::process::id(),
        now(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn backward_clock_correction_cannot_hide_a_frozen_background_worker() {
    with_profile(|store| {
        let config = Config {
            automatic: true,
            collect: true,
            learning_enabled: false,
            ..Config::default()
        };
        let progress = Progress::default();
        store.set("config", &config).unwrap();
        store.set("learning", &progress).unwrap();
        let wall = now();
        let future = wall + 30 * 86_400;
        store.set("heartbeat", &future).unwrap();

        // Reproduce the exact settings health calculation against the stored
        // checkpoint after a 30-day backward clock correction. The process
        // mutex can remain held even when the worker has stopped responding.
        let recorded: i64 = store.get("heartbeat").unwrap();
        assert_eq!(
            health(true, recorded, wall, "", &config, &progress),
            Health::Unresponsive,
            "a future heartbeat must not keep an unresponsive worker green"
        );

        // The existing 150-second tolerance is symmetric for small clock
        // adjustments; a fresh worker update clears the warning immediately.
        for drift in [-150_i64, -15, 0, 15, 150] {
            store.set("heartbeat", &(wall + drift)).unwrap();
            let recorded: i64 = store.get("heartbeat").unwrap();
            assert_eq!(
                health(true, recorded, wall, "", &config, &progress),
                Health::Automatic,
                "healthy worker with {drift}-second drift"
            );
        }
        for drift in [-151_i64, 151] {
            assert_eq!(
                health(true, wall + drift, wall, "", &config, &progress),
                Health::Unresponsive
            );
        }
        assert_eq!(store.get::<Config>("config").unwrap(), config);
        assert_eq!(store.get::<Progress>("learning").unwrap().sessions, 0);
    });
}

#[test]
fn offline_startup_manual_and_observation_states_keep_their_priority() {
    let wall = now();
    let progress = Progress::default();
    let mut config = Config {
        learning_enabled: false,
        automatic: false,
        collect: false,
        ..Config::default()
    };
    let future = wall + 30 * 86_400;
    assert_eq!(
        health(false, future, wall, "", &config, &progress),
        Health::Offline
    );
    assert_eq!(
        health(true, 0, wall, "", &config, &progress),
        Health::Starting
    );
    assert_eq!(
        health(true, wall, wall, "", &config, &progress),
        Health::Manual
    );
    assert_eq!(
        health(true, future, wall, "", &config, &progress),
        Health::Unresponsive
    );
    config.collect = true;
    config.learning_enabled = true;
    assert_eq!(
        health(true, wall, wall, "", &config, &progress),
        Health::Observing
    );
    config.collect = false;
    assert_eq!(
        health(true, wall, wall, "", &config, &progress),
        Health::ObservationPaused
    );
    assert_eq!(
        health(
            true,
            wall,
            wall,
            "Changes paused: source offline",
            &config,
            &progress
        ),
        Health::Attention
    );
}

#[test]
fn paused_adaptive_capture_is_not_reported_as_active_rearrangement() {
    let at = now();
    let mut config = Config {
        automatic: true,
        collect: false,
        learning_enabled: false,
        ..Config::default()
    };
    let progress = Progress::default();
    assert_eq!(
        health(true, at, at, "", &config, &progress),
        Health::ObservationPaused
    );

    config.collect = true;
    assert_eq!(
        health(true, at, at, "", &config, &progress),
        Health::Automatic
    );
    config.automatic = false;
    assert_eq!(
        health(true, at, at, "", &config, &progress),
        Health::Suggestions
    );
    config.collect = false;
    assert_eq!(health(true, at, at, "", &config, &progress), Health::Manual);
    assert_eq!(
        health(false, at, at, "", &config, &progress),
        Health::Offline
    );
}


#[test]
fn next_review_respects_paused_collection_and_corrected_clock() {
    with_profile(|store| {
        let mut config = Config {
            automatic: true,
            collect: false,
            learning_enabled: false,
            interval_days: 7,
            ..Config::default()
        };
        let mut state = State {
            initialized: true,
            last_review: now() - 8 * 86_400,
            ..State::default()
        };
        store.set("setup_complete", &true).unwrap();
        store.set("config", &config).unwrap();
        store.set("state", &state).unwrap();

        // Exercise the real Settings/Store loading path and the same label
        // rendered on Home. The worker can be alive while collection is paused.
        let mut settings = Settings::new(store.home.clone()).unwrap();
        settings.background_alive = true;
        assert_eq!(
            settings.next_review(),
            t(
                "Автоматические изменения приостановлены: наблюдение выключено",
                "Automatic changes paused: observation is off",
            ),
            "a paused adaptive profile must not promise an unattended change"
        );

        // A Windows wall-clock correction can move the last saved review into
        // the future. The real scheduler treats a large rewind as due, not
        // as a multiweek delay. Home must tell the same story.
        config.collect = true;
        state.last_review = now() + 45 * 86_400;
        store.set("config", &config).unwrap();
        store.set("state", &state).unwrap();
        settings.catalog_checkpoint = None;
        settings.reload().unwrap();
        settings.background_alive = true;
        assert_eq!(
            settings.next_review(),
            t("При ближайшем бездействии", "At the next idle opportunity"),
            "displayed review schedule must agree with corrected-clock eligibility"
        );

        // With an ordinary recent checkpoint there must be no false due-now
        // message. The manual mode label keeps its existing meaning.
        state.last_review = now() - 86_400;
        store.set("state", &state).unwrap();
        settings.catalog_checkpoint = None;
        settings.reload().unwrap();
        settings.background_alive = true;
        assert_ne!(
            settings.next_review(),
            t("При ближайшем бездействии", "At the next idle opportunity")
        );
        config.automatic = false;
        store.set("config", &config).unwrap();
        settings.catalog_checkpoint = None;
        settings.reload().unwrap();
        assert_eq!(
            settings.next_review(),
            t("По вашей команде", "When you choose Apply")
        );
        assert_eq!(
            store.get::<State>("state").unwrap().last_review,
            state.last_review
        );
    });
}
