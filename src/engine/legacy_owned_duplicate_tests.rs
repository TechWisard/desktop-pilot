use super::*;
use crate::store::Store;
use std::sync::atomic::{AtomicU64, Ordering};

static SERIAL: AtomicU64 = AtomicU64::new(0);
const ORIGINAL: &[u8] = b"[InternetShortcut]\r\nURL=https://example.invalid/legacy-owned\r\n";

fn with_profile(test: impl FnOnce(&Store, &App, &Path, &Path)) {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-legacy-owned-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let root = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = root.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let original = root.join("Original.url");
    std::fs::write(&original, ORIGINAL).unwrap();
    let store = Store::open(&root).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Default::default()
            },
        )
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    let app = native::read_link(&original).unwrap();
    store.put_app(&app).unwrap();
    test(&store, &app, &desktop, &original);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn owned(app: &App, path: &Path) -> Owned {
    Owned {
        app: app.id.clone(),
        path: path.to_string_lossy().into_owned(),
        sha: fingerprint(path).unwrap(),
        position: Point::default(),
        manual: false,
        since: now() - 86_400,
        file_id: None,
    }
}

fn save_legacy(store: &Store, entries: Vec<Owned>) {
    store
        .set(
            "state",
            &State {
                owned: entries,
                initialized: true,
                ..Default::default()
            },
        )
        .unwrap();
}

#[test]
fn repeated_ownership_row_is_collapsed_before_never_apply_and_undo() {
    with_profile(|store, app, desktop, original| {
        let copy = desktop.join("Quick.url");
        std::fs::copy(original, &copy).unwrap();
        let row = owned(app, &copy);
        save_legacy(store, vec![row.clone(), row]);
        store.policy(&app.id, Policy::Never).unwrap();

        let proposed = preview(store, false).unwrap();
        assert!(proposed.selected.is_empty());
        assert_eq!(proposed.removed.len(), 2);
        apply(store, false).unwrap();
        assert!(!copy.exists());
        assert_eq!(std::fs::read(original).unwrap(), ORIGINAL);
        assert!(store.get::<State>("state").unwrap().owned.is_empty());

        rollback(store).unwrap();
        let restored: State = store.get("state").unwrap();
        assert_eq!(restored.owned.len(), 1);
        assert_eq!(restored.owned[0].app, app.id);
        assert_eq!(std::fs::read(&copy).unwrap(), ORIGINAL);
        assert_eq!(std::fs::read(original).unwrap(), ORIGINAL);
    });
}

#[test]
fn distinct_legacy_copies_of_one_app_have_separate_backups_and_restore() {
    with_profile(|store, app, desktop, original| {
        let first = desktop.join("Quick.url");
        let second = desktop.join("Other-quick.url");
        std::fs::copy(original, &first).unwrap();
        std::fs::copy(original, &second).unwrap();
        save_legacy(store, vec![owned(app, &first), owned(app, &second)]);
        store.policy(&app.id, Policy::Never).unwrap();

        let proposed = preview(store, false).unwrap();
        assert!(proposed.selected.is_empty());
        assert_eq!(proposed.removed.len(), 2);
        apply(store, false).unwrap();
        assert!(!first.exists() && !second.exists());
        assert_eq!(std::fs::read(original).unwrap(), ORIGINAL);

        rollback(store).unwrap();
        let restored: State = store.get("state").unwrap();
        assert_eq!(restored.owned.len(), 2);
        assert_eq!(std::fs::read(&first).unwrap(), ORIGINAL);
        assert_eq!(std::fs::read(&second).unwrap(), ORIGINAL);
        assert_eq!(std::fs::read(original).unwrap(), ORIGINAL);
    });
}

#[test]
fn conflicting_ownership_metadata_for_one_path_stops_without_changes() {
    with_profile(|store, app, desktop, original| {
        let copy = desktop.join("Quick.url");
        std::fs::copy(original, &copy).unwrap();
        let first = owned(app, &copy);
        let mut conflicting = first.clone();
        conflicting.manual = true;
        save_legacy(store, vec![first, conflicting]);
        store.policy(&app.id, Policy::Never).unwrap();

        let error = apply(store, false).unwrap_err();
        assert!(
            format!("{error:#}").contains("Конфликт записей управления"),
            "{error:#}"
        );
        assert_eq!(store.get::<State>("state").unwrap().owned.len(), 2);
        assert_eq!(std::fs::read(&copy).unwrap(), ORIGINAL);
        assert_eq!(std::fs::read(original).unwrap(), ORIGINAL);
    });
}
