use super::*;
use crate::{engine, store::Store};
use std::sync::atomic::{AtomicU64, Ordering};

fn app(id: &str, seed: f64, time: i64) -> App {
    App {
        id: id.into(),
        name: id.into(),
        seed,
        seed_at: if seed > 0.0 { time } else { 0 },
        valid: true,
        ..Default::default()
    }
}

fn day(id: &str, time: i64, sessions: u32, seconds: u32) -> Day {
    Day {
        app: id.into(),
        date: time,
        sessions,
        seconds,
        ..Default::default()
    }
}

#[test]
fn one_short_first_visit_is_not_a_confident_automatic_recommendation() {
    let time = 100 * 86400 + 12 * 3600;
    let candidate = app("incidental", 0.0, time);
    let cfg = Config::default();
    let evidence = [day("incidental", time - 12 * 3600, 1, 12)];
    let ranking = rank(&[candidate], &evidence, &cfg, time);
    assert!(ranking[0].score > 0.0);
    assert!(!ranking[0].eligible);
    assert!(
        select(
            &[app("incidental", 0.0, time)],
            &ranking,
            &State::default(),
            &cfg,
            true
        )
        .is_empty()
    );
}

#[test]
fn a_weak_legacy_seed_alone_does_not_fill_a_new_slot() {
    let time = 100 * 86400 + 12 * 3600;
    let candidate = app("old-signal", 1.0, time);
    let cfg = Config::default();
    let ranking = rank(std::slice::from_ref(&candidate), &[], &cfg, time);
    assert!(!ranking[0].eligible);
    assert!(select(&[candidate], &ranking, &State::default(), &cfg, true).is_empty());
}

#[test]
fn repeated_short_visits_or_one_long_work_session_still_qualify() {
    let time = 100 * 86400 + 12 * 3600;
    let cfg = Config {
        collect: true,
        ..Default::default()
    };
    let apps = [app("repeated", 0.0, time), app("focused", 0.0, time)];
    let days = [
        day("repeated", time - 12 * 3600, 2, 80),
        day("focused", time - 12 * 3600, 1, 3600),
    ];
    let ranking = rank(&apps, &days, &cfg, time);
    assert!(ranking.iter().all(|candidate| candidate.eligible));
    let selected = select(&apps, &ranking, &State::default(), &cfg, true);
    assert_eq!(selected.len(), 2);
}

#[test]
fn manual_rare_app_and_explicit_limits_take_precedence() {
    let time = 100 * 86400 + 12 * 3600;
    let mut rare = app("rare", 0.0, time);
    rare.policy = Policy::Always;
    let mut blocked = app("blocked", 100.0, time);
    blocked.policy = Policy::Never;
    let mut pinned = app("pinned", 100.0, time);
    pinned.pinned = true;
    let apps = [rare, blocked, pinned];
    let cfg = Config {
        slots: 2,
        ..Config::default()
    };
    let ranking = rank(&apps, &[], &cfg, time);
    assert_eq!(
        select(&apps, &ranking, &State::default(), &cfg, true),
        ["rare"]
    );
    let cfg = Config { min_days: 3, ..cfg };
    let regular = app("regular", 0.0, time);
    let evidence = [day("regular", time - 12 * 3600, 12, 3600)];
    assert!(!rank(&[regular], &evidence, &cfg, time)[0].eligible);
}

fn with_profile(test: impl FnOnce(&Store, &App, &std::path::Path)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-cold-start-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = root.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&root).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                slots: 2,
                min_days: 1,
                learning_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    let original = root.join("Original.url");
    std::fs::write(
        &original,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Catalog\r\n",
    )
    .unwrap();
    let app = crate::native::read_link(&original).unwrap();
    store.put_app(&app).unwrap();
    test(&store, &app, &desktop);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn actual_store_preview_apply_and_undo_wait_for_repeated_demand() {
    with_profile(|store, app, desktop| {
        // Observe mode permits an explicit Apply of usage-driven suggestions.
        let mut config: Config = store.get("config").unwrap();
        config.collect = true;
        store.set("config", &config).unwrap();
        let original = std::fs::read(&app.source).unwrap();
        store.observe(&app.id, 12, true, false).unwrap();
        let early = engine::preview(store, true).unwrap();
        assert!(
            early.selected.is_empty(),
            "one incidental visit must not occupy a desktop slot"
        );
        engine::apply(store, true).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert!(!store.get::<State>("state").unwrap().initialized);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 0);

        store.observe(&app.id, 180, true, false).unwrap();
        let meaningful = engine::preview(store, true).unwrap();
        assert_eq!(meaningful.added.as_slice(), std::slice::from_ref(&app.id));
        engine::apply(store, true).unwrap();
        let before = store.get::<State>("state").unwrap();
        assert_eq!(before.owned.len(), 1);
        let quick = &before.owned[0];
        assert_eq!(std::fs::read(&quick.path).unwrap(), original);
        assert_eq!(std::fs::read(&app.source).unwrap(), original);

        for _ in 0..3 {
            let preview = engine::preview(store, false).unwrap();
            assert!(preview.added.is_empty() && preview.removed.is_empty());
            engine::apply(store, false).unwrap();
            let after = store.get::<State>("state").unwrap();
            assert_eq!(after.owned[0].path, quick.path);
            assert_eq!(after.owned[0].position, quick.position);
            assert_eq!(after.owned[0].file_id, quick.file_id);
        }
        engine::command(store, "rollback").unwrap();
        assert!(!std::path::Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), original);
    });
}

#[test]
fn an_explicit_manual_choice_bypasses_the_cold_start_gate() {
    with_profile(|store, app, _desktop| {
        store.policy(&app.id, Policy::Always).unwrap();
        let choice = engine::preview(store, true).unwrap();
        assert_eq!(choice.added.as_slice(), std::slice::from_ref(&app.id));
        engine::apply(store, true).unwrap();
        assert_eq!(store.get::<State>("state").unwrap().owned.len(), 1);
        engine::command(store, "rollback").unwrap();
        assert!(std::path::Path::new(&app.source).exists());
    });
}

#[test]
fn recent_five_minute_work_qualifies_but_an_isolated_stale_visit_fades() {
    let time = 100 * 86400 + 12 * 3600;
    let candidate = app("focused", 0.0, time);
    let config = Config::default();
    let recent = rank(
        std::slice::from_ref(&candidate),
        &[day("focused", time - 86400, 1, 300)],
        &config,
        time,
    );
    assert!(recent[0].eligible);
    let stale = rank(
        &[candidate],
        &[day("focused", time - 3 * 86400, 1, 300)],
        &config,
        time,
    );
    assert!(!stale[0].eligible);
}

#[test]
fn weaker_first_placement_gate_does_not_loosen_replacement_stability() {
    let time = 100 * 86400 + 12 * 3600;
    let incumbent = app("incumbent", 20.0, time);
    let config = Config {
        collect: true,
        slots: 1,
        advantage: 1.1,
        ..Default::default()
    };
    let state = State {
        owned: vec![Owned {
            app: incumbent.id.clone(),
            path: String::new(),
            sha: String::new(),
            position: Point::default(),
            manual: false,
            since: 0,
            file_id: None,
        }],
        initialized: true,
        ..Default::default()
    };
    let near = [incumbent.clone(), app("challenger", 22.8, time)];
    let near_rank = rank(&near, &[], &config, time);
    assert_eq!(
        select(&near, &near_rank, &state, &config, false),
        ["incumbent"]
    );
    let strong = [incumbent, app("challenger", 23.5, time)];
    let strong_rank = rank(&strong, &[], &config, time);
    assert_eq!(
        select(&strong, &strong_rank, &state, &config, false),
        ["challenger"]
    );
}

#[test]
fn changing_projects_after_a_gap_keeps_the_rare_manual_shortcut_and_undo_is_respected() {
    with_profile(|store, rare, _desktop| {
        // Real inert Internet shortcuts, the production SQLite Store, review,
        // placement and Undo. Days are synthetic historical records, not a
        // replay of human interactions or simulated filesystem operations.
        let mut cfg: Config = store.get("config").unwrap();
        cfg.automatic = true;
        cfg.collect = true;
        cfg.learning_enabled = false;
        cfg.respect_taskbar_pins = false;
        cfg.slots = 2;
        cfg.max_changes = 1;
        cfg.advantage = 1.5;
        store.set("config", &cfg).unwrap();
        store.policy(&rare.id, Policy::Always).unwrap();
        let make = |name: &str, score: f64, policy: Policy| {
            let path = store.home.join(format!("{name}.url"));
            std::fs::write(
                &path,
                format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
            )
            .unwrap();
            let mut app = crate::native::read_link(&path).unwrap();
            app.seed = score;
            app.seed_at = if score > 0.0 { now() } else { 0 };
            app.policy = policy;
            store.put_app(&app).unwrap();
            app
        };
        let old = make("FormerProject", 50.0, Policy::Auto);
        let recent = make("CurrentProject", 0.0, Policy::Auto);
        let excluded = make("ExplicitlyExcluded", 500.0, Policy::Never);
        let source_bytes: Vec<_> = [rare, &old, &recent, &excluded]
            .iter()
            .map(|app| std::fs::read(&app.source).unwrap())
            .collect();

        engine::apply(store, true).unwrap();
        let first: State = store.get("state").unwrap();
        assert_eq!(first.owned.len(), 2);
        let rare_copy = first
            .owned
            .iter()
            .find(|o| o.app == rare.id)
            .unwrap()
            .clone();
        let old_copy = first
            .owned
            .iter()
            .find(|o| o.app == old.id)
            .unwrap()
            .clone();
        assert!(std::path::Path::new(&rare_copy.path).is_file());
        assert!(!first.owned.iter().any(|o| o.app == excluded.id));

        // A weekend / project gap with no new observations must not generate
        // fresh copies or move the protected file.
        for _ in 0..4 {
            let unchanged = engine::preview(store, false).unwrap();
            assert!(unchanged.added.is_empty() && unchanged.removed.is_empty());
        }

        // Four distinct historical work days on a different project; each
        // uses the actual daily-history table read by engine::preview.
        for age_days in (1_i64..=4).rev() {
            store
                .db
                .execute(
                    "INSERT INTO days(app,date,sessions,launches,seconds) VALUES(?1,?2,12,12,600)",
                    rusqlite::params![recent.id, now() - age_days * 86400],
                )
                .unwrap();
            let proposal = engine::preview(store, false).unwrap();
            assert!(proposal.selected.contains(&rare.id));
            assert!(!proposal.selected.contains(&excluded.id));
            if age_days == 4 {
                assert!(
                    proposal.added.is_empty(),
                    "one historical day must not churn"
                );
            }
        }
        let proposal = engine::preview(store, false).unwrap();
        assert_eq!(proposal.added, vec![recent.id.clone()]);
        assert_eq!(proposal.removed, vec![old.id.clone()]);
        assert!(proposal.selected.contains(&rare.id));

        // Quiet-time scheduling is a real gate, even when a change is ready.
        let mut due: State = store.get("state").unwrap();
        due.last_review = now() - 8 * 86400;
        store.set("state", &due).unwrap();
        assert!(!engine::automatic_review(store, false).unwrap());
        assert_eq!(store.get::<State>("state").unwrap().owned, due.owned);
        assert!(engine::automatic_review(store, true).unwrap());
        let changed: State = store.get("state").unwrap();
        assert_eq!(changed.owned.len(), 2);
        let held = changed.owned.iter().find(|o| o.app == rare.id).unwrap();
        assert_eq!(held.path, rare_copy.path);
        assert_eq!(held.position, rare_copy.position);
        assert_eq!(held.file_id, rare_copy.file_id);
        assert!(changed.owned.iter().any(|o| o.app == recent.id));
        assert!(!std::path::Path::new(&old_copy.path).exists());

        // The real Undo pauses automation. Repeated quiet reviews must not
        // immediately replay a decision the user explicitly rejected.
        engine::command(store, "rollback").unwrap();
        let restored: State = store.get("state").unwrap();
        assert!(restored.owned.iter().any(|o| o.app == old.id));
        assert_eq!(
            restored
                .owned
                .iter()
                .find(|o| o.app == rare.id)
                .unwrap()
                .path,
            rare_copy.path
        );
        assert!(!store.get::<Config>("config").unwrap().automatic);
        let added_path = changed
            .owned
            .iter()
            .find(|o| o.app == recent.id)
            .unwrap()
            .path
            .clone();
        for _ in 0..4 {
            assert!(!engine::automatic_review(store, true).unwrap());
            assert!(!std::path::Path::new(&added_path).exists());
        }

        // An unavailable source must not sacrifice a manually protected copy.
        let offline = store.home.join("RareSource.offline");
        std::fs::rename(&rare.source, &offline).unwrap();
        engine::refresh_apps(store).unwrap();
        let unavailable = store.app(&rare.id).unwrap();
        assert!(!unavailable.valid);
        let offline_choice = engine::preview(store, false).unwrap();
        assert!(offline_choice.selected.contains(&rare.id));
        assert!(!offline_choice.removed.contains(&rare.id));
        // Explicit Apply still works while automatic adaptation remains paused.
        engine::command(store, "apply").unwrap();
        let offline_state: State = store.get("state").unwrap();
        assert!(offline_state.owned.iter().any(|o| o.app == rare.id));
        engine::command(store, "rollback").unwrap();
        std::fs::rename(&offline, &rare.source).unwrap();
        engine::refresh_apps(store).unwrap();
        assert!(store.app(&rare.id).unwrap().valid);
        assert_eq!(store.app(&rare.id).unwrap().policy, Policy::Always);
        for (app, original) in [rare, &old, &recent, &excluded].iter().zip(source_bytes) {
            assert_eq!(std::fs::read(&app.source).unwrap(), original);
        }
    });
}

#[test]
fn pausing_while_a_due_review_is_queued_does_not_rearrange_shortcuts() {
    with_profile(|store, old, desktop| {
        let mut config: Config = store.get("config").unwrap();
        config.automatic = true;
        config.collect = true;
        config.learning_enabled = false;
        config.slots = 1;
        config.max_changes = 1;
        store.set("config", &config).unwrap();

        let mut first = store.app(&old.id).unwrap();
        first.seed = 30.0;
        first.seed_at = now();
        store.put_app(&first).unwrap();
        engine::apply(store, true).unwrap();
        let original: State = store.get("state").unwrap();
        assert_eq!(original.owned[0].app, old.id);
        let original_bytes = std::fs::read(&old.source).unwrap();
        let original_copy = original.owned[0].path.clone();

        let new_source = store.home.join("LaterProject.url");
        let new_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/LaterProject\r\n";
        std::fs::write(&new_source, new_bytes).unwrap();
        let mut candidate = crate::native::read_link(&new_source).unwrap();
        candidate.seed = 160.0;
        candidate.seed_at = now();
        store.put_app(&candidate).unwrap();

        let mut due = original.clone();
        due.last_review = now() - 8 * 86_400;
        store.set("state", &due).unwrap();
        let proposal = engine::preview(store, false).unwrap();
        assert_eq!(proposal.added, vec![candidate.id.clone()]);
        assert_eq!(proposal.removed, vec![old.id.clone()]);

        // The worker already checked its old Adaptive permission. Pause at
        // its actual boundary before Apply while Settings commits a new mode.
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let home = store.home.clone();
        let review = std::thread::spawn(move || {
            let worker = Store::open(&home).unwrap();
            engine::automatic_review_with_boundary(&worker, true, || {
                entered_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            })
            .unwrap()
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap();
        config.collect = false;
        store.set("config", &config).unwrap();
        resume_tx.send(()).unwrap();
        let changed = review.join().unwrap();

        assert!(
            !changed,
            "a committed observation pause must cancel a queued unattended review"
        );
        let after: State = store.get("state").unwrap();
        assert_eq!(after.owned, due.owned);
        assert_eq!(after.last_review, due.last_review);
        assert_eq!(std::fs::read(&original_copy).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&old.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&new_source).unwrap(), new_bytes);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 1);
    });
}

#[test]
fn extending_review_interval_while_a_review_is_queued_prevents_early_replacement() {
    with_profile(|store, incumbent, desktop| {
        let mut config: Config = store.get("config").unwrap();
        config.automatic = true;
        config.collect = true;
        config.learning_enabled = false;
        config.slots = 1;
        config.max_changes = 1;
        config.interval_days = 7;
        store.set("config", &config).unwrap();

        let mut old = store.app(&incumbent.id).unwrap();
        old.seed = 30.0;
        old.seed_at = now();
        store.put_app(&old).unwrap();
        engine::apply(store, true).unwrap();
        let baseline: State = store.get("state").unwrap();
        assert_eq!(baseline.owned.len(), 1);
        assert_eq!(baseline.owned[0].app, old.id);
        let old_bytes = std::fs::read(&old.source).unwrap();
        let old_copy = baseline.owned[0].path.clone();

        let new_source = store.home.join("NextProject.url");
        let new_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/NextProject\r\n";
        std::fs::write(&new_source, new_bytes).unwrap();
        let mut candidate = crate::native::read_link(&new_source).unwrap();
        candidate.seed = 160.0;
        candidate.seed_at = now();
        store.put_app(&candidate).unwrap();

        let mut due = baseline.clone();
        due.last_review = now() - 8 * 86400;
        store.set("state", &due).unwrap();
        let proposal = engine::preview(store, false).unwrap();
        assert_eq!(proposal.added, vec![candidate.id.clone()]);
        assert_eq!(proposal.removed, vec![old.id.clone()]);

        // The worker approved a seven-day review. Settings lengthens the
        // interval before any placement, under a different SQLite connection.
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let home = store.home.clone();
        let review = std::thread::spawn(move || {
            let worker = Store::open(&home).unwrap();
            engine::automatic_review_with_boundary(&worker, true, || {
                entered_tx.send(()).unwrap();
                resume_rx.recv().unwrap();
            })
            .unwrap()
        });
        entered_rx
            .recv_timeout(std::time::Duration::from_secs(15))
            .unwrap();
        config.interval_days = 30;
        store.set("config", &config).unwrap();
        resume_tx.send(()).unwrap();

        assert!(
            !review.join().unwrap(),
            "extending a saved review interval must cancel a queued premature change"
        );
        let after: State = store.get("state").unwrap();
        assert_eq!(after.owned, due.owned);
        assert_eq!(after.last_review, due.last_review);
        assert_eq!(std::fs::read(&old.source).unwrap(), old_bytes);
        assert_eq!(std::fs::read(&old_copy).unwrap(), old_bytes);
        assert_eq!(std::fs::read(&new_source).unwrap(), new_bytes);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 1);

        // Restore the earlier interval: the same real scheduled review is
        // eligible again, and Undo still recovers the original shortcut.
        config.interval_days = 7;
        store.set("config", &config).unwrap();
        assert!(engine::automatic_review(store, true).unwrap());
        let changed: State = store.get("state").unwrap();
        assert_eq!(changed.owned.len(), 1);
        assert_eq!(changed.owned[0].app, candidate.id);
        engine::command(store, "rollback").unwrap();
        let restored: State = store.get("state").unwrap();
        assert_eq!(restored.owned[0].app, old.id);
        assert_eq!(std::fs::read(&old.source).unwrap(), old_bytes);
        assert_eq!(std::fs::read(&old_copy).unwrap(), old_bytes);
        assert_eq!(std::fs::read(&new_source).unwrap(), new_bytes);
    });
}

#[test]
fn paused_collection_must_not_apply_stale_adaptive_recommendations() {
    with_profile(|store, old, desktop| {
        // This unusual but supported state is reachable through Settings'
        // concurrent-edit merge: automatic permission remains enabled while
        // input collection is explicitly paused.
        let mut config: Config = store.get("config").unwrap();
        config.automatic = true;
        config.collect = true;
        config.learning_enabled = false;
        config.slots = 1;
        config.max_changes = 1;
        store.set("config", &config).unwrap();

        let mut previous = store.app(&old.id).unwrap();
        previous.seed = 30.0;
        previous.seed_at = now();
        store.put_app(&previous).unwrap();
        engine::apply(store, true).unwrap();
        let initial: State = store.get("state").unwrap();
        assert_eq!(initial.owned.len(), 1);
        assert_eq!(initial.owned[0].app, old.id);
        let initial_copy = initial.owned[0].clone();
        let original_bytes = std::fs::read(&old.source).unwrap();

        let new_source = store.home.join("NewProject.url");
        let new_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/NewProject\r\n";
        std::fs::write(&new_source, new_bytes).unwrap();
        let mut candidate = crate::native::read_link(&new_source).unwrap();
        candidate.seed = 160.0;
        candidate.seed_at = now();
        store.put_app(&candidate).unwrap();

        let mut due = initial.clone();
        due.last_review = now() - 8 * 86_400;
        store.set("state", &due).unwrap();
        let proposal = engine::preview(store, false).unwrap();
        assert_eq!(
            proposal.added.as_slice(),
            std::slice::from_ref(&candidate.id)
        );
        assert_eq!(proposal.removed.as_slice(), std::slice::from_ref(&old.id));

        // Pausing foreground collection is not permission to quietly rearrange
        // the desktop using a now-stale historical preference.
        config.collect = false;
        store.set("config", &config).unwrap();
        assert!(
            !engine::automatic_review(store, true).unwrap(),
            "a paused tracker must not make unattended desktop changes"
        );
        let paused: State = store.get("state").unwrap();
        assert_eq!(paused.owned, due.owned);
        assert_eq!(paused.last_review, due.last_review);
        assert_eq!(std::fs::read(&initial_copy.path).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&old.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&new_source).unwrap(), new_bytes);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 1);

        // Resuming explicitly restores scheduled changes and the usual Undo.
        config.collect = true;
        store.set("config", &config).unwrap();
        assert!(engine::automatic_review(store, true).unwrap());
        let changed: State = store.get("state").unwrap();
        assert_eq!(changed.owned.len(), 1);
        assert_eq!(changed.owned[0].app, candidate.id);
        assert!(!std::path::Path::new(&initial_copy.path).exists());
        assert_eq!(std::fs::read(&changed.owned[0].path).unwrap(), new_bytes);
        engine::command(store, "rollback").unwrap();
        let restored: State = store.get("state").unwrap();
        assert_eq!(restored.owned.len(), 1);
        assert_eq!(restored.owned[0].app, old.id);
        assert_eq!(std::fs::read(&initial_copy.path).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&old.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&new_source).unwrap(), new_bytes);
        assert!(!store.get::<Config>("config").unwrap().automatic);
    });
}

#[test]
fn a_recent_clock_rewind_must_not_evict_a_familiar_shortcut() {
    with_profile(|store, incumbent, desktop| {
        let mut config: Config = store.get("config").unwrap();
        config.collect = true;
        config.slots = 1;
        config.max_changes = 1;
        config.respect_taskbar_pins = false;
        store.set("config", &config).unwrap();

        let mut familiar = incumbent.clone();
        familiar.seed = 100.0;
        familiar.seed_at = now();
        store.put_app(&familiar).unwrap();
        engine::apply(store, true).unwrap();
        let before: State = store.get("state").unwrap();
        assert_eq!(before.owned.len(), 1);
        assert_eq!(before.owned[0].app, familiar.id);
        let quick_path = before.owned[0].path.clone();
        let original_bytes = std::fs::read(&familiar.source).unwrap();
        let quick_bytes = std::fs::read(&quick_path).unwrap();

        // A new project has a legitimate but weaker current-time signal.
        let other_source = store.home.join("CurrentProject.url");
        let other_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/CurrentProject\r\n";
        std::fs::write(&other_source, other_bytes).unwrap();
        let mut challenger = crate::native::read_link(&other_source).unwrap();
        assert!(challenger.valid);
        challenger.seed = 40.0;
        challenger.seed_at = now();
        store.put_app(&challenger).unwrap();

        // A *new* shortcut with an unsupported future score must not gain
        // eligibility from this recovery rule, even if its claimed score
        // dwarfs the already owned candidate's confirmed history.
        let untrusted_source = store.home.join("UntrustedFuture.url");
        let untrusted_bytes =
            b"[InternetShortcut]\r\nURL=https://example.invalid/UntrustedFuture\r\n";
        std::fs::write(&untrusted_source, untrusted_bytes).unwrap();
        let mut untrusted = crate::native::read_link(&untrusted_source).unwrap();
        untrusted.seed = 3000.0;
        untrusted.seed_at = now() + 4 * 3600;
        store.put_app(&untrusted).unwrap();

        // A four-hour Windows clock correction makes the existing recent
        // checkpoint appear to be in the future. It is not evidence that the
        // familiar app ceased being useful and must not erase its entire rank.
        familiar.seed_at = now() + 4 * 3600;
        store.put_app(&familiar).unwrap();
        let rewind_preview = engine::preview(store, false).unwrap();
        assert_eq!(
            rewind_preview.selected,
            [familiar.id.clone()],
            "a small backward clock correction must not displace a stronger familiar shortcut"
        );
        assert!(rewind_preview.added.is_empty());
        assert!(rewind_preview.removed.is_empty());

        // Explicit user intent still wins, even when history was kept.
        familiar.policy = Policy::Never;
        store.put_app(&familiar).unwrap();
        let excluded = engine::preview(store, false).unwrap();
        assert_eq!(excluded.selected, [challenger.id.clone()]);
        assert_eq!(excluded.removed, [familiar.id.clone()]);
        assert_eq!(excluded.added, [challenger.id.clone()]);

        // A checkpoint several days ahead is not trusted as fresh evidence.
        // This conservative boundary prevents an implausibly future-dated
        // imported score from overriding a currently supported project.
        familiar.policy = Policy::Auto;
        familiar.seed_at = now() + 3 * 86_400;
        store.put_app(&familiar).unwrap();
        let distant_future = engine::preview(store, false).unwrap();
        assert_eq!(distant_future.selected, [challenger.id.clone()]);

        // This is a read-only preview sequence, not an actual rearrangement.
        assert_eq!(std::fs::read(&familiar.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&quick_path).unwrap(), quick_bytes);
        assert_eq!(std::fs::read(&other_source).unwrap(), other_bytes);
        assert_eq!(std::fs::read(&untrusted_source).unwrap(), untrusted_bytes);
        assert_eq!(std::fs::read_dir(desktop).unwrap().count(), 1);
        assert_eq!(store.get::<State>("state").unwrap().owned, before.owned);
    });
}
