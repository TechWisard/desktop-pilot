use super::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};

fn status(store: &Store, id: i64) -> String {
    store.db.query_row(
        "SELECT status FROM commands WHERE id=?1",
        [id],
        |row| row.get::<_, String>(0),
    ).unwrap()
}

#[test]
fn rescan_requested_after_enumeration_must_run_again_before_reporting_success() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-deferred-rescan-{}-{}-{}",
        std::process::id(), now(), SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&root).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let original = desktop.join("Existing.url");
    let before = b"[InternetShortcut]\r\nURL=https://example.invalid/Existing\r\n";
    std::fs::write(&original, before).unwrap();
    let _com = native::Com::new().unwrap();
    let store = Store::open(&home).unwrap();
    store.set("test_desktop", &desktop.to_string_lossy()).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("config", &Config {
        automatic: false,
        collect: false,
        learning_enabled: false,
        autostart: false,
        ..Default::default()
    }).unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (completed_tx, completed_rx) = mpsc::channel();
    {
        let mut slot = TEST_POST_DISCOVERY_GATE.lock().unwrap();
        assert!(slot.is_none());
        *slot = Some(TestScanGate {
            home: home.clone(),
            entered: entered_tx,
            release: release_rx,
            completed: completed_tx,
        });
    }
    let (worker_tx, worker_rx) = mpsc::channel();
    let worker_home = home.clone();
    let worker = std::thread::spawn(move || {
        let _ = worker_tx.send(monitor(worker_home));
    });
    entered_rx.recv_timeout(Duration::from_secs(15))
        .expect("The real first scan must finish enumerating the directory");

    // The first scan's file list is already fixed. A new file and two explicit
    // refresh commands belong to a later generation, not the old snapshot.
    let added = desktop.join("Added after enumeration.url");
    let after = b"[InternetShortcut]\r\nURL=https://example.invalid/NewProject\r\n";
    std::fs::write(&added, after).unwrap();
    let expected = native::read_link(&added).unwrap();
    let ids: Vec<i64> = (0..2).map(|_| {
        store.command("rescan").unwrap();
        store.db.last_insert_rowid()
    }).collect();
    let start = Instant::now();
    let mut both_claimed = false;
    while start.elapsed() < Duration::from_secs(12) {
        if ids.iter().all(|id| status(&store, *id) == "running") {
            both_claimed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(75));
    }
    release_tx.send(()).unwrap();
    completed_rx.recv_timeout(Duration::from_secs(10))
        .expect("The first scan must leave its boundary");

    let start = Instant::now();
    let mut both_done = false;
    while start.elapsed() < Duration::from_secs(16) {
        if ids.iter().all(|id| status(&store, *id) == "done") {
            both_done = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(75));
    }
    let saved = store.apps().unwrap();
    let discovered = saved.iter().any(|app| app.id == expected.id && app.source == expected.source);
    let unchanged = std::fs::read(&original).unwrap() == before
        && std::fs::read(&added).unwrap() == after;
    let state: State = store.get("state").unwrap();
    store.set("shutdown", &true).unwrap();
    worker_rx.recv_timeout(Duration::from_secs(8))
        .expect("Monitor must stop after test")
        .expect("Monitor should stop successfully");
    worker.join().unwrap();
    drop(store);
    std::fs::remove_dir_all(&root).unwrap();
    assert!(both_claimed, "Both real queued refresh requests must be claimed");
    assert!(both_done, "Both requests must finish after their requested scan");
    assert!(discovered, "An explicit refresh completed using a scan that began before the new shortcut existed");
    assert!(unchanged, "Refreshing must never edit original shortcut files");
    assert!(state.owned.is_empty(), "Manual mode must not create automatic placements");
}
