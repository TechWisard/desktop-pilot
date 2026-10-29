use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

struct CountedHook(Arc<AtomicUsize>);
impl Drop for CountedHook {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn with_store(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-hook-recovery-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("pins_verified", &true).unwrap();
    store.set("shutdown", &false).unwrap();
    store.set("background_disabled", &false).unwrap();
    store
        .set(
            "config",
            &Config {
                collect: true,
                automatic: false,
                learning_enabled: false,
                respect_taskbar_pins: false,
                ..Config::default()
            },
        )
        .unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn rejected_hooks_keep_manual_shortcuts_and_undo_operational_then_recover() {
    with_store(|store| {
        let original = store.home.join("Protected.url");
        let bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Protected\r\n";
        std::fs::write(&original, bytes).unwrap();
        let mut app = native::read_link(&original).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        store.observe(&app.id, 300, true, false).unwrap();
        let prior_days = serde_json::to_value(store.days().unwrap()).unwrap();
        let drops = Arc::new(AtomicUsize::new(0));
        let mut hooks = InputHookRuntime::<CountedHook>::default();
        let mut status_warning = None;

        // The previous implementation propagated this exact installer error
        // through monitor's '?' and terminated all background/manual commands.
        let (ready, warning) = hooks
            .poll(store, true, || -> Result<CountedHook> {
                anyhow::bail!("simulated OS hook registration refusal")
            })
            .unwrap();
        assert!(!ready);
        assert!(hooks.hooks.is_none());
        let warning = warning.unwrap();
        assert!(warning.contains("simulated OS hook registration refusal"));
        publish_hook_warning(store, &mut status_warning, Some(&warning), true).unwrap();
        let saved_status: String = store.get("status").unwrap();
        assert!(saved_status.starts_with("Изменения приостановлены:"));
        let mut attempted = false;
        let (ready, again) = hooks
            .poll(store, true, || {
                attempted = true;
                Ok(CountedHook(drops.clone()))
            })
            .unwrap();
        assert!(!ready && again.as_deref() == Some(warning.as_str()));
        assert!(!attempted, "retries must not run on every worker tick");

        // Existing real SQLite and desktop mutation paths remain usable.
        let applied = apply(store, true).unwrap();
        assert!(!applied.is_empty());
        assert!(store.get::<State>("state").unwrap().owned.len() == 1);
        command(store, "rollback").unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
        assert_eq!(
            serde_json::to_value(store.days().unwrap()).unwrap(),
            prior_days
        );

        // Retry after the bounded interval; a valid hook then resumes
        // observation without manufacturing a usage sample or editing files.
        hooks.retry_at = Some(Instant::now() - INPUT_HOOK_RETRY - Duration::from_secs(1));
        let (ready, warning) = hooks
            .poll(store, true, || Ok(CountedHook(drops.clone())))
            .unwrap();
        assert!(ready && warning.is_none());
        assert!(hooks.hooks.is_some());
        publish_hook_warning(store, &mut status_warning, None, true).unwrap();
        assert_eq!(status_warning, None);
        assert_eq!(
            store.get::<String>("status").unwrap(),
            "Фоновый учёт работает"
        );

        let mut config: Config = store.get("config").unwrap();
        config.collect = false;
        store.set("config", &config).unwrap();
        let (ready, warning) = hooks
            .poll(store, false, || -> Result<CountedHook> {
                panic!("manual-only mode must not install hooks")
            })
            .unwrap();
        assert!(!ready && warning.is_none());
        assert!(hooks.hooks.is_none());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert_eq!(store.get::<Config>("config").unwrap(), config);
        assert_eq!(std::fs::read(&original).unwrap(), bytes);
    });
}

#[test]
fn storage_failure_is_not_misreported_as_a_recoverable_hook_error() {
    with_store(|store| {
        let mut hooks = InputHookRuntime::<CountedHook>::default();
        store
            .set("config", &serde_json::json!("broken config"))
            .unwrap();
        let mut attempted = false;
        let result = hooks.poll(store, true, || {
            attempted = true;
            Ok(CountedHook(Arc::new(AtomicUsize::new(0))))
        });
        assert!(
            result.is_err(),
            "corrupt persisted settings must remain fatal"
        );
        assert!(!attempted);
        assert!(hooks.retry_at.is_none());
    });
}
