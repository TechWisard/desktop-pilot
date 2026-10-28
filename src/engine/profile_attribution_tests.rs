use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_browser_shortcut(arguments: &str, test: impl FnOnce(&Store, &Path, &App)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-profile-attribution-{}-{}-{}",
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
    store.set("setup_complete", &true).unwrap();
    store.set("pins_verified", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                automatic: false,
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Default::default()
            },
        )
        .unwrap();
    // Never run this inert file. Actual Windows .lnk parsing is what matters.
    let executable = home.join("browser.exe");
    std::fs::write(&executable, b"inert fixture").unwrap();
    let original = home.join("Work.lnk");
    let _com = native::Com::new().unwrap();
    native::make_link(&original, &executable, arguments, "", 0, "work").unwrap();
    let app = native::read_link(&original).unwrap();
    assert_eq!(app.arguments, arguments);
    store.put_app(&app).unwrap();
    test(&store, &executable, &app);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn sample_and_store(store: &Store, app: Option<String>) {
    // Match the same production decision to account for a foreground process.
    // SQLite Store and the actual ranking/preview paths are exercised.
    if let Some(id) = app {
        let mut checkpoint = HashMap::new();
        record_usage(store, &mut checkpoint, &id, 600, true, now()).unwrap();
    }
}

#[test]
fn a_single_profile_specific_shortcut_cannot_claim_unqualified_executable_time() {
    with_browser_shortcut("--profile-directory=Work", |store, executable, work| {
        let original = std::fs::read(&work.source).unwrap();
        let apps = store.apps().unwrap();
        let observed = observe_match(&apps, &executable.to_string_lossy());
        assert_eq!(
            observed, None,
            "A process path does not reveal its browser profile"
        );
        sample_and_store(store, observed);
        assert!(store.days().unwrap().is_empty());
        let choice = preview(store, true).unwrap();
        assert!(choice.selected.is_empty());
        assert_eq!(std::fs::read(&work.source).unwrap(), original);
    });
}

#[test]
fn a_missing_competing_profile_cannot_make_another_profile_unambiguous() {
    with_browser_shortcut("--profile-directory=Work", |store, executable, work| {
        let other = store.home.join("Personal.lnk");
        native::make_link(
            &other,
            executable,
            "--profile-directory=Personal",
            "",
            0,
            "personal",
        )
        .unwrap();
        let mut personal = native::read_link(&other).unwrap();
        assert_ne!(personal.id, work.id);
        std::fs::remove_file(&other).unwrap();
        personal.valid = false;
        personal.unavailable_since = now();
        store.put_app(&personal).unwrap();
        let observed = observe_match(&store.apps().unwrap(), &executable.to_string_lossy());
        assert_eq!(
            observed, None,
            "An unavailable shortcut is not proof that its process cannot run"
        );
        sample_and_store(store, observed);
        assert!(store.days().unwrap().is_empty());
        assert_eq!(store.app(&work.id).unwrap().policy, Policy::Auto);
        assert_eq!(store.app(&personal.id).unwrap().policy, Policy::Auto);
    });
}

#[test]
fn a_plain_shortcut_does_not_claim_a_known_parameterized_profile() {
    with_browser_shortcut("", |store, executable, plain| {
        let profile = store.home.join("Private.lnk");
        native::make_link(
            &profile,
            executable,
            "--profile-directory=Private",
            "",
            0,
            "private",
        )
        .unwrap();
        let mut private = native::read_link(&profile).unwrap();
        private.valid = false;
        store.put_app(&private).unwrap();
        let observed = observe_match(&store.apps().unwrap(), &executable.to_string_lossy());
        assert_eq!(observed, None);
        sample_and_store(store, observed);
        assert!(store.days().unwrap().is_empty());
        assert_eq!(store.app(&plain.id).unwrap().policy, Policy::Auto);
    });
}

#[test]
fn a_truly_unambiguous_plain_shortcut_still_learns_and_undoes_safely() {
    with_browser_shortcut("", |store, executable, plain| {
        let original = std::fs::read(&plain.source).unwrap();
        let observed = observe_match(&store.apps().unwrap(), &executable.to_string_lossy());
        assert_eq!(observed, Some(plain.id.clone()));
        sample_and_store(store, observed);
        assert_eq!(store.days().unwrap().len(), 1);
        let choice = preview(store, true).unwrap();
        assert_eq!(choice.selected, [plain.id.clone()]);
        apply(store, true).unwrap();
        let state: State = store.get("state").unwrap();
        assert_eq!(state.owned.len(), 1);
        let copy = state.owned[0].path.clone();
        assert_eq!(std::fs::read(&copy).unwrap(), original);
        command(store, "rollback").unwrap();
        assert!(!Path::new(&copy).exists());
        assert_eq!(std::fs::read(&plain.source).unwrap(), original);
    });
}
