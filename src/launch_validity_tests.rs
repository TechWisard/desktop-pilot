use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_profile(test: impl FnOnce(&Store, &Path)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-launch-validity-{}-{}-{}",
        std::process::id(),
        model::now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let store = Store::open(&home).unwrap();
    test(&store, &home);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn saved_link(store: &Store, home: &Path) -> (App, PathBuf, PathBuf) {
    let executable = home.join("inert.exe");
    std::fs::write(&executable, b"inert fixture: never executed").unwrap();
    let link = home.join("Work profile.lnk");
    native::make_link(&link, &executable, "--profile Work", "", 0, "Work").unwrap();
    let app = native::read_link(&link).unwrap();
    assert!(app.valid);
    store.put_app(&app).unwrap();
    (app, executable, link)
}

#[test]
fn removed_launch_target_is_rejected_then_recovers_without_losing_history() {
    let _com = native::Com::new().unwrap();
    with_profile(|store, home| {
        let (app, executable, link) = saved_link(store, home);
        store.observe(&app.id, 300, true, false).unwrap();
        let days_before = serde_json::to_value(store.days().unwrap()).unwrap();
        assert!(checked_launch_source(store, &app.id).unwrap().valid);
        std::fs::remove_file(&executable).unwrap();
        let unavailable = native::read_link(&link).unwrap();
        assert_eq!(unavailable.identity(), app.identity());
        assert!(!unavailable.valid);
        assert!(
            checked_launch_source(store, &app.id).is_err(),
            "The actual legacy launch guard must reject an unavailable target before ShellExecute"
        );
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            days_before
        );
        assert_eq!(store.app(&app.id).unwrap().policy, app.policy);
        std::fs::write(&executable, b"inert fixture: never executed").unwrap();
        assert!(checked_launch_source(store, &app.id).unwrap().valid);
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            days_before
        );
    });
}

#[test]
fn edited_launch_identity_is_rejected_even_if_new_target_exists() {
    let _com = native::Com::new().unwrap();
    with_profile(|store, home| {
        let (app, _exe, link) = saved_link(store, home);
        let other = home.join("other-inert.exe");
        std::fs::write(&other, b"inert second target").unwrap();
        native::make_link(&link, &other, "--profile Work", "", 0, "Changed").unwrap();
        assert!(native::read_link(&link).unwrap().valid);
        assert!(checked_launch_source(store, &app.id).is_err());
        assert_eq!(store.app(&app.id).unwrap().source, app.source);
    });
}

#[test]
fn deleted_original_link_is_not_treated_as_a_valid_launch() {
    let _com = native::Com::new().unwrap();
    with_profile(|store, home| {
        let (app, _exe, link) = saved_link(store, home);
        std::fs::remove_file(link).unwrap();
        assert!(checked_launch_source(store, &app.id).is_err());
        assert_eq!(store.app(&app.id).unwrap().id, app.id);
    });
}

#[test]
fn healthy_protocol_shortcut_remains_launchable_by_the_guard_without_network_access() {
    let _com = native::Com::new().unwrap();
    with_profile(|store, home| {
        let source = home.join("Documentation.url");
        std::fs::write(
            &source,
            b"[InternetShortcut]\r\nURL=https://example.invalid/guide\r\n",
        )
        .unwrap();
        let app = native::read_link(&source).unwrap();
        assert!(app.valid);
        store.put_app(&app).unwrap();
        let actual = checked_launch_source(store, &app.id).unwrap();
        assert_eq!(actual.identity(), app.identity());
        assert!(actual.valid);
    });
}
