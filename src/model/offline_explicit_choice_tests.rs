use super::*;
use crate::{engine, native, store::Store};
use std::{fs, path::Path, time::{Duration, SystemTime, UNIX_EPOCH}};

fn managed(id: &str) -> Owned {
    Owned {
        app: id.into(),
        path: String::new(),
        sha: String::new(),
        position: Point::default(),
        manual: false,
        since: 0,
        file_id: None,
    }
}

fn choices(apps: &[App], state: &State, cfg: &Config, at: i64) -> Vec<String> {
    let ranking = rank_with_owned(apps, &[], cfg, at, &state.owned);
    select_at(apps, &ranking, state, cfg, false, at)
}

#[test]
fn new_manual_always_replaces_an_offline_auto_before_an_available_auto() {
    let at = now();
    let offline = App {
        id: "offline".into(),
        name: "offline".into(),
        seed: 200.0,
        seed_at: at,
        valid: false,
        unavailable_since: at - 3600,
        ..Default::default()
    };
    let online = App {
        id: "online".into(),
        name: "online".into(),
        seed: 50.0,
        seed_at: at,
        valid: true,
        ..Default::default()
    };
    let mut chosen = App {
        id: "chosen".into(),
        name: "chosen".into(),
        valid: true,
        ..Default::default()
    };
    let cfg = Config {
        collect: true,
        automatic: true,
        slots: 2,
        ..Default::default()
    };
    let state = State {
        owned: vec![managed("offline"), managed("online")],
        initialized: true,
        ..Default::default()
    };
    let apps = vec![offline.clone(), online.clone(), chosen.clone()];
    // Nothing moves merely because a previously managed source disappears.
    assert_eq!(choices(&apps, &state, &cfg, at), ["offline", "online"]);

    chosen.policy = Policy::Always;
    let apps = vec![offline.clone(), online.clone(), chosen.clone()];
    // The explicit user choice is new. The already working shortcut should
    // not pay the price of protecting an unavailable Auto shortcut.
    assert_eq!(choices(&apps, &state, &cfg, at), ["online", "chosen"]);
    assert_eq!(choices(&apps, &state, &cfg, at), ["online", "chosen"]);

    // A *manual placement* of the offline copy is stronger than Auto grace.
    let mut protected = state.clone();
    protected.owned[0].manual = true;
    assert_eq!(choices(&apps, &protected, &cfg, at), ["offline", "chosen"]);

    // Explicit choices remain effective in manual-only mode as well.
    let manual = Config {
        automatic: false,
        collect: false,
        ..cfg
    };
    assert_eq!(choices(&apps, &state, &manual, at), ["online", "chosen"]);
}

fn create_url(path: &Path, target: &str) -> App {
    fs::write(
        path,
        format!("[InternetShortcut]\\r\\nURL={target}\\r\\n").as_bytes(),
    )
    .unwrap();
    let app = native::read_link(path).unwrap();
    assert!(app.valid, "a synthetic URL must be a valid native shortcut");
    app
}

#[test]
fn sqlite_catalog_preview_preserves_live_file_and_rules_when_always_is_added() {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-offline-always-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&home).unwrap();
    {
        let store = Store::open(&home).unwrap();
        let at = now();
        let offline_path = home.join("offline.url");
        let online_path = home.join("online.url");
        let chosen_path = home.join("chosen.url");
        let mut offline = create_url(&offline_path, "https://example.invalid/offline");
        let mut online = create_url(&online_path, "https://example.invalid/online");
        let mut chosen = create_url(&chosen_path, "https://example.invalid/chosen");
        offline.seed = 200.0;
        offline.seed_at = at;
        offline.valid = false;
        offline.unavailable_since = at - 3600;
        online.seed = 50.0;
        online.seed_at = at;
        chosen.seed = 0.0;
        chosen.seed_at = 0;
        let online_bytes = fs::read(&online_path).unwrap();
        let chosen_bytes = fs::read(&chosen_path).unwrap();
        for app in [&offline, &online, &chosen] {
            store.put_app(app).unwrap();
        }
        let cfg = Config {
            collect: true,
            automatic: true,
            learning_enabled: false,
            slots: 2,
            ..Default::default()
        };
        store.set("config", &cfg).unwrap();
        let state = State {
            owned: vec![managed(&offline.id), managed(&online.id)],
            initialized: true,
            ..Default::default()
        };
        store.set("state", &state).unwrap();
        // A missing source and the passage of an hour do not disturb
        // a two-slot arrangement in the absence of new user instructions.
        fs::remove_file(&offline_path).unwrap();
        let before = engine::preview(&store, false).unwrap();
        assert_eq!(before.selected, [offline.id.clone(), online.id.clone()]);
        assert!(before.added.is_empty());
        assert!(before.removed.is_empty());

        // Use the actual persisted policy command and production preview,
        // not an isolated alternate implementation of the selector.
        store.policy(&chosen.id, Policy::Always).unwrap();
        let after = engine::preview(&store, false).unwrap();
        assert_eq!(after.selected, [online.id.clone(), chosen.id.clone()]);
        assert_eq!(after.removed, [offline.id.clone()]);
        assert_eq!(after.added, [chosen.id.clone()]);
        assert_eq!(store.app(&chosen.id).unwrap().policy, Policy::Always);
        assert_eq!(store.app(&offline.id).unwrap().policy, Policy::Auto);
        assert_eq!(fs::read(&online_path).unwrap(), online_bytes);
        assert_eq!(fs::read(&chosen_path).unwrap(), chosen_bytes);
        // Releasing Always returns to the original stable layout.
        store.policy(&chosen.id, Policy::Auto).unwrap();
        let restored = engine::preview(&store, false).unwrap();
        assert_eq!(restored.selected, [offline.id.clone(), online.id.clone()]);
        assert!(restored.added.is_empty() && restored.removed.is_empty());
    }
    for attempt in 0..5 {
        match fs::remove_dir_all(&home) {
            Ok(()) => break,
            Err(_) if attempt < 4 => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => panic!("could not remove isolated profile: {error}"),
        }
    }
}
