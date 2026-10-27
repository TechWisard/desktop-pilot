use super::*;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc,
};

/// A scanner stuck on slow I/O must not monopolize foreground-use collection,
/// manual commands or shutdown. The test holds the actual production refresh
/// at its filesystem boundary; it does not replace the Store or command path.
#[test]
fn slow_catalog_read_cannot_delay_manual_policy_heartbeat_or_shutdown() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-slow-worker-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let source = desktop.join("Long lived.url");
    std::fs::write(
        &source,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Original\r\n",
    )
    .unwrap();
    let _com = native::Com::new().unwrap();
    let app = native::read_link(&source).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                slots: 2,
                learning_enabled: false,
                automatic: false,
                collect: false,
                autostart: false,
                ..Default::default()
            },
        )
        .unwrap();
    store.put_app(&app).unwrap();

    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (completed_tx, completed_rx) = mpsc::channel();
    {
        let mut gate = TEST_SCAN_GATE.lock().unwrap();
        assert!(gate.is_none());
        *gate = Some(TestScanGate {
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
    entered_rx
        .recv_timeout(Duration::from_secs(15))
        .expect("Worker must reach its real catalog scan");

    let first_heartbeat: i64 = store.get("heartbeat").unwrap();
    assert!(
        first_heartbeat > 0,
        "Worker must publish readiness before scanning"
    );
    store.command(&format!("policy:{}:Never", app.id)).unwrap();
    let command_id = store.db.last_insert_rowid();
    let started = Instant::now();
    let mut policy_done_in_time = false;
    while started.elapsed() < Duration::from_secs(7) {
        let status: String = store
            .db
            .query_row(
                "SELECT status FROM commands WHERE id=?1",
                [command_id],
                |row| row.get(0),
            )
            .unwrap();
        if status == "done" {
            policy_done_in_time = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // Continue holding the filesystem boundary for a full heartbeat interval.
    while started.elapsed() < Duration::from_secs(19) {
        std::thread::sleep(Duration::from_millis(100));
    }
    let refreshed_heartbeat: i64 = store.get("heartbeat").unwrap();
    let heartbeat_advanced = refreshed_heartbeat >= first_heartbeat + 15;

    // A later user edit during shutdown must never be committed by the old
    // scanner after its owning monitor has exited.
    let updated = b"[InternetShortcut]\r\nURL=https://example.invalid/UserChanged\r\n";
    std::fs::write(&source, updated).unwrap();
    store.set("shutdown", &true).unwrap();
    let stopped_while_scan_waited = match worker_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(outcome) => {
            outcome.expect("Background monitor must stop successfully");
            true
        }
        Err(mpsc::RecvTimeoutError::Timeout) => false,
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            panic!("Background monitor exited without reporting its result")
        }
    };
    let _ = release_tx.send(());
    completed_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("The blocked scan must eventually finish");
    if !stopped_while_scan_waited {
        worker_rx
            .recv_timeout(Duration::from_secs(8))
            .expect("Worker must exit after the scan resumes")
            .expect("Background monitor must stop successfully");
    }
    worker.join().unwrap();

    let saved = store.app(&app.id).unwrap();
    let original_kept = saved.valid && saved.target == app.target;
    let rule_kept = saved.policy == Policy::Never;
    let original_bytes_kept = std::fs::read(&source).unwrap() == updated;
    drop(store);
    std::fs::remove_dir_all(&home).unwrap();

    assert!(
        policy_done_in_time,
        "A slow catalog scan blocked the user's policy command"
    );
    assert!(
        heartbeat_advanced,
        "A slow catalog scan stopped normal heartbeat updates"
    );
    assert!(
        stopped_while_scan_waited,
        "Shutdown waited for an unrelated slow catalog scan"
    );
    assert!(
        rule_kept,
        "Explicit Never rule must survive a concurrent scan"
    );
    assert!(
        original_kept,
        "An abandoned old scan must not commit stale catalog changes"
    );
    assert!(
        original_bytes_kept,
        "Desktop Pilot must not edit an original shortcut"
    );
}
