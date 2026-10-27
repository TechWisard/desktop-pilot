use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_managed_shortcut(test: impl FnOnce(&Store, &App, &Owned)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-renamed-owned-scan-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store
        .set(
            "config",
            &Config {
                learning_enabled: false,
                respect_taskbar_pins: false,
                slots: 2,
                ..Default::default()
            },
        )
        .unwrap();
    let original = home.join("Original.url");
    std::fs::write(
        &original,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Work\r\n",
    )
    .unwrap();
    let mut app = native::read_link(&original).unwrap();
    app.policy = Policy::Always;
    app.note = "My permanent work tool".into();
    store.put_app(&app).unwrap();
    store.observe(&app.id, 180, true, false).unwrap();
    apply(&store, true).unwrap();
    let state: State = store.get("state").unwrap();
    assert_eq!(state.owned.len(), 1);
    assert!(state.owned[0].file_id.is_some());
    test(&store, &app, &state.owned[0]);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn renamed_quick_copy_during_original_outage_preserves_source_rule_and_position() {
    with_managed_shortcut(|store, app, owned| {
        let previous = std::fs::read(&app.source).unwrap();
        let renamed = root(store).unwrap().join("My chosen position.url");
        std::fs::rename(&owned.path, &renamed).unwrap();
        let offline = store.home.join("Original.offline");
        std::fs::rename(&app.source, &offline).unwrap();

        refresh_apps(store).unwrap();
        let current = store.app(&app.id).unwrap();
        assert_eq!(
            norm(&current.source),
            norm(&app.source),
            "The program must not turn its renamed quick copy into an original"
        );
        assert!(!current.valid);
        assert_eq!(current.policy, Policy::Always);
        assert_eq!(current.note, app.note);
        assert_eq!(store.apps().unwrap().len(), 1);

        reconcile_manual(store).unwrap();
        let next: State = store.get("state").unwrap();
        assert_eq!(next.owned.len(), 1);
        assert_eq!(norm(&next.owned[0].path), norm(&renamed.to_string_lossy()));
        assert_eq!(next.owned[0].file_id, owned.file_id);
        assert_eq!(next.owned[0].position, owned.position);
        assert!(next.owned[0].manual);
        let plan = preview(store, false).unwrap();
        assert!(plan.added.is_empty() && plan.removed.is_empty());
        assert_eq!(std::fs::read(&renamed).unwrap(), previous);

        std::fs::rename(&offline, &app.source).unwrap();
        refresh_apps(store).unwrap();
        assert!(store.app(&app.id).unwrap().valid);
        assert_eq!(store.app(&app.id).unwrap().source, app.source);
        command(store, &format!("policy:{}:Never", app.id)).unwrap();
        apply(store, false).unwrap();
        assert!(!renamed.exists());
        assert_eq!(std::fs::read(&app.source).unwrap(), previous);
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
    });
}

#[test]
fn renamed_and_edited_managed_file_is_not_discovered_as_another_application() {
    with_managed_shortcut(|store, app, owned| {
        let previous = std::fs::read(&app.source).unwrap();
        let renamed = root(store).unwrap().join("Modified by user.url");
        std::fs::rename(&owned.path, &renamed).unwrap();
        let edited = b"[InternetShortcut]\r\nURL=https://example.invalid/Personal\r\n";
        std::fs::write(&renamed, edited).unwrap();
        assert_eq!(file_identity(&renamed).unwrap(), owned.file_id);

        refresh_apps(store).unwrap();
        assert_eq!(
            store.apps().unwrap().len(),
            1,
            "A user edit to a managed copy is not another opted-in original"
        );
        assert_eq!(store.app(&app.id).unwrap().source, app.source);
        reconcile_manual(store).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
        assert_eq!(std::fs::read(&renamed).unwrap(), edited);
        assert_eq!(std::fs::read(&app.source).unwrap(), previous);
    });
}

#[test]
fn independent_byte_identical_alias_remains_valid_after_original_outage() {
    with_managed_shortcut(|store, app, owned| {
        let alias = root(store).unwrap().join("Independent original.url");
        std::fs::copy(&app.source, &alias).unwrap();
        assert_ne!(file_identity(&alias).unwrap(), owned.file_id);
        let offline = store.home.join("Original.offline");
        std::fs::rename(&app.source, &offline).unwrap();

        refresh_apps(store).unwrap();
        let current = store.app(&app.id).unwrap();
        assert_eq!(norm(&current.source), norm(&alias.to_string_lossy()));
        assert_eq!(current.policy, Policy::Always);
        assert!(current.valid);
        assert_eq!(store.apps().unwrap().len(), 1);
        assert_eq!(
            std::fs::read(&owned.path).unwrap(),
            std::fs::read(&offline).unwrap()
        );
    });
}

#[test]
fn replacing_a_saved_copy_path_does_not_make_its_renamed_file_an_original() {
    with_managed_shortcut(|store, app, owned| {
        let original_bytes = std::fs::read(&app.source).unwrap();
        let renamed = root(store).unwrap().join("Original moved by user.url");
        std::fs::rename(&owned.path, &renamed).unwrap();
        let unrelated = b"[InternetShortcut]\r\nURL=https://example.invalid/Unrelated\r\n";
        std::fs::write(&owned.path, unrelated).unwrap();
        assert_ne!(
            file_identity(Path::new(&owned.path)).unwrap(),
            owned.file_id
        );
        let offline = store.home.join("Original.offline");
        std::fs::rename(&app.source, &offline).unwrap();

        refresh_apps(store).unwrap();
        let saved = store.app(&app.id).unwrap();
        assert_eq!(saved.source, app.source);
        assert!(!saved.valid);
        assert_eq!(saved.policy, Policy::Always);
        assert_eq!(store.apps().unwrap().len(), 1);
        assert_eq!(std::fs::read(&renamed).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&owned.path).unwrap(), unrelated);
        assert_eq!(std::fs::read(&offline).unwrap(), original_bytes);
        reconcile_manual(store).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
        assert!(renamed.exists());
    });
}
