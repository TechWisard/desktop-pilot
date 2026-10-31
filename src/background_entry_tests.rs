use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

static HOME_SERIAL: AtomicU64 = AtomicU64::new(0);

fn test_home_at(timestamp: u128) -> PathBuf {
    // Windows clocks may report the same instant for concurrent tests.
    // Never let parallel tests silently share a profile or delete its files.
    let home = std::env::temp_dir().join(format!(
        "test-desktop-pilot-background-entry-{}-{}-{}",
        std::process::id(),
        timestamp,
        HOME_SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&home).unwrap();
    home
}

fn test_home() -> PathBuf {
    test_home_at(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    )
}

// A short-lived Windows reader (indexer/AV) may outlive SQLite's own handles.
// Retrying only ERROR_SHARING_VIOLATION preserves a genuine cleanup failure:
// no test is allowed to silently leave a temporary user-like profile behind.
fn cleanup_test_home(home: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match std::fs::remove_dir_all(home) {
            Ok(()) => return,
            Err(error)
                if error.raw_os_error() == Some(32) && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(error) => panic!("Cannot remove isolated test profile {}: {error}", home.display()),
        }
    }
}

#[test]
fn cleanup_waits_for_a_transient_windows_sharing_violation() {
    use std::os::windows::fs::OpenOptionsExt;
    use windows::Win32::Storage::FileSystem::FILE_SHARE_READ;

    let home = test_home();
    let held_path = home.join("held.txt");
    std::fs::write(&held_path, b"temporary fixture").unwrap();
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ.0)
        .open(&held_path)
        .unwrap();

    assert_eq!(
        std::fs::remove_dir_all(&home).unwrap_err().raw_os_error(),
        Some(32),
        "Windows must refuse deletion while another handle denies delete sharing"
    );
    let (finished, complete) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        cleanup_test_home(&home);
        finished.send(home).unwrap();
    });
    assert!(
        complete.recv_timeout(Duration::from_millis(150)).is_err(),
        "cleanup must not report success while the file is still locked"
    );
    drop(reader);
    let removed = complete.recv_timeout(Duration::from_secs(5)).unwrap();
    worker.join().unwrap();
    assert!(!removed.exists(), "a released lock must leave no test files");
}

#[test]
fn simultaneous_fixture_creation_with_the_same_clock_tick_is_isolated() {
    let jobs: Vec<_> = (0..16)
        .map(|_| std::thread::spawn(|| test_home_at(123_456_789)))
        .collect();
    let mut paths = std::collections::HashSet::new();
    for job in jobs {
        let home = job.join().unwrap();
        assert!(
            paths.insert(home.clone()),
            "parallel test profiles must never reuse the same directory"
        );
        std::fs::remove_dir(home).unwrap();
    }
}

#[test]
fn a_disabled_profile_cannot_be_resurrected_by_a_late_background_entry() {
    // Exercise the same SQLite transition used by the production --background
    // entry, after an actual Turn off. No tray, hooks or user desktop needed.
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    let original = home.join("Important.url");
    let bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/important\r\n";
    std::fs::write(&original, bytes).unwrap();
    let mut app = native::read_link(&original).unwrap();
    app.policy = Policy::Always;
    store.put_app(&app).unwrap();
    store.observe(&app.id, 130, true, false).unwrap();
    setup::disable_background(&home).unwrap();
    store.set("heartbeat", &789i64).unwrap();
    let before = serde_json::json!({
        "disabled": store.get::<bool>("background_disabled").unwrap(),
        "shutdown": store.get::<bool>("shutdown").unwrap(),
        "heartbeat": store.get::<i64>("heartbeat").unwrap(),
        "pid": store.get::<u32>("background_pid").unwrap(),
        "filter": store.get::<String>("input_filter").unwrap(),
        "apps": store.apps().unwrap(),
        "days": store.days().unwrap(),
        "config": store.get::<Config>("config").unwrap(),
        "state": store.get::<State>("state").unwrap(),
    });
    let admitted = register_background_session(&store).unwrap();
    let after = serde_json::json!({
        "disabled": store.get::<bool>("background_disabled").unwrap(),
        "shutdown": store.get::<bool>("shutdown").unwrap(),
        "heartbeat": store.get::<i64>("heartbeat").unwrap(),
        "pid": store.get::<u32>("background_pid").unwrap(),
        "filter": store.get::<String>("input_filter").unwrap(),
        "apps": store.apps().unwrap(),
        "days": store.days().unwrap(),
        "config": store.get::<Config>("config").unwrap(),
        "state": store.get::<State>("state").unwrap(),
    });
    let saved_bytes = std::fs::read(&original).unwrap();
    drop(store);
    cleanup_test_home(&home);
    assert!(
        !admitted,
        "a background launch must respect explicit Turn off"
    );
    assert_eq!(
        before, after,
        "rejected startup must not rewrite the profile"
    );
    assert_eq!(saved_bytes, bytes);
}

#[test]
fn a_permitted_background_entry_can_resume_without_resetting_preferences() {
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("background_disabled", &false).unwrap();
    store.set("shutdown", &true).unwrap();
    store.set("heartbeat", &789i64).unwrap();
    let config: Config = store.get("config").unwrap();
    assert!(register_background_session(&store).unwrap());
    assert!(!store.get::<bool>("shutdown").unwrap());
    assert_eq!(store.get::<i64>("heartbeat").unwrap(), 0);
    assert_eq!(
        store.get::<u32>("background_pid").unwrap(),
        std::process::id()
    );
    assert_eq!(store.get::<Config>("config").unwrap(), config);
    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn explicit_start_grants_background_entry_and_restores_turn_off_after_failure() {
    // Exercise the same start-permission transition used by the real launcher.
    // Only the simulated failed child is injected; SQLite admission is real.
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("background_disabled", &true).unwrap();
    store.set("shutdown", &true).unwrap();
    let failed = setup::with_background_start_permission(&store, || {
        assert!(register_background_session(&store)?);
        anyhow::bail!("simulated child failed readiness")
    });
    assert!(failed.is_err());
    assert!(store.get::<bool>("background_disabled").unwrap());
    assert!(store.get::<bool>("shutdown").unwrap());
    assert!(!register_background_session(&store).unwrap());

    setup::with_background_start_permission(&store, || {
        assert!(register_background_session(&store)?);
        Ok(())
    })
    .unwrap();
    assert!(!store.get::<bool>("background_disabled").unwrap());
    assert!(!store.get::<bool>("shutdown").unwrap());
    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn a_disabled_entry_never_installs_global_input_hooks() {
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    let source = home.join("Protected.url");
    let bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/protected\r\n";
    std::fs::write(&source, bytes).unwrap();
    let mut app = native::read_link(&source).unwrap();
    app.policy = Policy::Always;
    store.put_app(&app).unwrap();
    store.observe(&app.id, 300, true, false).unwrap();
    setup::disable_background(&home).unwrap();
    let before = serde_json::json!({
        "disabled": store.get::<bool>("background_disabled").unwrap(),
        "shutdown": store.get::<bool>("shutdown").unwrap(),
        "heartbeat": store.get::<i64>("heartbeat").unwrap(),
        "input_filter": store.get::<String>("input_filter").unwrap(),
        "apps": store.apps().unwrap(),
        "days": store.days().unwrap(),
    });
    let mut attempts = 0usize;
    let installed = install_input_hooks_if_enabled(&store, || -> Result<()> {
        attempts += 1;
        Ok(())
    })
    .unwrap();
    assert!(
        installed.is_none(),
        "Turn off must refuse the input-hook stage"
    );
    assert_eq!(
        attempts, 0,
        "do not even attempt to monitor input after Turn off"
    );
    assert!(!register_background_session(&store).unwrap());
    let after = serde_json::json!({
        "disabled": store.get::<bool>("background_disabled").unwrap(),
        "shutdown": store.get::<bool>("shutdown").unwrap(),
        "heartbeat": store.get::<i64>("heartbeat").unwrap(),
        "input_filter": store.get::<String>("input_filter").unwrap(),
        "apps": store.apps().unwrap(),
        "days": store.days().unwrap(),
    });
    assert_eq!(before, after);
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn enabled_manual_only_entry_must_not_request_global_input_hooks() {
    // Unlike explicit Turn off, this mode keeps the tray and manual commands
    // available. It has no permission to observe keyboard or mouse activity.
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("background_disabled", &false).unwrap();
    let config = Config {
        collect: false,
        automatic: false,
        learning_enabled: false,
        ..Default::default()
    };
    store.set("config", &config).unwrap();
    let mut attempted = 0;
    let hooks = install_input_hooks_if_enabled(&store, || -> Result<()> {
        attempted += 1;
        Ok(())
    })
    .unwrap();
    assert!(
        hooks.is_none(),
        "manual-only mode needs no global input hooks"
    );
    assert_eq!(attempted, 0, "no hook initializer should be called");
    assert!(register_background_session(&store).unwrap());
    assert!(!store.get::<bool>("shutdown").unwrap());
    assert_eq!(store.get::<Config>("config").unwrap(), config);
    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn turning_off_during_input_hook_setup_still_rejects_session_registration() {
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("background_disabled", &false).unwrap();
    store.set("shutdown", &false).unwrap();
    let mut config: Config = store.get("config").unwrap();
    config.collect = true;
    store.set("config", &config).unwrap();
    let mut attempts = 0usize;
    let installed = install_input_hooks_if_enabled(&store, || -> Result<()> {
        attempts += 1;
        // The user can turn off the program after the first permission read.
        store.set("background_disabled", &true)?;
        store.set("shutdown", &true)?;
        Ok(())
    })
    .unwrap();
    assert!(installed.is_some());
    assert_eq!(attempts, 1);
    assert!(!register_background_session(&store).unwrap());
    assert!(store.get::<bool>("background_disabled").unwrap());
    assert!(store.get::<bool>("shutdown").unwrap());
    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn input_hooks_follow_manual_observe_manual_changes_and_concurrent_turn_off() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct FakeHook(Arc<AtomicUsize>);
    impl Drop for FakeHook {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    // Run the exact production lifecycle against a saved SQLite configuration.
    // The injector stands in for Windows registration so the operation counts
    // are deterministic. No keyboard events or user files are recorded.
    let home = test_home();
    let store = Store::open(&home).unwrap();
    store.set("background_disabled", &false).unwrap();
    store.set("shutdown", &false).unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut current: Option<FakeHook> = None;
    let mut installs = 0usize;
    let mut config = Config {
        collect: false,
        automatic: false,
        learning_enabled: false,
        ..Default::default()
    };

    for enabled in [false, false, true, true, false, true, false] {
        config.collect = enabled;
        store.set("config", &config).unwrap();
        let active = engine::sync_input_hooks(&store, &mut current, enabled, || {
            installs += 1;
            Ok(FakeHook(dropped.clone()))
        })
        .unwrap();
        assert_eq!(active, enabled);
        assert_eq!(current.is_some(), enabled);
    }
    assert_eq!(installs, 2, "steady tracking must not register extra hooks");
    assert_eq!(dropped.load(Ordering::SeqCst), 2);

    // Stop while the real monitor's installation boundary is in progress.
    // A successful OS registration must still be released, never activated.
    config.collect = true;
    store.set("config", &config).unwrap();
    let allowed = engine::sync_input_hooks(&store, &mut current, true, || {
        installs += 1;
        store.set("background_disabled", &true)?;
        store.set("shutdown", &true)?;
        Ok(FakeHook(dropped.clone()))
    })
    .unwrap();
    assert!(!allowed);
    assert!(current.is_none());
    assert_eq!(installs, 3);
    assert_eq!(dropped.load(Ordering::SeqCst), 3);
    assert!(store.get::<bool>("background_disabled").unwrap());
    assert!(store.get::<bool>("shutdown").unwrap());

    drop(store);
    cleanup_test_home(&home);
}

#[test]
fn stale_collect_snapshot_cannot_keep_existing_hooks_after_permission_is_revoked() {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct CountedHook(Arc<AtomicUsize>);
    impl Drop for CountedHook {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    // A different Settings connection can commit a pause after monitor has
    // already read its Config snapshot. Exercise the actual hook lifecycle,
    // persisted SQLite state and the same stale collecting argument.
    let home = test_home();
    let store = Store::open(&home).unwrap();
    let settings = Store::open(&home).unwrap();
    let original = home.join("Keep.url");
    let original_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Keep\r\n";
    std::fs::write(&original, original_bytes).unwrap();
    let mut app = native::read_link(&original).unwrap();
    app.policy = Policy::Always;
    store.put_app(&app).unwrap();
    store.observe(&app.id, 300, true, false).unwrap();
    let saved_apps = store.apps().unwrap();
    let saved_days = store.days().unwrap();
    let mut config = Config {
        collect: true,
        automatic: false,
        ..Default::default()
    };
    store.set("config", &config).unwrap();
    store.set("shutdown", &false).unwrap();
    store.set("background_disabled", &false).unwrap();
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut hooks: Option<CountedHook> = None;
    let installations = std::cell::Cell::new(0usize);

    let sync = |hooks: &mut Option<CountedHook>| {
        engine::sync_input_hooks(&store, hooks, true, || {
            installations.set(installations.get() + 1);
            Ok(CountedHook(dropped.clone()))
        })
        .unwrap()
    };
    assert!(sync(&mut hooks));
    assert!(hooks.is_some());
    assert_eq!(installations.get(), 1);

    // Pausing collection must invalidate a formerly authorized installation
    // even when this monitor iteration still carries an older true snapshot.
    config.collect = false;
    settings.set("config", &config).unwrap();
    assert!(
        !sync(&mut hooks),
        "paused observation must revoke the stale snapshot"
    );
    assert!(hooks.is_none());
    assert_eq!(dropped.load(Ordering::SeqCst), 1);

    config.collect = true;
    settings.set("config", &config).unwrap();
    assert!(sync(&mut hooks));
    assert_eq!(installations.get(), 2);

    // Turn off can also commit after the same snapshot, before the hook poll.
    settings.set("background_disabled", &true).unwrap();
    assert!(
        !sync(&mut hooks),
        "Turn off must revoke already installed hooks"
    );
    assert!(hooks.is_none());
    assert_eq!(dropped.load(Ordering::SeqCst), 2);
    assert_eq!(
        installations.get(),
        2,
        "revocation cannot register new hooks"
    );

    settings.set("background_disabled", &false).unwrap();
    assert!(sync(&mut hooks));
    assert_eq!(installations.get(), 3);
    settings.set("shutdown", &true).unwrap();
    assert!(!sync(&mut hooks), "shutdown must release existing hooks");
    assert!(hooks.is_none());
    assert_eq!(dropped.load(Ordering::SeqCst), 3);

    assert_eq!(store.apps().unwrap(), saved_apps);
    assert_eq!(
        serde_json::to_value(store.days().unwrap()).unwrap(),
        serde_json::to_value(saved_days).unwrap()
    );
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    drop(settings);
    drop(store);
    cleanup_test_home(&home);
}
