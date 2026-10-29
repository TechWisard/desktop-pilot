use super::*;
use std::{
    sync::{Mutex, mpsc},
    time::Duration,
};

pub(super) struct Pause {
    home: PathBuf,
    arrived: mpsc::Sender<()>,
    resume: mpsc::Receiver<()>,
}

static PAUSE: Mutex<Option<Pause>> = Mutex::new(None);

pub(super) fn pause_before_change(home: &Path) {
    let gate = PAUSE.lock().unwrap();
    if let Some(gate) = gate.as_ref().filter(|gate| gate.home == home) {
        gate.arrived.send(()).expect("direct CLI action reached the boundary");
        gate.resume.recv_timeout(Duration::from_secs(15))
            .expect("direct CLI checkpoint must be released");
    }
}

#[test]
fn direct_cli_rollback_cannot_overlap_an_explicit_start() {
    // Actual production direct_cli_change, native SQLite, Win32 named mutex,
    // inert source shortcut, placement and Undo. No user desktop is touched.
    let home = std::env::temp_dir().join(format!(
        "test-desktop-pilot-cli-startup-{}-{}", std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)
            .unwrap().as_nanos()
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    store.set("test_desktop", &desktop.to_string_lossy()).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("pins_verified", &true).unwrap();
    store.set("background_disabled", &true).unwrap();
    store.set("config", &Config {
        learning_enabled: false, automatic: false, collect: false,
        respect_taskbar_pins: false, ..Config::default()
    }).unwrap();
    let source = home.join("Important.url");
    let source_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Important\r\n";
    std::fs::write(&source, source_bytes).unwrap();
    let mut app = native::read_link(&source).unwrap();
    app.policy = Policy::Always;
    store.put_app(&app).unwrap();
    engine::apply(&store, true).unwrap();
    let before: State = store.get("state").unwrap();
    assert_eq!(before.owned.len(), 1);
    let managed_path = before.owned[0].path.clone();

    let (arrived_tx, arrived_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    *PAUSE.lock().unwrap() = Some(Pause {
        home: home.clone(), arrived: arrived_tx, resume: resume_rx
    });
    let worker_home = home.clone();
    let worker = std::thread::spawn(move || -> Result<String> {
        let store = Store::open(&worker_home)?;
        direct_cli_change(&store, || {
            let _changes = native::exclusive("changes", &store.home)?;
            engine::rollback(&store)
        })
    });
    arrived_rx.recv_timeout(Duration::from_secs(15))
        .expect("CLI rollback reached the production boundary");
    // The explicit Settings Start operation acquires this same mutex.
    // It cannot be permitted between the CLI's stopped check and Undo.
    let start_can_interleave = native::try_exclusive("startup", &home).unwrap().is_some();
    resume_tx.send(()).unwrap();
    worker.join().unwrap().unwrap();
    PAUSE.lock().unwrap().take();
    assert!(!std::path::Path::new(&managed_path).exists());
    assert_eq!(std::fs::read(&source).unwrap(), source_bytes);
    assert!(store.get::<bool>("background_disabled").unwrap());
    assert!(store.get::<State>("state").unwrap().owned.is_empty());
    assert!(!native::is_running("background", &home).unwrap());
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
    assert!(
        !start_can_interleave,
        "another window may start background work after CLI accepts direct Undo but before it runs"
    );
}
