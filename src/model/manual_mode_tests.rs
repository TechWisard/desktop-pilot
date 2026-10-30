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
    assert_eq!(
        selected,
        ["chosen"],
        "Manual Apply should add only explicitly selected apps"
    );
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
            select(
                &apps,
                &rank(&apps, &[], &cfg, now()),
                &State::default(),
                &cfg,
                true
            ),
            ["recommended"],
        );
    }
}

#[test]
fn manual_mode_does_not_remove_an_existing_auto_copy_just_because_it_is_taskbar_pinned() {
    let mut pinned = app("already-placed", Policy::Auto, 25.0);
    pinned.pinned = true;
    let apps = vec![pinned.clone()];
    let cfg = Config {
        slots: 1,
        respect_taskbar_pins: true,
        ..Default::default()
    };
    let state = State {
        owned: vec![owned("already-placed", false)],
        initialized: true,
        ..Default::default()
    };
    let ranking = rank(&apps, &[], &cfg, now());
    assert!(!ranking[0].eligible);
    assert_eq!(
        select(&apps, &ranking, &state, &cfg, false),
        ["already-placed"],
        "Manual mode must not silently undo an earlier placement after an external taskbar pin"
    );

    // Outside manual-only mode, the user's taskbar-pin filter still works.
    let observing = Config {
        collect: true,
        ..cfg.clone()
    };
    assert!(select(&apps, &ranking, &state, &observing, false).is_empty());

    pinned.policy = Policy::Never;
    let excluded = vec![pinned];
    assert!(
        select(
            &excluded,
            &rank(&excluded, &[], &cfg, now()),
            &state,
            &cfg,
            false
        )
        .is_empty(),
        "An explicit Never decision must still remove the existing copy"
    );
}

#[test]
fn saved_sqlite_preview_keeps_a_taskbar_pinned_copy_in_manual_mode() {
    use crate::{engine, native, store::Store};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-manual-pin-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = root.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let original = root.join("Source.url");
    let bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/manual-pin\r\n";
    std::fs::write(&original, bytes).unwrap();
    let quick = desktop.join("Saved.url");
    std::fs::copy(&original, &quick).unwrap();

    let store = Store::open(&root).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    let mut saved = native::read_link(&original).unwrap();
    saved.pinned = true;
    store.put_app(&saved).unwrap();
    let cfg = Config {
        slots: 1,
        learning_enabled: false,
        respect_taskbar_pins: true,
        ..Default::default()
    };
    store.set("config", &cfg).unwrap();
    let state = State {
        owned: vec![Owned {
            app: saved.id.clone(),
            path: quick.to_string_lossy().into_owned(),
            sha: hash(bytes),
            position: Point::default(),
            manual: false,
            since: now(),
            file_id: None,
        }],
        initialized: true,
        ..Default::default()
    };
    store.set("state", &state).unwrap();
    let preview = engine::preview(&store, false).unwrap();
    assert_eq!(preview.selected, [saved.id.clone()]);
    assert!(preview.added.is_empty() && preview.removed.is_empty());
    assert_eq!(std::fs::read(&quick).unwrap(), bytes);
    assert_eq!(std::fs::read(&original).unwrap(), bytes);

    store.policy(&saved.id, Policy::Never).unwrap();
    let explicit = engine::preview(&store, false).unwrap();
    assert_eq!(explicit.removed, [saved.id.clone()]);
    store.policy(&saved.id, Policy::Auto).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                ..cfg
            },
        )
        .unwrap();
    let observing = engine::preview(&store, false).unwrap();
    assert_eq!(observing.removed, [saved.id]);
    assert_eq!(store.get::<State>("state").unwrap().owned, state.owned);
    assert_eq!(std::fs::read(&quick).unwrap(), bytes);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
