use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const DAY: i64 = 86_400;

fn with_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-retention-test-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn populate(store: &Store, at: i64, age_days: i64) {
    let observed_at = at - age_days * DAY;
    store
        .db
        .execute(
            "INSERT INTO days(app,date,sessions,launches,seconds) VALUES(?1,?2,1,1,1200)",
            params!["long-running-project", observed_at],
        )
        .unwrap();
    let sessions = std::collections::HashMap::from([(
        "long-running-project".to_owned(),
        observed_at,
    )]);
    store.set("app_session_times", &sessions).unwrap();
}

fn activity_count(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT COUNT(*) FROM days", [], |row| row.get(0))
        .unwrap()
}

fn session_count(store: &Store) -> usize {
    store
        .get::<std::collections::HashMap<String, i64>>("app_session_times")
        .unwrap()
        .len()
}

#[test]
fn forward_clock_jump_must_not_erase_recent_project_history_at_first_housekeeping() {
    with_profile(|store| {
        let at = now();
        populate(store, at, 30);
        store.housekeeping_at(at).unwrap();
        assert_eq!(activity_count(store), 1);
        assert_eq!(session_count(store), 1);

        // A system-time correction is not four months of confirmed inactivity.
        // The old immediate cutoff erased both counters and the recent checkpoint.
        store.housekeeping_at(at + 120 * DAY).unwrap();
        assert_eq!(activity_count(store), 1);
        assert_eq!(session_count(store), 1);

        // A genuine, stable prolonged absence is allowed to expire bounded usage
        // only after the confirmation window; no permanent retention is intended.
        store.housekeeping_at(at + 123 * DAY).unwrap();
        assert_eq!(activity_count(store), 1);
        assert_eq!(session_count(store), 1);
        store.housekeeping_at(at + 127 * DAY).unwrap();
        assert_eq!(activity_count(store), 0);
        assert_eq!(session_count(store), 0);
    });
}

#[test]
fn corrected_forward_clock_restores_normal_cleanup_without_touching_user_rules() {
    with_profile(|store| {
        let at = now();
        populate(store, at, 30);
        let app = App {
            id: "rare".to_owned(),
            name: "Rare manually chosen utility".to_owned(),
            policy: Policy::Always,
            valid: true,
            note: "Do not remove".to_owned(),
            ..Default::default()
        };
        store.put_app(&app).unwrap();
        let config = Config::default();
        store.set("config", &config).unwrap();

        store.housekeeping_at(at).unwrap();
        store.housekeeping_at(at + 120 * DAY).unwrap();
        assert_eq!(activity_count(store), 1);
        assert_eq!(session_count(store), 1);

        // Clock returns to the real date shortly after the erroneous jump.
        store.housekeeping_at(at + 3600).unwrap();
        assert_eq!(activity_count(store), 1);
        assert_eq!(session_count(store), 1);

        assert_eq!(store.app(&app.id).unwrap().policy, Policy::Always);
        assert_eq!(store.app(&app.id).unwrap().note, app.note);
        assert_eq!(store.get::<Config>("config").unwrap(), config);
    });
}

#[test]
fn ordinary_passage_of_two_days_still_expires_old_usage() {
    with_profile(|store| {
        let at = now();
        populate(store, at, 89);
        store.housekeeping_at(at).unwrap();
        assert_eq!(activity_count(store), 1);
        store.housekeeping_at(at + 2 * DAY).unwrap();
        assert_eq!(activity_count(store), 0);
        assert_eq!(session_count(store), 0);
    });
}
