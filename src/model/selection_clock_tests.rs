use super::*;
use crate::{engine, native, store::Store};
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);

#[test]
fn offline_retention_and_rank_use_one_snapshot_time() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-preview-time-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let store = Store::open(&home).unwrap();
    let source = home.join("Offline.url");
    std::fs::write(
        &source,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Offline\r\n",
    )
    .unwrap();
    let alternative = home.join("Available.url");
    std::fs::write(
        &alternative,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Available\r\n",
    )
    .unwrap();
    let at = now();
    let mut offline = native::read_link(&source).unwrap();
    offline.valid = false;
    offline.unavailable_since = at - 3600;
    let mut available = native::read_link(&alternative).unwrap();
    available.seed = 100.0;
    available.seed_at = at;
    store.put_app(&offline).unwrap();
    store.put_app(&available).unwrap();
    let cfg = Config {
        slots: 1,
        collect: true,
        learning_enabled: false,
        ..Config::default()
    };
    store.set("config", &cfg).unwrap();
    let state = State {
        initialized: true,
        owned: vec![Owned {
            app: offline.id.clone(),
            path: home
                .join("Desktop")
                .join("Offline.url")
                .to_string_lossy()
                .into(),
            sha: String::new(),
            position: Point::default(),
            manual: false,
            since: at - 7200,
            file_id: None,
        }],
        ..State::default()
    };
    store.set("state", &state).unwrap();
    let apps = store.apps().unwrap();
    let days = store.days().unwrap();
    let before = offline.unavailable_since + UNAVAILABLE_GRACE_SECONDS - 1;
    let early = engine::preview_loaded(&cfg, &apps, &days, &state, false, before).unwrap();
    assert_eq!(early.selected, [offline.id.clone()]);
    assert!(early.added.is_empty() && early.removed.is_empty());

    let after = offline.unavailable_since + UNAVAILABLE_GRACE_SECONDS + 1;
    let late = engine::preview_loaded(&cfg, &apps, &days, &state, false, after).unwrap();
    assert_eq!(
        late.selected,
        [available.id.clone()],
        "an expired auto outage must free its slot at the preview instant"
    );
    assert_eq!(late.added, [available.id.clone()]);
    assert_eq!(late.removed, [offline.id.clone()]);

    let mut changed = apps.clone();
    changed
        .iter_mut()
        .find(|a| a.id == offline.id)
        .unwrap()
        .policy = Policy::Always;
    let protected = engine::preview_loaded(&cfg, &changed, &days, &state, false, after).unwrap();
    assert_eq!(protected.selected, [offline.id.clone()]);
    changed
        .iter_mut()
        .find(|a| a.id == offline.id)
        .unwrap()
        .policy = Policy::Never;
    let excluded = engine::preview_loaded(&cfg, &changed, &days, &state, false, after).unwrap();
    assert_eq!(excluded.selected, [available.id.clone()]);
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"[InternetShortcut]\r\nURL=https://example.invalid/Offline\r\n"
    );
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
