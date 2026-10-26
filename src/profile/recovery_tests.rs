use super::select;
use crate::{
    model::{App, Config, Owned, Point, Policy, State, now},
    store::Store,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "desktop-pilot-profile-recovery-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn locked_custom_profile_preserves_selection_and_user_choices_after_recovery() {
    let fixture = Fixture::new();
    let custom = fixture.0.join("my-portable-profile");
    let local = fixture.0.join("fallback");
    let executable = custom.join("desktop-pilot.exe");
    let store = Store::open(&custom).unwrap();
    let config = Config {
        language: "ru".into(),
        slots: 6,
        max_changes: 2,
        ..Default::default()
    };
    store.set("config", &config).unwrap();
    for (id, policy) in [("rare-tool", Policy::Always), ("excluded", Policy::Never)] {
        store
            .put_app(&App {
                id: id.into(),
                name: id.into(),
                policy,
                valid: true,
                ..Default::default()
            })
            .unwrap();
    }
    store.observe("rare-tool", 42, true, false).unwrap();
    let state = State {
        initialized: true,
        owned: vec![Owned {
            app: "rare-tool".into(),
            path: custom.join("rare-tool.url").to_string_lossy().into_owned(),
            sha: "unchanged".into(),
            position: Point { x: 220, y: 330 },
            manual: true,
            since: now(),
            file_id: None,
        }],
        ..Default::default()
    };
    store.set("state", &state).unwrap();
    let expected_state = serde_json::to_value(&state).unwrap();
    let expected_apps = serde_json::to_value(store.apps().unwrap()).unwrap();
    let expected_days = serde_json::to_value(store.days().unwrap()).unwrap();
    drop(store);

    // A real SQLite exclusive lock, not a mocked discovery result. The read-only
    // schema probe cannot identify the profile while another connection owns it.
    let locked = rusqlite::Connection::open(custom.join("usage.sqlite")).unwrap();
    locked
        .execute_batch("PRAGMA journal_mode=DELETE; BEGIN EXCLUSIVE;")
        .unwrap();
    let selected = select(None, &local, &executable);
    assert_eq!(selected, custom);
    assert!(
        !local.exists(),
        "Discovery must not create a replacement profile"
    );
    let explicit = fixture.0.join("explicit");
    assert_eq!(select(Some(&explicit), &local, &executable), explicit);
    locked.execute_batch("ROLLBACK;").unwrap();
    drop(locked);

    // The next real open resumes the same history, rules, settings and positions.
    let resumed = Store::open(&selected).unwrap();
    assert_eq!(resumed.get::<Config>("config").unwrap(), config);
    assert_eq!(
        serde_json::to_value(resumed.get::<State>("state").unwrap()).unwrap(),
        expected_state
    );
    assert_eq!(
        serde_json::to_value(resumed.apps().unwrap()).unwrap(),
        expected_apps
    );
    assert_eq!(
        serde_json::to_value(resumed.days().unwrap()).unwrap(),
        expected_days
    );
    assert_eq!(select(None, &local, &executable), custom);
    assert!(!local.exists());
}

#[test]
fn corrupt_adjacent_database_is_reported_without_switching_or_overwriting() {
    let fixture = Fixture::new();
    let custom = fixture.0.join("custom-install");
    let local = fixture.0.join("fallback");
    std::fs::create_dir_all(&custom).unwrap();
    let path = custom.join("usage.sqlite");
    let original = b"unreadable profile fixture: preserve these bytes";
    std::fs::write(&path, original).unwrap();
    let selected = select(None, &local, &custom.join("desktop-pilot.exe"));
    assert_eq!(selected, custom);
    assert!(Store::open(&selected).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(!local.exists());
}

#[test]
fn obstructed_adjacent_database_is_not_treated_as_an_absent_profile() {
    let fixture = Fixture::new();
    let custom = fixture.0.join("custom-install");
    let local = fixture.0.join("fallback");
    let obstruction = custom.join("usage.sqlite");
    std::fs::create_dir_all(&obstruction).unwrap();
    std::fs::write(obstruction.join("keep.txt"), b"keep").unwrap();
    let selected = select(None, &local, &custom.join("desktop-pilot.exe"));
    assert_eq!(selected, custom);
    assert!(Store::open(&selected).is_err());
    assert_eq!(
        std::fs::read(obstruction.join("keep.txt")).unwrap(),
        b"keep"
    );
    assert!(!local.exists());
}

#[cfg(windows)]
#[test]
fn sharing_violation_does_not_redirect_a_custom_profile() {
    use std::os::windows::fs::OpenOptionsExt;

    let fixture = Fixture::new();
    let custom = fixture.0.join("custom-install");
    let local = fixture.0.join("fallback");
    let store = Store::open(&custom).unwrap();
    store.set("recovery-marker", &"same profile").unwrap();
    drop(store);
    let locked = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(custom.join("usage.sqlite"))
        .unwrap();
    let selected = select(None, &local, &custom.join("desktop-pilot.exe"));
    assert_eq!(selected, custom);
    drop(locked);
    let resumed = Store::open(&selected).unwrap();
    assert_eq!(
        resumed.get::<String>("recovery-marker").unwrap(),
        "same profile"
    );
    assert!(!local.exists());
}
