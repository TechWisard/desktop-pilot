use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-manual-selection-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed),
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                slots: 3,
                ..Default::default()
            },
        )
        .unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn original(store: &Store, name: &str, policy: Policy, used: bool) -> App {
    let source = store.home.join(format!("{name}.url"));
    std::fs::write(
        &source,
        format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
    )
    .unwrap();
    let mut app = native::read_link(&source).unwrap();
    app.policy = policy;
    store.put_app(&app).unwrap();
    if used {
        store.observe(&app.id, 300, true, false).unwrap();
    }
    app
}

#[test]
fn real_preview_apply_and_undo_respect_manual_then_observe_modes() {
    with_profile(|store| {
        let chosen = original(store, "Chosen", Policy::Always, false);
        let frequent = original(store, "FrequentlyUsed", Policy::Auto, true);
        let saved = store.get::<Config>("config").unwrap();
        let ranking = preview(store, true).unwrap();
        assert!(
            ranking
                .ranking
                .iter()
                .any(|r| r.id == frequent.id && r.eligible)
        );
        assert_eq!(
            ranking.selected,
            [chosen.id.clone()],
            "Manual mode must not silently add usage-ranked shortcuts",
        );
        apply(store, true).unwrap();
        let first: State = store.get("state").unwrap();
        assert_eq!(first.owned.len(), 1);
        assert_eq!(first.owned[0].app, chosen.id);
        assert!(std::path::Path::new(&first.owned[0].path).exists());
        assert_eq!(
            std::fs::read(&chosen.source).unwrap(),
            std::fs::read(&first.owned[0].path).unwrap()
        );

        // Observe only can propose and apply usage recommendations,
        // but returning to Manual must not quietly add another app.
        let observe = Config {
            collect: true,
            ..saved.clone()
        };
        store.set("config", &observe).unwrap();
        let proposal = preview(store, false).unwrap();
        assert!(proposal.added.contains(&frequent.id));
        apply(store, false).unwrap();
        let second: State = store.get("state").unwrap();
        assert_eq!(second.owned.len(), 2);

        let more = original(store, "NewlyBusy", Policy::Auto, true);
        store.set("config", &saved).unwrap();
        let manual = preview(store, false).unwrap();
        assert!(manual.added.is_empty());
        assert!(manual.removed.is_empty());
        apply(store, false).unwrap();
        let stable: State = store.get("state").unwrap();
        assert_eq!(
            stable
                .owned
                .iter()
                .map(|o| (&o.app, &o.path, &o.position))
                .collect::<Vec<_>>(),
            second
                .owned
                .iter()
                .map(|o| (&o.app, &o.path, &o.position))
                .collect::<Vec<_>>(),
        );
        assert!(!stable.owned.iter().any(|o| o.app == more.id));

        command(store, &format!("policy:{}:Never", frequent.id)).unwrap();
        apply(store, false).unwrap();
        let final_state: State = store.get("state").unwrap();
        assert_eq!(final_state.owned.len(), 1);
        assert_eq!(final_state.owned[0].app, chosen.id);
        assert!(std::path::Path::new(&chosen.source).exists());
        assert!(std::path::Path::new(&frequent.source).exists());
        assert!(std::path::Path::new(&more.source).exists());
        rollback(store).unwrap();
        assert!(std::path::Path::new(&frequent.source).exists());
        assert_eq!(store.app(&frequent.id).unwrap().policy, Policy::Never);
    });
}
