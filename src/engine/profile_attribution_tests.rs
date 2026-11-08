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
        assert_eq!(choice.selected.as_slice(), std::slice::from_ref(&plain.id));
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

#[test]
fn unavailable_competing_profile_with_the_same_process_root_blocks_attribution() {
    with_browser_shortcut("--profile-directory=Work", |store, executable, work| {
        let shared = store.home.join("Shared");
        std::fs::create_dir_all(&shared).unwrap();
        let foreground = shared.join("game.exe");
        std::fs::write(&foreground, b"inert foreground fixture").unwrap();

        let mut work = work.clone();
        work.observed_roots = vec![shared.to_string_lossy().into_owned()];
        store.put_app(&work).unwrap();

        let shortcut = store.home.join("Personal.lnk");
        native::make_link(
            &shortcut,
            executable,
            "--profile-directory=Personal",
            "",
            0,
            "personal",
        )
        .unwrap();
        let mut personal = native::read_link(&shortcut).unwrap();
        assert_ne!(personal.id, work.id);
        personal.observed_roots = work.observed_roots.clone();
        std::fs::remove_file(&shortcut).unwrap();
        personal.valid = false;
        personal.unavailable_since = now();
        store.put_app(&personal).unwrap();

        // A shared folder identifies neither launch profile. The missing
        // Personal shortcut may still have launched the active process.
        let observed = observe_match(&store.apps().unwrap(), &foreground.to_string_lossy());
        assert_eq!(observed, None);
        sample_and_store(store, observed);
        assert!(store.days().unwrap().is_empty());
        assert!(preview(store, true).unwrap().selected.is_empty());

        // Restoring that shortcut must not suddenly change the answer.
        personal.valid = true;
        store.put_app(&personal).unwrap();
        assert_eq!(
            observe_match(&store.apps().unwrap(), &foreground.to_string_lossy()),
            None
        );
    });
}

#[test]
fn different_verified_process_roots_still_credit_the_right_profile() {
    with_browser_shortcut("--profile-directory=Work", |store, executable, work| {
        let work_root = store.home.join("WorkGame");
        let private_root = store.home.join("PrivateGame");
        std::fs::create_dir_all(&work_root).unwrap();
        std::fs::create_dir_all(&private_root).unwrap();
        let foreground = work_root.join("game.exe");
        std::fs::write(&foreground, b"inert foreground fixture").unwrap();

        let mut work = work.clone();
        work.observed_roots = vec![work_root.to_string_lossy().into_owned()];
        store.put_app(&work).unwrap();

        let shortcut = store.home.join("Private.lnk");
        native::make_link(
            &shortcut,
            executable,
            "--profile-directory=Private",
            "",
            0,
            "private",
        )
        .unwrap();
        let mut private = native::read_link(&shortcut).unwrap();
        private.observed_roots = vec![private_root.to_string_lossy().into_owned()];
        private.valid = false;
        private.unavailable_since = now();
        store.put_app(&private).unwrap();

        let observed = observe_match(&store.apps().unwrap(), &foreground.to_string_lossy());
        assert_eq!(observed, Some(work.id.clone()));
        sample_and_store(store, observed);
        let days = store.days().unwrap();
        assert_eq!(days.len(), 1);
        assert_eq!(days[0].app, work.id);
        assert_eq!(preview(store, true).unwrap().selected, vec![work.id]);
    });
}

#[test]
fn unavailable_competing_packaged_app_alias_cannot_claim_foreground_usage() {
    // A real parsed Windows shortcut and SQLite catalog, not a separate
    // mock of the app-ID matcher. Neither inert executable is launched.
    with_browser_shortcut(
        "shell:AppsFolder\\Example.Package!App",
        |store, _executable, work| {
            let app_id = "example.package!app";
            let original = std::fs::read(&work.source).unwrap();
            assert_eq!(
                observe_app_id(&store.apps().unwrap(), app_id),
                Some(work.id.clone())
            );

            // A second saved launch path carries the same application ID,
            // but its original shortcut temporarily disappears.
            let alternate_exe = store.home.join("alternate.exe");
            std::fs::write(&alternate_exe, b"inert second launch path").unwrap();
            let alternate_link = store.home.join("Alternate.lnk");
            native::make_link(
                &alternate_link,
                &alternate_exe,
                "shell:AppsFolder\\Example.Package!App",
                "",
                0,
                "alternate",
            )
            .unwrap();
            let mut alternate = native::read_link(&alternate_link).unwrap();
            assert_ne!(alternate.id, work.id);
            std::fs::remove_file(&alternate_link).unwrap();
            alternate.valid = false;
            alternate.unavailable_since = now();
            store.put_app(&alternate).unwrap();

            let attributed = observe_app_id(&store.apps().unwrap(), app_id);
            assert_eq!(
                attributed, None,
                "Offline saved launch paths still make an App ID ambiguous"
            );
            sample_and_store(store, attributed);
            assert!(store.days().unwrap().is_empty());
            assert!(preview(store, true).unwrap().selected.is_empty());
            assert_eq!(std::fs::read(&work.source).unwrap(), original);

            // Availability changing again cannot resolve identical App IDs.
            native::make_link(
                &alternate_link,
                &alternate_exe,
                "shell:AppsFolder\\Example.Package!App",
                "",
                0,
                "alternate",
            )
            .unwrap();
            alternate.valid = true;
            alternate.unavailable_since = 0;
            store.put_app(&alternate).unwrap();
            assert_eq!(observe_app_id(&store.apps().unwrap(), app_id), None);

            // An explicit exclusion also must not turn the *other* profile
            // into a provably unique foreground launch identity.
            store.policy(&alternate.id, Policy::Never).unwrap();
            assert_eq!(observe_app_id(&store.apps().unwrap(), app_id), None);
            assert_eq!(store.app(&alternate.id).unwrap().policy, Policy::Never);
            assert_eq!(std::fs::read(&work.source).unwrap(), original);
        },
    );
}
