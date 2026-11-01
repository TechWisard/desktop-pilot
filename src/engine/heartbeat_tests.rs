use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn in_temporary_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-heartbeat-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn sql_writes(store: &Store) -> i64 {
    store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap()
}

#[test]
fn idle_hour_uses_bounded_real_sqlite_heartbeat_updates() {
    in_temporary_profile(|store| {
        let start = Instant::now();
        let mut last = None;
        let before = sql_writes(store);
        let mut writes = 0;
        // Reproduce the monitor's two-second tick cadence for sixty minutes
        // without sleeping or inventing a simplified persistence layer.
        for tick in 0..1800 {
            let at = start + Duration::from_secs(tick * 2);
            if publish_heartbeat_if_due(store, &mut last, at, false).unwrap() {
                writes += 1;
            }
        }
        assert_eq!(writes, 225);
        assert_eq!(sql_writes(store) - before, writes);
        assert!(store.get::<i64>("heartbeat").unwrap() > 0);
        // Every observation interval stays far inside Settings' 150-second
        // unresponsive threshold; the first tick is always published.
        assert!(HEARTBEAT_INTERVAL < Duration::from_secs(150));
    });
}

#[test]
fn startup_long_absence_and_explicit_commands_never_lose_readiness() {
    in_temporary_profile(|store| {
        let start = Instant::now();
        let mut last = None;
        let before = sql_writes(store);
        assert!(publish_heartbeat_if_due(store, &mut last, start, false).unwrap());
        assert!(
            !publish_heartbeat_if_due(store, &mut last, start + Duration::from_secs(2), false)
                .unwrap()
        );
        // An explicit queued command forces a fresh signal without waiting.
        assert!(
            publish_heartbeat_if_due(store, &mut last, start + Duration::from_secs(3), true)
                .unwrap()
        );
        assert!(
            !publish_heartbeat_if_due(store, &mut last, start + Duration::from_secs(16), false)
                .unwrap()
        );
        assert!(
            publish_heartbeat_if_due(store, &mut last, start + Duration::from_secs(18), false)
                .unwrap()
        );
        // After a suspended clock or an extended absence, one write resumes
        // operation; no backlog of idle ticks is replayed into the WAL.
        assert!(
            publish_heartbeat_if_due(
                store,
                &mut last,
                start + Duration::from_secs(8 * 3600),
                false
            )
            .unwrap()
        );
        assert!(
            !publish_heartbeat_if_due(
                store,
                &mut last,
                start + Duration::from_secs(8 * 3600 + 2),
                false
            )
            .unwrap()
        );
        assert_eq!(sql_writes(store) - before, 4);
    });
}

#[test]
fn a_clock_correction_is_healed_by_the_actual_bounded_sqlite_writer() {
    in_temporary_profile(|store| {
        let observed = now();
        let future = observed + 30 * 86_400;
        store.set("heartbeat", &future).unwrap();
        let start = Instant::now();
        let mut last = Some(start);
        assert!(
            !publish_heartbeat_if_due(store, &mut last, start + Duration::from_secs(2), false)
                .unwrap()
        );
        assert_eq!(store.get::<i64>("heartbeat").unwrap(), future);
        assert!(
            publish_heartbeat_if_due(store, &mut last, start + HEARTBEAT_INTERVAL, false).unwrap()
        );
        assert!(store.get::<i64>("heartbeat").unwrap().abs_diff(now()) <= 2);
        assert!(
            !publish_heartbeat_if_due(
                store,
                &mut last,
                start + HEARTBEAT_INTERVAL + Duration::from_secs(2),
                false
            )
            .unwrap()
        );
    });
}

#[test]
fn temporary_writer_lock_drops_only_the_heartbeat_and_recovers_on_next_tick() {
    in_temporary_profile(|store| {
        let old = 17_i64;
        store.set("heartbeat", &old).unwrap();
        store.db.busy_timeout(Duration::from_millis(1)).unwrap();
        let blocker = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
        blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();

        let first_tick = Instant::now();
        let mut last = None;
        // The real periodic writer must not kill foreground observation when
        // a Settings/backup transaction briefly owns SQLite's only writer.
        assert!(!publish_heartbeat_if_due(store, &mut last, first_tick, false).unwrap());
        assert!(last.is_none(), "a refused write is not a published heartbeat");
        assert_eq!(store.get::<i64>("heartbeat").unwrap(), old);

        blocker.execute_batch("ROLLBACK;").unwrap();
        drop(blocker);
        let resumed = first_tick + Duration::from_secs(2);
        assert!(publish_heartbeat_if_due(store, &mut last, resumed, false).unwrap());
        assert_eq!(last, Some(resumed));
        assert!(store.get::<i64>("heartbeat").unwrap() > old);
        assert!(
            !publish_heartbeat_if_due(
                store,
                &mut last,
                resumed + Duration::from_secs(2),
                false,
            )
            .unwrap()
        );
    });
}

#[test]
fn non_lock_heartbeat_database_failure_is_not_hidden() {
    in_temporary_profile(|store| {
        // Damage only an isolated synthetic profile. Unlike BUSY/LOCKED,
        // a missing database table must remain an actionable error.
        store.db.execute_batch("DROP TABLE kv;").unwrap();
        let mut last = None;
        assert!(publish_heartbeat_if_due(store, &mut last, Instant::now(), false).is_err());
        assert!(last.is_none());
    });
}
