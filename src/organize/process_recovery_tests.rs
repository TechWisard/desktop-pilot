use super::*;
use crate::model::App;
use std::{
    cell::RefCell,
    fs as disk,
    io::Write,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const CHILD_TEST: &str = "organize::tests::process_recovery::crash_process_fixture";
const PROFILE_ENV: &str = "DESKTOP_PILOT_CRASH_FIXTURE_PROFILE";
const NONCE_ENV: &str = "DESKTOP_PILOT_CRASH_FIXTURE_NONCE";
const INDEX_ENV: &str = "DESKTOP_PILOT_CRASH_FIXTURE_INDEX";
const PREFIX: &str = "test-organizer-process-recovery-";

thread_local! {
    // Test-only synchronization. No environment-controlled interruption hook
    // is compiled into the application or its release executable.
    static PAUSE: RefCell<Option<(usize, PathBuf)>> = const { RefCell::new(None) };
}

pub(super) fn checkpoint(index: usize) -> Result<()> {
    let marker = PAUSE.with(|pause| {
        pause
            .borrow()
            .as_ref()
            .filter(|(wanted, _)| *wanted == index)
            .map(|(_, marker)| marker.clone())
    });
    if let Some(marker) = marker {
        let mut file = disk::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(marker)?;
        write!(file, "{index}")?;
        file.sync_all()?;
        // The parent terminates this process. Returning an error here would
        // invoke automatic rollback and would not test crash recovery.
        loop {
            std::thread::park_timeout(Duration::from_secs(60));
        }
    }
    Ok(())
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = disk::remove_dir_all(&self.0);
    }
}

#[test]
#[ignore = "spawned only by the real process interruption acceptance test"]
fn crash_process_fixture() {
    let home = PathBuf::from(std::env::var_os(PROFILE_ENV).expect("isolated profile"));
    let nonce = std::env::var(NONCE_ENV).expect("fixture nonce");
    let index: usize = std::env::var(INDEX_ENV)
        .expect("rename index")
        .parse()
        .unwrap();
    assert!(index < 2);
    let home = crate::profile::ordinary_path(disk::canonicalize(home).unwrap());
    let temp = crate::profile::ordinary_path(disk::canonicalize(std::env::temp_dir()).unwrap());
    assert_eq!(home.parent(), Some(temp.as_path()));
    assert_eq!(
        home.file_name().unwrap(),
        format!("{PREFIX}{nonce}").as_str()
    );
    assert_eq!(disk::read_to_string(home.join("nonce")).unwrap(), nonce);
    let _com = native::Com::new().unwrap();
    let store = Store::open(&home).unwrap();
    assert_eq!(
        norm_literal_path(&store.get::<String>("test_desktop").unwrap()),
        norm_literal_path(&home.join("Desktop").to_string_lossy())
    );
    assert!(store.get::<bool>("background_disabled").unwrap());
    let plan = analyze(
        &store,
        Options::default(),
        &store.get::<Config>("config").unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(plan.summary(), (2, 1, 1));
    PAUSE.with(|pause| *pause.borrow_mut() = Some((index, home.join("renamed"))));
    apply(&store, &plan, &AtomicBool::new(false)).unwrap();
    panic!("the real rename checkpoint was not reached");
}

fn add_site(store: &Store, name: &str) -> App {
    let path = store.home.join("Desktop").join(format!("{name}.url"));
    disk::write(
        &path,
        format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
    )
    .unwrap();
    let mut app = native::read_link(&path).unwrap();
    app.note = format!("Preserve the saved choice for {name}");
    store.put_app(&app).unwrap();
    app
}

fn run_interruption(index: usize) {
    let nonce = fs::operation_id().unwrap();
    let home = std::env::temp_dir().join(format!("{PREFIX}{nonce}"));
    disk::create_dir_all(home.join("Desktop")).unwrap();
    let home = crate::profile::ordinary_path(disk::canonicalize(home).unwrap());
    let _cleanup = Cleanup(home.clone());
    disk::write(home.join("nonce"), &nonce).unwrap();
    let _com = native::Com::new().unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &home.join("Desktop").to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("background_disabled", &true).unwrap();
    let config = Config {
        collect: false,
        autostart: false,
        learning_enabled: false,
        respect_taskbar_pins: false,
        ..Default::default()
    };
    store.set("config", &config).unwrap();
    let alpha = add_site(&store, "Alpha");
    let beta = add_site(&store, "Beta");
    let rare = add_site(&store, "RareImportant");
    store.policy(&rare.id, Policy::Always).unwrap();
    store.observe(&alpha.id, 321, true, false).unwrap();
    let usage = serde_json::to_value(store.days().unwrap()).unwrap();
    let originals: Vec<_> = [&alpha, &beta, &rare]
        .iter()
        .map(|app| {
            (
                app.source.clone(),
                fs::inspect(Path::new(&app.source), true).unwrap(),
                disk::read(&app.source).unwrap(),
            )
        })
        .collect();
    drop(store);

    let stdout = disk::File::create(home.join("child-stdout.txt")).unwrap();
    let stderr = disk::File::create(home.join("child-stderr.txt")).unwrap();
    let mut child = ChildGuard(
        Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", CHILD_TEST, "--nocapture"])
            .env(PROFILE_ENV, &home)
            .env(NONCE_ENV, nonce)
            .env(INDEX_ENV, index.to_string())
            .stdin(Stdio::null())
            .stdout(stdout)
            .stderr(stderr)
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(30);
    while !home.join("renamed").exists() {
        let exit = child.0.try_wait().unwrap();
        assert!(
            exit.is_none() && Instant::now() < deadline,
            "child did not reach the real rename boundary: {exit:?}\n{}\n{}",
            disk::read_to_string(home.join("child-stdout.txt")).unwrap(),
            disk::read_to_string(home.join("child-stderr.txt")).unwrap()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
    drop(child);

    // Reopen the actual WAL-backed profile after abrupt termination, without
    // using the in-process error/rollback path or constructing a fake journal.
    let store = Store::open(&home).unwrap();
    let interrupted = status(&store).unwrap();
    assert!(interrupted.pending);
    let id = interrupted.undo.unwrap();
    assert_eq!(load(&store, &id).unwrap().phase, Phase::Prepared);
    assert!(ensure_ready(&store).is_err());
    assert!(store.policy(&alpha.id, Policy::Never).is_err());
    assert_eq!(store.app(&alpha.id).unwrap().policy, Policy::Auto);
    assert_eq!(store.app(&rare.id).unwrap().policy, Policy::Always);
    assert_eq!(disk::read(&rare.source).unwrap(), originals[2].2);
    let result = undo(&store, &id).unwrap();
    assert_eq!(result.restored, index + 1);
    assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    assert!(!status(&store).unwrap().pending);
    assert!(status(&store).unwrap().undo.is_none());
    assert_eq!(load(&store, &id).unwrap().phase, Phase::Undone);
    for (path, stamp, bytes) in &originals {
        assert_eq!(fs::inspect(Path::new(path), true).unwrap(), *stamp);
        assert_eq!(disk::read(path).unwrap(), *bytes);
    }
    assert_eq!(serde_json::to_value(store.days().unwrap()).unwrap(), usage);
    assert_eq!(store.app(&alpha.id).unwrap().source, alpha.source);
    assert_eq!(store.app(&alpha.id).unwrap().note, alpha.note);
    assert_eq!(store.app(&beta.id).unwrap().source, beta.source);
    assert_eq!(store.app(&rare.id).unwrap().policy, Policy::Always);
    assert!(store.get::<bool>("background_disabled").unwrap());
    assert_eq!(
        serde_json::to_value(store.get::<Config>("config").unwrap()).unwrap(),
        serde_json::to_value(config).unwrap()
    );
    assert!(undo(&store, &id).is_err());

    // Recovery must leave the feature usable, not just remove a pending flag.
    let plan = analyze(
        &store,
        Options::default(),
        &store.get::<Config>("config").unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(
        apply(&store, &plan, &AtomicBool::new(false)).unwrap().moved,
        2
    );
    let next = status(&store).unwrap().undo.unwrap();
    assert_eq!(undo(&store, &next).unwrap().restored, 2);
    for (path, stamp, bytes) in &originals {
        assert_eq!(fs::inspect(Path::new(path), true).unwrap(), *stamp);
        assert_eq!(disk::read(path).unwrap(), *bytes);
    }
}

#[test]
fn killed_process_after_first_or_last_rename_recovers_originals_and_manual_choices() {
    for index in [0, 1] {
        run_interruption(index);
    }
}
