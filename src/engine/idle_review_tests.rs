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
fn reset_during_idle_completion_preflight_must_not_restore_old_learning_progress() {
    with_store(|store| {
        let config = Config {
            collect: true,
            automatic: false,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        store
            .set(
                "learning",
                &Progress {
                    started_at: now().saturating_sub(7200),
                    sessions: config.learning_sessions,
                    ..Progress::default()
                },
            )
            .unwrap();
        audit_writes(store);

        // A second Settings connection resets learning after the review has
        // read its qualifying progress, but before the completion write.
        BEFORE_IDLE_LEARNING_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|store| {
                let settings = Store::open(&store.home).unwrap();
                settings.set("learning", &Progress::default()).unwrap();
            }));
        });
        assert!(!automatic_review(store, false).unwrap());
        let saved: Progress = store.get("learning").unwrap();
        assert_eq!(saved.completed_at, 0, "explicit reset must win");
        assert_eq!(saved.sessions, 0);
        assert_eq!(saved.started_at, 0);
        assert_eq!(
            write_count(store),
            1,
            "only the user's explicit reset may update learning"
        );
    });
}

#[test]
fn changed_learning_goal_during_idle_completion_preflight_must_be_rechecked() {
    with_store(|store| {
        let config = Config {
            collect: true,
            automatic: false,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        store
            .set(
                "learning",
                &Progress {
                    started_at: now().saturating_sub(7200),
                    sessions: config.learning_sessions,
                    ..Progress::default()
                },
            )
            .unwrap();
        audit_writes(store);
        BEFORE_IDLE_LEARNING_WRITE.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|store| {
                let settings = Store::open(&store.home).unwrap();
                let mut config: Config = settings.get("config").unwrap();
                config.learning_sessions += 1;
                settings.set("config", &config).unwrap();
            }));
        });
        assert!(!automatic_review(store, false).unwrap());
        let saved: Progress = store.get("learning").unwrap();
        assert_eq!(saved.completed_at, 0);
        assert_eq!(saved.sessions, config.learning_sessions);
        assert_eq!(write_count(store), 0);
        assert_eq!(
            store.get::<Config>("config").unwrap().learning_sessions,
            config.learning_sessions + 1
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
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();

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
        assert!(automatic_review_with_quiet_recheck(store, true, || {}, || true).unwrap());
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

#[test]
fn active_user_during_slow_preflight_defers_the_same_due_review_without_churn() {
    with_store(|store| {
        let desktop = store.home.join("Desktop");
        std::fs::create_dir_all(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();

        let source = store.home.join("Project report.url");
        let original = b"[InternetShortcut]\r\nURL=https://example.com/project-report\r\n";
        std::fs::write(&source, original).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        store
            .set(
                "config",
                &Config {
                    automatic: true,
                    collect: true,
                    learning_enabled: false,
                    slots: 1,
                    ..Config::default()
                },
            )
            .unwrap();
        let before = State {
            initialized: true,
            last_review: now() - 8 * 86400,
            ..State::default()
        };
        store.set("state", &before).unwrap();

        // Early quiet permission still passes. The user returns only after
        // the production catalog refresh, reconciliation and layout preflight.
        // Without the late check this due review would stage and publish a copy.
        let user_active = std::cell::Cell::new(false);
        let quiet_checks = std::cell::Cell::new(0usize);
        assert!(
            !automatic_review_with_precommit_boundary(
                store,
                true,
                || user_active.set(true),
                || {
                    quiet_checks.set(quiet_checks.get() + 1);
                    !user_active.get()
                },
            )
            .unwrap()
        );
        assert_eq!(quiet_checks.get(), 2, "check before and after preflight");
        let deferred: State = store.get("state").unwrap();
        assert_eq!(deferred.last_review, before.last_review);
        assert!(deferred.owned.is_empty());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(std::fs::read_dir(&desktop).unwrap().next().is_none());

        // After the activity ends the same review must still be due and
        // must use the normal real database, selector and copy/Undo journal.
        assert!(automatic_review_with_quiet_recheck(store, true, || {}, || true).unwrap());
        let applied: State = store.get("state").unwrap();
        assert_eq!(applied.owned.len(), 1);
        assert_eq!(applied.owned[0].app, app.id);
        assert!(std::path::Path::new(&applied.owned[0].path).exists());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(!automatic_review_with_quiet_recheck(store, true, || {}, || true).unwrap());
    });
}

#[test]
fn user_returning_during_copy_preparation_defers_review_without_orphaned_files() {
    with_store(|store| {
        let desktop = store.home.join("Desktop");
        std::fs::create_dir_all(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        let source = store.home.join("Rare session.url");
        let original = b"[InternetShortcut]\r\nURL=https://example.com/rare-session\r\n";
        std::fs::write(&source, original).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        store
            .set(
                "config",
                &Config {
                    automatic: true,
                    collect: true,
                    learning_enabled: false,
                    slots: 1,
                    ..Config::default()
                },
            )
            .unwrap();
        let before = State {
            initialized: true,
            last_review: now() - 8 * 86400,
            ..State::default()
        };
        store.set("state", &before).unwrap();

        // The first two production quiet checks pass. The third runs after
        // real staging and backup preparation but before the durable journal.
        // In the old implementation there was no third check: the quick
        // shortcut was published even though the user had returned.
        let checks = std::cell::Cell::new(0usize);
        let deferred = automatic_review_with_quiet_recheck(
            store,
            true,
            || {},
            || {
                checks.set(checks.get() + 1);
                checks.get() < 3
            },
        )
        .unwrap();
        assert!(!deferred);
        assert_eq!(checks.get(), 3);
        let state: State = store.get("state").unwrap();
        assert_eq!(state.last_review, before.last_review);
        assert!(state.owned.is_empty());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(std::fs::read_dir(&desktop).unwrap().next().is_none());
        let history = store.home.join("history");
        if history.exists() {
            assert!(std::fs::read_dir(history).unwrap().next().is_none());
        }
        let journals: i64 = store
            .db
            .query_row("SELECT COUNT(*) FROM transactions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journals, 0);

        // Once idle again, the same real selector, SQLite journal and
        // filesystem apply/Undo path can finish without losing user intent.
        assert!(automatic_review_with_quiet_recheck(store, true, || {}, || true).unwrap());
        let applied: State = store.get("state").unwrap();
        assert_eq!(applied.owned.len(), 1);
        assert_eq!(applied.owned[0].app, app.id);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        rollback(store).unwrap();
        assert!(store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert!(std::fs::read_dir(&desktop).unwrap().next().is_none());
    });
}
