use super::*;
use crate::{engine, native, store::Store};
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[test]
fn manual_taskbar_pin_offline_source_explicit_choice_and_undo_are_consistent() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-compound-control-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();

    // All sources are inert Internet Shortcut fixtures; nothing is launched.
    let urls = [
        b"[InternetShortcut]\r\nURL=https://example.invalid/Existing\r\n".as_slice(),
        b"[InternetShortcut]\r\nURL=https://example.invalid/Offline\r\n".as_slice(),
        b"[InternetShortcut]\r\nURL=https://example.invalid/Chosen\r\n".as_slice(),
    ];
    let originals = ["Existing.url", "Offline.url", "Chosen.url"]
        .iter()
        .zip(urls)
        .map(|(name, contents)| {
            let path = home.join(name);
            std::fs::write(&path, contents).unwrap();
            path
        })
        .collect::<Vec<_>>();
    let mut apps = originals
        .iter()
        .map(|path| native::read_link(path).unwrap())
        .collect::<Vec<_>>();
    apps[0].pinned = true;
    apps[2].policy = Policy::Always;
    let outage_at = now() - 10 * 86_400;
    apps[1].valid = false;
    apps[1].unavailable_since = outage_at;
    let offline_original = home.join("Offline.source-suspended");
    std::fs::rename(&originals[1], &offline_original).unwrap();

    let copies = [desktop.join("Existing.url"), desktop.join("Offline.url")];
    for (index, copy) in copies.iter().enumerate() {
        std::fs::write(copy, urls[index]).unwrap();
    }
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    let config = Config {
        slots: 2,
        learning_enabled: false,
        respect_taskbar_pins: true,
        ..Config::default()
    };
    store.set("config", &config).unwrap();
    for app in &apps {
        store.put_app(app).unwrap();
    }
    let before = State {
        initialized: true,
        owned: (0..2)
            .map(|index| Owned {
                app: apps[index].id.clone(),
                path: copies[index].to_string_lossy().into_owned(),
                sha: hash(urls[index]),
                position: Point {
                    x: (index as i32) * 76,
                    y: 2,
                },
                manual: false,
                since: now() - 11 * 86_400,
                file_id: None,
            })
            .collect(),
        ..State::default()
    };
    store.set("state", &before).unwrap();
    let ranking = rank(&apps, &[], &config, now());
    assert!(!ranking.iter().find(|item| item.id == apps[0].id).unwrap().eligible);
    let planned = engine::preview_loaded(&config, &apps, &[], &before, false, now()).unwrap();
    assert_eq!(planned.selected, [apps[0].id.clone(), apps[2].id.clone()]);
    assert_eq!(planned.added, [apps[2].id.clone()]);
    assert_eq!(planned.removed, [apps[1].id.clone()]);
    assert_eq!(
        engine::preview(&store, false).unwrap().selected,
        planned.selected
    );
    for (path, expected) in originals.iter().zip(urls) {
        if path == &originals[1] {
            assert!(!path.exists());
        } else {
            assert_eq!(std::fs::read(path).unwrap(), expected);
        }
    }

    // Execute the real SQLite/desktop-copy transaction. The explicit Always
    // choice may replace an unavailable Auto copy, but the unrelated pinned
    // copy must not be removed just because the user chose Manual mode.
    engine::apply(&store, false).unwrap();
    let applied: State = store.get("state").unwrap();
    assert_eq!(applied.owned.len(), 2);
    assert!(applied.owned.iter().any(|item| item.app == apps[0].id));
    let chosen_copy = applied
        .owned
        .iter()
        .find(|item| item.app == apps[2].id)
        .unwrap()
        .path
        .clone();
    assert!(copies[0].exists());
    assert!(!copies[1].exists());
    assert_eq!(std::fs::read(&chosen_copy).unwrap(), urls[2]);
    assert_eq!(std::fs::read(&copies[0]).unwrap(), urls[0]);
    assert_eq!(std::fs::read(&originals[0]).unwrap(), urls[0]);
    assert_eq!(std::fs::read(&originals[2]).unwrap(), urls[2]);
    assert_eq!(std::fs::read(&offline_original).unwrap(), urls[1]);

    // Undo is a real file transaction, not merely a selector simulation.
    engine::command(&store, "rollback").unwrap();
    let restored: State = store.get("state").unwrap();
    assert_eq!(restored.owned.len(), 2);
    assert!(restored.owned.iter().any(|item| item.app == apps[0].id));
    assert!(restored.owned.iter().any(|item| item.app == apps[1].id));
    assert!(!std::path::Path::new(&chosen_copy).exists());
    assert_eq!(std::fs::read(&copies[0]).unwrap(), urls[0]);
    assert_eq!(std::fs::read(&copies[1]).unwrap(), urls[1]);
    assert_eq!(std::fs::read(&offline_original).unwrap(), urls[1]);

    // A subsequent explicit Never changes only the preview until Apply.
    store.policy(&apps[0].id, Policy::Never).unwrap();
    let excluded = engine::preview(&store, false).unwrap();
    assert!(excluded.removed.contains(&apps[0].id));
    assert!(!excluded.selected.contains(&apps[0].id));
    assert_eq!(std::fs::read(&copies[0]).unwrap(), urls[0]);

    // Changing to Observe stops Manual-only outage retention without silently
    // applying the preview or discarding the saved Never rule.
    store
        .set(
            "config",
            &Config {
                collect: true,
                ..config
            },
        )
        .unwrap();
    let observing = engine::preview(&store, false).unwrap();
    assert!(observing.removed.contains(&apps[1].id));
    assert!(observing.removed.contains(&apps[0].id));
    assert_eq!(store.app(&apps[0].id).unwrap().policy, Policy::Never);
    assert_eq!(std::fs::read(&copies[1]).unwrap(), urls[1]);
    assert_eq!(std::fs::read(&offline_original).unwrap(), urls[1]);

    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
