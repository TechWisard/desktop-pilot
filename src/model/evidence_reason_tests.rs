use super::*;
use crate::{engine, native, store::Store};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};

fn link(root: &Path, name: &str) -> App {
    let path = root.join(format!("{name}.url"));
    std::fs::write(
        &path,
        format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
    )
    .unwrap();
    let app = native::read_link(&path).unwrap();
    assert!(app.valid, "{name} must be a usable native shortcut");
    app
}

fn reason<'a>(preview: &'a engine::Preview, app: &App) -> &'a str {
    preview
        .ranking
        .iter()
        .find(|ranked| ranked.id == app.id)
        .expect("real preview must include the saved app")
        .reason
        .as_str()
}

#[test]
fn recommendation_reasons_describe_observed_evidence_not_assumed_repetition() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-evidence-reasons-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let store = Store::open(&root).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                learning_enabled: false,
                respect_taskbar_pins: false,
                slots: 8,
                min_days: 1,
                ..Config::default()
            },
        )
        .unwrap();

    // A historical seed can qualify without any observed active day.
    let mut historical = link(&root, "historical-priority");
    historical.seed = 8.0;
    historical.seed_at = now();
    store.put_app(&historical).unwrap();

    // One focused session is not repeated usage.
    let focused = link(&root, "focused-project");
    store.put_app(&focused).unwrap();
    store.observe(&focused.id, 600, true, false).unwrap();

    // Repeated short interactions on one calendar day are not multiple days.
    let frequent = link(&root, "frequent-checks");
    store.put_app(&frequent).unwrap();
    for _ in 0..3 {
        store.observe(&frequent.id, 40, true, false).unwrap();
    }

    // Two distinct days from the actual SQLite usage schema.
    let across_days = link(&root, "two-days");
    store.put_app(&across_days).unwrap();
    store.observe(&across_days.id, 180, true, false).unwrap();
    let current_day = store
        .days()
        .unwrap()
        .into_iter()
        .find(|day| day.app == across_days.id)
        .unwrap()
        .date;
    store
        .db
        .execute(
            "INSERT INTO days(app,date,sessions,launches,seconds) VALUES(?1,?2,1,0,180)",
            rusqlite::params![across_days.id, current_day - 86_400],
        )
        .unwrap();

    // Weak and explicit policies must retain their original explanations.
    let brief = link(&root, "brief-once");
    store.put_app(&brief).unwrap();
    store.observe(&brief.id, 12, true, false).unwrap();

    let mut manual = link(&root, "rare-manual");
    manual.policy = Policy::Always;
    store.put_app(&manual).unwrap();

    let mut excluded = link(&root, "excluded");
    excluded.policy = Policy::Never;
    store.put_app(&excluded).unwrap();
    store.observe(&excluded.id, 1200, true, false).unwrap();

    let saved_days = serde_json::to_value(store.days().unwrap()).unwrap();
    let saved_files: Vec<_> = store
        .apps()
        .unwrap()
        .iter()
        .map(|app| {
            (
                app.source.clone(),
                std::fs::read(&app.source).expect("source file remains readable"),
            )
        })
        .collect();
    let preview = engine::preview(&store, true).unwrap();
    assert_eq!(preview.added.len(), 5);
    assert_eq!(preview.removed.len(), 0);
    assert_eq!(
        reason(&preview, &historical),
        "Сохранённый исторический приоритет; свежих наблюдений пока мало"
    );
    assert_eq!(
        reason(&preview, &focused),
        "Продолжительная работа за день; быстрый ярлык может быть полезен"
    );
    assert_eq!(
        reason(&preview, &frequent),
        "Несколько сеансов за день; быстрый ярлык может быть полезен"
    );
    assert_eq!(
        reason(&preview, &across_days),
        "Использование в разные дни; быстрый ярлык может быть полезен"
    );
    assert_eq!(
        reason(&preview, &brief),
        "Пока мало активности для нового быстрого ярлыка"
    );
    assert_eq!(reason(&preview, &manual), "Постоянно: ручной выбор");
    assert_eq!(reason(&preview, &excluded), "Исключено вручную");
    assert_eq!(
        serde_json::to_value(store.days().unwrap()).unwrap(),
        saved_days
    );
    assert!(store.get::<State>("state").unwrap().owned.is_empty());
    for (path, bytes) in saved_files {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn explicit_never_reason_survives_missing_original_and_restoration() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-exclusion-outage-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = root.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let source = desktop.join("Excluded project.url");
    let original = b"[InternetShortcut]\r\nURL=https://example.invalid/ExcludedProject\r\n";
    std::fs::write(&source, original).unwrap();
    let parked = root.join("Source temporarily unavailable");
    let _com = native::Com::new().unwrap();
    let store = Store::open(&root).unwrap();
    store.set("test_desktop", &desktop.to_string_lossy()).unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Config::default()
            },
        )
        .unwrap();
    let mut excluded = native::read_link(&source).unwrap();
    assert!(excluded.valid);
    excluded.policy = Policy::Never;
    store.put_app(&excluded).unwrap();
    store.observe(&excluded.id, 1200, true, false).unwrap();

    let before_history = serde_json::to_value(store.days().unwrap()).unwrap();
    let before = engine::preview(&store, true).unwrap();
    assert_eq!(reason(&before, &excluded), "Исключено вручную");
    assert!(!before.selected.contains(&excluded.id));

    // Use the actual catalog scanner and SQLite preview, not a duplicate
    // ranking model. A temporarily disconnected source must not erase the
    // user's explicit Never decision or misstate why it was excluded.
    std::fs::rename(&source, &parked).unwrap();
    engine::refresh_apps(&store).unwrap();
    let offline = store.app(&excluded.id).unwrap();
    assert!(!offline.valid);
    assert_eq!(offline.policy, Policy::Never);
    let during = engine::preview(&store, true).unwrap();
    assert_eq!(reason(&during, &excluded), "Исключено вручную");
    assert!(!during.selected.contains(&excluded.id));
    assert!(during.added.is_empty());

    std::fs::rename(&parked, &source).unwrap();
    engine::refresh_apps(&store).unwrap();
    let restored = store.app(&excluded.id).unwrap();
    assert!(restored.valid);
    assert_eq!(restored.policy, Policy::Never);
    let after = engine::preview(&store, true).unwrap();
    assert_eq!(reason(&after, &excluded), "Исключено вручную");
    assert!(!after.selected.contains(&excluded.id));
    assert_eq!(
        serde_json::to_value(store.days().unwrap()).unwrap(),
        before_history
    );
    assert!(store.get::<State>("state").unwrap().owned.is_empty());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
