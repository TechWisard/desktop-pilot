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
        .query_row("SELECT COUNT(*) FROM learning_write_audit", [], |row| row.get(0))
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
        assert_eq!(write_count(store), 0, "idle reviews must not touch the writer");
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
        assert_eq!(write_count(store), 1, "completed progress must not be rewritten");
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
