use super::*;
use crate::model::IDENTITY_EVALUATIONS;
use std::sync::atomic::{AtomicU64, Ordering};

/// Exercise the real filesystem scan, Windows URL parser, SQLite catalog,
/// manual selection and Undo. The counter is test-only and records actual
/// App::identity calls, not timings or a parallel simplified index.
#[test]
fn large_saved_catalog_rescans_require_linear_identity_work_and_preserve_manual_choices() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let _com = native::Com::new().unwrap();
    for size in [100usize, 1000] {
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-identity-index-{}-{}-{}",
            std::process::id(),
            now(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let sources = home.join("Desktop").join("Projects");
        std::fs::create_dir_all(&sources).unwrap();
        let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
        let desktop = home.join("Desktop");
        let sources = desktop.join("Projects");
        let store = Store::open(&home).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store
            .set(
                "config",
                &Config {
                    slots: 2,
                    collect: false,
                    automatic: false,
                    learning_enabled: false,
                    respect_taskbar_pins: false,
                    ..Default::default()
                },
            )
            .unwrap();
        for i in 0..size {
            let path = sources.join(format!("Project-{i:04}.url"));
            std::fs::write(
                path,
                format!("[InternetShortcut]\r\nURL=https://example.invalid/projects/{i:04}\r\n"),
            )
            .unwrap();
        }
        refresh_apps(&store).unwrap();
        let apps = store.apps().unwrap();
        assert_eq!(apps.len(), size);
        let important = apps[0].clone();
        let excluded = apps[1].clone();
        store.policy(&important.id, Policy::Always).unwrap();
        store.policy(&excluded.id, Policy::Never).unwrap();
        store.observe(&important.id, 180, true, false).unwrap();
        apply(&store, true).unwrap();
        let owned_before: State = store.get("state").unwrap();
        assert_eq!(owned_before.owned.len(), 1);
        let quick = owned_before.owned[0].clone();
        let original_bytes = std::fs::read(&important.source).unwrap();
        let quick_bytes = std::fs::read(&quick.path).unwrap();
        let history_before = serde_json::to_value(store.days().unwrap()).unwrap();

        IDENTITY_EVALUATIONS.with(|calls| calls.set(0));
        SCAN_SHORTCUT_READS.with(|calls| calls.set(0));
        let started = Instant::now();
        refresh_apps(&store).unwrap();
        let elapsed = started.elapsed();
        let evaluations = IDENTITY_EVALUATIONS.with(|calls| calls.get());
        let parses = SCAN_SHORTCUT_READS.with(|calls| calls.get());
        assert!(
            parses <= size + 8,
            "Stable saved originals should be parsed once per rescan, not twice: {parses} for {size}"
        );
        println!(
            "saved_catalog={} identity_evaluations={} rescan_ms={} (CI fixture, not human latency)",
            size,
            evaluations,
            elapsed.as_millis()
        );
        assert!(
            evaluations <= size * 10 + 100,
            "A saved catalog must not compare each discovered shortcut against every saved app: {evaluations} identities for {size} sources"
        );
        assert_eq!(store.apps().unwrap().len(), size);
        assert_eq!(store.app(&important.id).unwrap().policy, Policy::Always);
        assert_eq!(store.app(&excluded.id).unwrap().policy, Policy::Never);
        assert_eq!(store.app(&important.id).unwrap().source, important.source);
        assert_eq!(
            serde_json::to_value(store.get::<State>("state").unwrap()).unwrap(),
            serde_json::to_value(&owned_before).unwrap()
        );
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            history_before
        );
        let plan = preview(&store, false).unwrap();
        assert!(plan.added.is_empty() && plan.removed.is_empty());
        assert_eq!(std::fs::read(&important.source).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&quick.path).unwrap(), quick_bytes);
        rollback(&store).unwrap();
        assert!(!Path::new(&quick.path).exists());
        assert_eq!(std::fs::read(&important.source).unwrap(), original_bytes);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
}
