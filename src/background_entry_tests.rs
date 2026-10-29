use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

fn test_home() -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "test-desktop-pilot-background-entry-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&home).unwrap();
    home
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
    std::fs::remove_dir_all(home).unwrap();
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
    std::fs::remove_dir_all(home).unwrap();
}
