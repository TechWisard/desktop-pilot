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
    assert!(available.valid);
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

#[test]
fn short_clock_rewind_across_midnight_keeps_managed_usage_but_not_new_candidates() {
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-midnight-rewind-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let store = Store::open(&home).unwrap();
    let source = home.join("Familiar.url");
    let other_source = home.join("Other.url");
    let source_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Familiar\r\n";
    std::fs::write(&source, source_bytes).unwrap();
    std::fs::write(
        &other_source,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Other\r\n",
    )
    .unwrap();
    let familiar = native::read_link(&source).unwrap();
    let mut other = native::read_link(&other_source).unwrap();
    // The user worked after midnight, then corrected the clock to 22:00
    // on the previous day. A Store day bucket now looks two hours ahead.
    let midnight = (2_000_000_000_i64 / 86_400) * 86_400;
    let at = midnight - 2 * 3600;
    other.seed = 18.0;
    other.seed_at = at;
    store.put_app(&familiar).unwrap();
    store.put_app(&other).unwrap();
    store
        .db
        .execute(
            "INSERT INTO days(app,date,sessions,launches,seconds) VALUES(?1,?2,12,0,3600)",
            rusqlite::params![familiar.id, midnight],
        )
        .unwrap();
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let managed_copy = desktop.join("Familiar.url");
    std::fs::copy(&source, &managed_copy).unwrap();
    let state = State {
        initialized: true,
        owned: vec![Owned {
            app: familiar.id.clone(),
            path: managed_copy.to_string_lossy().into(),
            sha: hash(source_bytes),
            position: Point::default(),
            manual: false,
            since: at - 86_400,
            file_id: None,
        }],
        ..State::default()
    };
    let cfg = Config {
        slots: 1,
        collect: true,
        learning_enabled: false,
        ..Config::default()
    };
    store.set("state", &state).unwrap();
    store.set("config", &cfg).unwrap();
    let apps = store.apps().unwrap();
    let days = store.days().unwrap();

    let retained = engine::preview_loaded(&cfg, &apps, &days, &state, false, at).unwrap();
    assert_eq!(
        retained.selected,
        [familiar.id.clone()],
        "a two-hour clock rewind must not evict a managed shortcut with confirmed recent use"
    );
    assert!(retained.added.is_empty() && retained.removed.is_empty());

    let fresh = engine::preview_loaded(&cfg, &apps, &days, &State::default(), true, at).unwrap();
    assert_eq!(
        fresh.selected,
        [other.id.clone()],
        "future-dated history alone must never justify a new shortcut"
    );

    let mut excluded = apps.clone();
    excluded
        .iter_mut()
        .find(|app| app.id == familiar.id)
        .unwrap()
        .policy = Policy::Never;
    let manually_excluded =
        engine::preview_loaded(&cfg, &excluded, &days, &state, false, at).unwrap();
    assert_eq!(manually_excluded.selected, [other.id.clone()]);
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
    assert_eq!(std::fs::read(&managed_copy).unwrap(), source_bytes);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}
