use super::*;
use crate::{engine, native, store::Store};
use std::{
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[test]
fn manual_auto_anchor_and_new_always_rule_report_a_conflict_before_apply() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-manual-slot-feedback-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&home).unwrap();
    let desktop = home.join("Desktop");
    fs::create_dir_all(&desktop).unwrap();

    {
        let store = Store::open(&home).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        let manual_path = home.join("Frequent.url");
        let chosen_path = home.join("RareImportant.url");
        fs::write(
            &manual_path,
            b"[InternetShortcut]\r\nURL=https://example.invalid/Frequent\r\n",
        )
        .unwrap();
        fs::write(
            &chosen_path,
            b"[InternetShortcut]\r\nURL=https://example.invalid/RareImportant\r\n",
        )
        .unwrap();
        let manual = native::read_link(&manual_path).unwrap();
        let mut chosen = native::read_link(&chosen_path).unwrap();
        chosen.policy = Policy::Always;
        store.put_app(&manual).unwrap();
        store.put_app(&chosen).unwrap();
        let copy = desktop.join("Frequent.url");
        fs::copy(&manual_path, &copy).unwrap();
        let original = fs::read(&copy).unwrap();
        let state = State {
            owned: vec![Owned {
                app: manual.id.clone(),
                path: copy.to_string_lossy().into_owned(),
                sha: hash(&original),
                position: Point::default(),
                manual: true,
                since: now(),
                file_id: None,
            }],
            initialized: true,
            ..Default::default()
        };
        store.set("state", &state).unwrap();
        let config = Config {
            slots: 1,
            learning_enabled: false,
            ..Default::default()
        };
        store.set("config", &config).unwrap();

        let current = store.apps().unwrap();
        assert_eq!(
            current
                .iter()
                .filter(|app| app.valid && app.policy == Policy::Always)
                .count(),
            1,
            "the previous warning missed the manual Auto anchor"
        );
        assert_eq!(manual_slot_requirements(&current, &state), 2);
        let warning = engine::preview(&store, false).err().unwrap().to_string();
        assert!(warning.contains("Ручных закреплений"));
        assert_eq!(fs::read(&copy).unwrap(), original);
        assert_eq!(store.get::<State>("state").unwrap().owned.len(), 1);

        // Revoking the manual anchor resolves the conflict without removing
        // the user's original file or changing the requested Always rule.
        store.policy(&manual.id, Policy::Never).unwrap();
        assert_eq!(manual_slot_requirements(&store.apps().unwrap(), &state), 1);
        let preview = engine::preview(&store, false).unwrap();
        assert_eq!(preview.selected, [chosen.id.clone()]);
        assert_eq!(preview.removed, [manual.id.clone()]);
        assert_eq!(preview.added, [chosen.id.clone()]);
        assert_eq!(fs::read(&manual_path).unwrap(), original);
        assert_eq!(fs::read(&copy).unwrap(), original);

        // A later Undo of the policy choice restores the same explanation.
        store.policy(&manual.id, Policy::Auto).unwrap();
        assert_eq!(manual_slot_requirements(&store.apps().unwrap(), &state), 2);
        assert!(engine::preview(&store, false).is_err());
    }
    for attempt in 0..5 {
        match fs::remove_dir_all(&home) {
            Ok(()) => break,
            Err(_) if attempt < 4 => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => panic!("could not remove isolated profile: {error}"),
        }
    }
}

#[test]
fn offline_manual_reservations_count_once_and_never_rules_release_them() {
    let manual = App {
        id: "manual".into(),
        valid: true,
        ..Default::default()
    };
    let chosen = App {
        id: "chosen".into(),
        valid: true,
        policy: Policy::Always,
        ..Default::default()
    };
    let row = Owned {
        app: manual.id.clone(),
        manual: true,
        ..Default::default()
    };
    let state = State {
        owned: vec![row.clone(), row],
        ..Default::default()
    };
    assert_eq!(
        manual_slot_requirements(&[manual.clone(), chosen.clone()], &state),
        2
    );
    let mut unavailable = manual;
    unavailable.valid = false;
    unavailable.unavailable_since = now() - 6 * 86400;
    assert_eq!(
        manual_slot_requirements(&[unavailable.clone(), chosen.clone()], &state),
        2,
        "a manually placed shortcut retains its slot through a source outage"
    );
    unavailable.unavailable_since = 0;
    assert_eq!(
        manual_slot_requirements(&[unavailable.clone(), chosen.clone()], &state),
        1,
        "a changed or unverified identity is not protected by offline grace"
    );
    unavailable.valid = true;
    unavailable.policy = Policy::Never;
    assert_eq!(
        manual_slot_requirements(&[unavailable, chosen], &state),
        1,
        "Never releases the old manual reservation"
    );
}
