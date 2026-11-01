use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_store(check: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-idle-review-{}-{}",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let store = Store::open(&home).unwrap();
    check(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn write_count(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT COUNT(*) FROM learning_write_audit", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn audit_writes(store: &Store) {
    store
        .db
        .execute_batch(
            "CREATE TABLE learning_write_audit (checkpoint INTEGER NOT NULL);
             CREATE TRIGGER record_learning_update
             AFTER UPDATE ON kv WHEN NEW.k='learning'
             BEGIN INSERT INTO learning_write_audit VALUES(1); END;",
        )
        .unwrap();
}

#[test]
fn idle_review_does_not_rewrite_unchanged_learning_progress_across_modes() {
    with_store(|store| {
        let config = Config {
            collect: true,
            automatic: false,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        let progress = Progress {
            sessions: 2,
            started_at: now().saturating_sub(7200),
            ..Progress::default()
        };
        store.set("learning", &progress).unwrap();
        audit_writes(store);

        for _ in 0..5 {
            assert!(!automatic_review(store, false).unwrap());
        }
        let manual = Config {
            collect: false,
            automatic: false,
            learning_enabled: false,
            ..config.clone()
        };
        store.set("config", &manual).unwrap();
        for _ in 0..5 {
            assert!(!automatic_review(store, false).unwrap());
        }
        assert_eq!(
            write_count(store),
            0,
            "idle reviews must not touch the writer"
        );
        let saved: Progress = store.get("learning").unwrap();
        assert_eq!(saved.sessions, progress.sessions);
        assert_eq!(saved.completed_at, 0);
        assert_eq!(saved.started_at, progress.started_at);

        // A manual profile also must not be made unresponsive by a writer
        // held for an unrelated transaction. Exercise the actual SQLite WAL.
        let another = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
        another.execute_batch("BEGIN IMMEDIATE;").unwrap();
        assert!(!automatic_review(store, false).unwrap());
        another.execute_batch("ROLLBACK;").unwrap();
        assert_eq!(write_count(store), 0);
    });
}

#[test]
fn learning_completion_is_persisted_once_and_survives_repeated_reviews() {
    with_store(|store| {
        let config = Config {
            collect: true,
            automatic: false,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        let progress = Progress {
            sessions: config.learning_sessions,
            started_at: now().saturating_sub(7200),
            ..Progress::default()
        };
        store.set("learning", &progress).unwrap();
        audit_writes(store);
        assert!(!automatic_review(store, false).unwrap());
        let completed: Progress = store.get("learning").unwrap();
        assert!(completed.completed_at > 0);
        assert_eq!(write_count(store), 1, "first completion must be committed");

        for _ in 0..6 {
            assert!(!automatic_review(store, false).unwrap());
        }
        assert_eq!(
            write_count(store),
            1,
            "completed progress must not be rewritten"
        );
        assert_eq!(
            store.get::<Progress>("learning").unwrap().completed_at,
            completed.completed_at
        );
    });
}

#[test]
fn completed_auto_review_when_not_due_does_not_take_an_unneeded_writer() {
    with_store(|store| {
        let config = Config {
            collect: true,
            automatic: true,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        let progress = Progress {
            completed_at: now().saturating_sub(86_400),
            sessions: config.learning_sessions,
            ..Progress::default()
        };
        store.set("learning", &progress).unwrap();
        store
            .set(
                "state",
                &State {
                    initialized: true,
                    last_review: now(),
                    ..State::default()
                },
            )
            .unwrap();
        audit_writes(store);
        for _ in 0..3 {
            assert!(!automatic_review(store, false).unwrap());
        }
        assert_eq!(write_count(store), 0);
    });
}

#[test]
fn queued_review_does_not_move_shortcuts_after_the_user_resumes_work() {
    with_store(|store| {
        let desktop = store.home.join("Desktop");
        std::fs::create_dir_all(&desktop).unwrap();
        store.set("test_desktop", &desktop.to_string_lossy()).unwrap();

        let source = store.home.join("Rare research.url");
        let original = b"[InternetShortcut]\r\nURL=https://example.com/rare-research\r\n";
        std::fs::write(&source, original).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();

        let config = Config {
            automatic: true,
            collect: true,
            learning_enabled: false,
            slots: 1,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        let before = State {
            initialized: true,
            last_review: now() - 8 * 86400,
            ..State::default()
        };
        store.set("state", &before).unwrap();

        // Initial idle permission was true, but foreground activity resumes
        // while the review waits to acquire its final placement boundary.
        let user_active = std::cell::Cell::new(false);
        let attempts = std::cell::Cell::new(0u32);
        assert!(
            !automatic_review_with_quiet_recheck(
                store,
                true,
                || {
                    attempts.set(attempts.get() + 1);
                    user_active.set(true);
                },
                || !user_active.get(),
            )
            .unwrap()
        );
        assert_eq!(attempts.get(), 1);
        let unchanged: State = store.get("state").unwrap();
        assert_eq!(unchanged.last_review, before.last_review);
        assert!(unchanged.owned.is_empty());
        assert!(std::fs::read_dir(&desktop).unwrap().next().is_none());
        assert_eq!(std::fs::read(&source).unwrap(), original);

        // The queued review remains due. Only a fresh quiet confirmation can
        // use the same real selector, placement and SQLite transaction paths.
        assert!(
            automatic_review_with_quiet_recheck(store, true, || {}, || true).unwrap()
        );
        let applied: State = store.get("state").unwrap();
        assert_eq!(applied.owned.len(), 1);
        assert_eq!(applied.owned[0].app, app.id);
        assert!(std::path::Path::new(&applied.owned[0].path).exists());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(
            !automatic_review_with_quiet_recheck(
                store,
                true,
                || panic!("a review that is not due must not enter the final boundary"),
                || panic!("a review that is not due must not recheck quiet"),
            )
            .unwrap()
        );
    });
}
