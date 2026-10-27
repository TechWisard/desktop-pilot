use super::*;

fn app(id: &str, policy: Policy, seed: f64) -> App {
    App {
        id: id.into(),
        name: id.into(),
        policy,
        valid: true,
        seed,
        seed_at: now(),
        ..Default::default()
    }
}
fn owned(id: &str, manual: bool) -> Owned {
    Owned {
        app: id.into(),
        path: String::new(),
        sha: String::new(),
        position: Point::default(),
        manual,
        since: now(),
        file_id: None,
    }
}

#[test]
fn manual_mode_does_not_add_old_high_score_apps_without_an_explicit_choice() {
    let cfg = Config {
        slots: 3,
        ..Default::default()
    };
    assert!(!cfg.collect && !cfg.automatic);
    let chosen = app("chosen", Policy::Always, 0.0);
    let old_activity = app("old-activity", Policy::Auto, 90.0);
    let apps = vec![chosen, old_activity];
    let ranked = rank(&apps, &[], &cfg, now());
    assert!(ranked.iter().any(|r| r.id == "old-activity" && r.eligible));
    let selected = select(&apps, &ranked, &State::default(), &cfg, true);
    assert_eq!(selected, ["chosen"], "Manual Apply should add only explicitly selected apps");
}

#[test]
fn manual_mode_keeps_existing_copies_stable_instead_of_replacing_them() {
    let cfg = Config {
        slots: 2,
        ..Default::default()
    };
    let apps = vec![
        app("chosen", Policy::Always, 0.0),
        app("existing", Policy::Auto, 4.0),
        app("newly-busy", Policy::Auto, 100.0),
    ];
    let state = State {
        owned: vec![owned("chosen", true), owned("existing", false)],
        initialized: true,
        ..Default::default()
    };
    let ranked = rank(&apps, &[], &cfg, now());
    assert_eq!(
        select(&apps, &ranked, &state, &cfg, false),
        ["chosen", "existing"],
        "Changing to manual control must not secretly replace an existing shortcut",
    );
}

#[test]
fn manual_mode_still_obeys_explicit_never_and_capacity_change() {
    let cfg = Config {
        slots: 1,
        ..Default::default()
    };
    let apps = vec![
        app("excluded", Policy::Never, 90.0),
        app("chosen", Policy::Always, 0.0),
        app("frequent", Policy::Auto, 120.0),
    ];
    let state = State {
        owned: vec![owned("excluded", false)],
        initialized: true,
        ..Default::default()
    };
    assert_eq!(
        select(&apps, &rank(&apps, &[], &cfg, now()), &state, &cfg, false),
        ["chosen"],
    );
}

#[test]
fn observe_only_and_paused_automatic_mode_still_allow_saved_recommendations() {
    for cfg in [
        Config {
            collect: true,
            automatic: false,
            slots: 1,
            ..Default::default()
        },
        Config {
            collect: false,
            automatic: true,
            slots: 1,
            ..Default::default()
        },
    ] {
        let apps = vec![app("recommended", Policy::Auto, 25.0)];
        assert_eq!(
            select(&apps, &rank(&apps, &[], &cfg, now()), &State::default(), &cfg, true),
            ["recommended"],
        );
    }
}
