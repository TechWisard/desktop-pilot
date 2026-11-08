//! Real filesystem/SQLite regressions for multipart downloads on a desktop.
//! Payloads are opaque fixtures: these checks exercise organization, not extraction.
use super::*;
use std::fs as disk;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = disk::remove_dir_all(&self.0);
    }
}

struct Fixture {
    desktop: PathBuf,
    store: Store,
    _com: native::Com,
    _cleanup: Cleanup,
}
impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-archive-sets-{}",
            fs::operation_id().unwrap()
        ));
        let store = Store::open(&home).unwrap();
        let desktop = home.join("Desktop %USERNAME%");
        disk::create_dir(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("config", &Config::default()).unwrap();
        Self {
            desktop,
            store,
            _com: native::Com::new().unwrap(),
            _cleanup: Cleanup(home),
        }
    }

    fn write(&self, name: &str) {
        disk::write(self.desktop.join(name), format!("opaque fixture: {name}\n")).unwrap();
    }

    fn intact(&self, name: &str) {
        assert_eq!(
            disk::read(self.desktop.join(name)).unwrap(),
            format!("opaque fixture: {name}\n").as_bytes()
        );
    }

    fn plan(&self, files: bool) -> Plan {
        analyze(
            &self.store,
            Options {
                include_files: files,
            },
            &Config::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }
}

#[test]
fn multipart_archive_sets_survive_real_apply_undo_and_store_reopen() {
    let f = Fixture::new();
    let protected = [
        "Backup %USERNAME%.ZIP",
        "Backup %USERNAME%.z01",
        "Backup %USERNAME%.z02",
        "Legacy.rar",
        "Legacy.r00",
        "Legacy.r01",
        "Съёмка.part001.rar",
        "Съёмка.part002.rar",
        "Numeric.000.rar",
        "Numeric.001.rar",
    ];
    for name in protected {
        f.write(name);
    }
    for name in ["Independent.zip", "Other.7z"] {
        f.write(name);
    }
    let important = f.desktop.join("Important.url");
    disk::write(
        &important,
        "[InternetShortcut]\r\nURL=https://example.invalid/important\r\n",
    )
    .unwrap();
    let mut app = native::read_link(&important).unwrap();
    app.policy = Policy::Always;
    app.note = "Keep the manual decision".into();
    f.store.put_app(&app).unwrap();
    f.store.observe(&app.id, 321, true, false).unwrap();
    let days = serde_json::to_value(f.store.days().unwrap()).unwrap();
    let stamps: Vec<_> = protected
        .iter()
        .map(|name| fs::inspect(&f.desktop.join(name), true).unwrap())
        .collect();
    let changes: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();

    assert!(f.plan(false).items.is_empty());
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (2, 1, 1));
    for name in protected {
        assert!(!plan.items.iter().any(|item| item.name == name));
    }
    let after: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(changes, after, "analysis must not write rules or history");
    let folder = plan.destination(&plan.items[0]);
    let outcome = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap();
    assert_eq!(outcome.moved, 2);
    assert_eq!(outcome.kept, 0);
    for (name, stamp) in protected.iter().zip(&stamps) {
        f.intact(name);
        assert_eq!(fs::inspect(&f.desktop.join(name), true).unwrap(), *stamp);
    }
    assert!(f.desktop.join(&folder).join("Independent.zip").is_file());
    assert!(f.desktop.join(&folder).join("Other.7z").is_file());

    // A new SQLite connection must see the same durable Undo and manual state.
    // This is not a claim of a physical power-loss or Explorer topology test.
    let reopened = Store::open(&f.store.home).unwrap();
    let id = status(&reopened).unwrap().undo.unwrap();
    assert_eq!(undo(&reopened, &id).unwrap().restored, 2);
    assert!(!status(&reopened).unwrap().pending);
    assert!(status(&reopened).unwrap().undo.is_none());
    assert!(!f.desktop.join(folder).exists());
    for name in protected.into_iter().chain(["Independent.zip", "Other.7z"]) {
        f.intact(name);
    }
    assert_eq!(reopened.app(&app.id).unwrap(), app);
    assert_eq!(
        serde_json::to_value(reopened.days().unwrap()).unwrap(),
        days
    );
}

#[test]
fn late_archive_volumes_reject_stale_preview_before_any_file_moves() {
    for (first, later) in [
        ("Late.zip", "Late.z01"),
        ("Legacy.rar", "Legacy.r00"),
        ("Numeric.000.rar", "Numeric.001.rar"),
    ] {
        let f = Fixture::new();
        f.write(first);
        f.write("Independent.7z");
        let keep = vec!["unrelated-saved-identity".to_owned()];
        f.store.set(KEEP, &keep).unwrap();
        let plan = f.plan(true);
        assert_eq!(plan.summary(), (2, 1, 1));
        let folder = plan.destination(&plan.items[0]);
        f.write(later);
        let result = apply(&f.store, &plan, &AtomicBool::new(false));
        assert!(result.is_err(), "a late volume must invalidate {first}");
        assert!(format!("{:#}", result.unwrap_err()).contains("Associated file set"));
        for name in [first, later, "Independent.7z"] {
            f.intact(name);
        }
        assert!(!f.desktop.join(folder).exists());
        assert!(!status(&f.store).unwrap().pending);
        assert!(status(&f.store).unwrap().undo.is_none());
        assert_eq!(f.store.get::<Vec<String>>(KEEP).unwrap(), keep);
        let refreshed = f.plan(true);
        assert!(!refreshed.items.iter().any(|item| item.name == first));
        assert_eq!(refreshed.summary(), (0, 0, 0));
    }
}

#[test]
fn incomplete_rar_download_and_manual_keep_survive_later_arrival_and_undo() {
    let f = Fixture::new();
    for name in ["Download.part001.rar", "Keep.zip", "Move.7z", "Other.tar"] {
        f.write(name);
    }
    let mut plan = f.plan(true);
    assert!(plan.skipped.iter().any(|item| {
        item.name == "Download.part001.rar" && item.reason.contains("Associated file set")
    }));
    plan.items
        .iter_mut()
        .find(|item| item.name == "Keep.zip")
        .unwrap()
        .selected = false;
    f.write("Download.part002.rar");
    let outcome = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap();
    assert_eq!((outcome.moved, outcome.kept), (2, 1));
    for name in ["Download.part001.rar", "Download.part002.rar", "Keep.zip"] {
        f.intact(name);
    }
    let id = status(&f.store).unwrap().undo.unwrap();
    assert_eq!(undo(&f.store, &id).unwrap().restored, 2);
    let fresh = f.plan(true);
    assert!(
        fresh
            .skipped
            .iter()
            .any(|item| { item.name == "Keep.zip" && item.reason.contains("You chose to keep") })
    );
    assert_eq!(fresh.summary(), (2, 1, 1));
    for name in [
        "Download.part001.rar",
        "Download.part002.rar",
        "Keep.zip",
        "Move.7z",
        "Other.tar",
    ] {
        f.intact(name);
    }
}
