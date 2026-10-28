use super::*;
use std::sync::mpsc;

/// The actual background loop must notice a newly imported catalog identity
/// without needing a six-hour filesystem rescan or an unrelated user command.
#[test]
fn background_observes_newly_imported_app_without_waiting_for_next_scan() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-live-import-{}-{}-{}",
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
    store
        .set(
            "config",
            &Config {
                automatic: false,
                collect: false,
                autostart: false,
                learning_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();
    let (observed_tx, observed_rx) = mpsc::channel();
    {
        let mut observer = TEST_MONITOR_CATALOG.lock().unwrap();
        assert!(observer.is_none());
        *observer = Some(TestMonitorCatalogObserver {
            home: home.clone(),
            sender: observed_tx,
        });
    }
    let worker_home = home.clone();
    let worker = std::thread::spawn(move || monitor(worker_home));

    // Finish the ordinary first scan; the next routine scan is six hours away.
    let mut started = false;
    let initial_deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < initial_deadline {
        match observed_rx.recv_timeout(Duration::from_millis(250)) {
            Ok((true, entries)) => {
                assert!(entries.is_empty());
                started = true;
                break;
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    assert!(started, "The first real catalog scan must complete");

    // A separate SQLite connection matches the Settings import writer.
    // The follow-up UI test proves the actual import publishes this marker.
    let source = home.join("New work profile.url");
    let original = b"[InternetShortcut]\r\nURL=https://example.invalid/NewWorkProfile\r\n";
    std::fs::write(&source, original).unwrap();
    let imported = native::read_link(&source).unwrap();
    store
        .transaction(|| {
            store.put_app(&imported)?;
            store.set("catalog_epoch", &1u64)
        })
        .unwrap();

    let deadline = Instant::now() + Duration::from_secs(7);
    let mut noticed = false;
    while Instant::now() < deadline {
        match observed_rx.recv_timeout(Duration::from_millis(250)) {
            Ok((_, entries)) if entries.contains(&imported.id) => {
                noticed = true;
                break;
            }
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    store.set("shutdown", &true).unwrap();
    worker.join().unwrap().unwrap();
    *TEST_MONITOR_CATALOG.lock().unwrap() = None;
    assert_eq!(store.app(&imported.id).unwrap().source, imported.source);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
    assert!(
        noticed,
        "The running monitor did not learn the newly imported app before the next six-hour scan"
    );
}
