use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;

fn in_isolated_profile(test: impl FnOnce(&Store)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-contention-test-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&home).unwrap();
    test(&store);
    drop(store);
    std::fs::remove_dir_all(home).unwrap();
}

fn lock_other_writer(home: &Path, hold: Duration) -> thread::JoinHandle<()> {
    let database = home.join("usage.sqlite");
    let (locked_tx, locked_rx) = mpsc::sync_channel(0);
    let handle = thread::spawn(move || {
        let other = Connection::open(database).unwrap();
        other.busy_timeout(Duration::from_secs(5)).unwrap();
        other.execute_batch("BEGIN IMMEDIATE").unwrap();
        locked_tx.send(()).unwrap();
        thread::sleep(hold);
        other.execute_batch("COMMIT").unwrap();
    });
    locked_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    handle
}

#[test]
fn transient_writer_busy_at_begin_does_not_discard_a_user_change() {
    in_isolated_profile(|store| {
        store.set("value", &0u32).unwrap();
        let holder = lock_other_writer(&store.home, Duration::from_millis(220));
        // Force the same database-busy condition normally reached after five
        // seconds into a short, deterministic SQLite integration test.
        store.db.busy_timeout(Duration::from_millis(10)).unwrap();
        let attempt = store.transaction(|| store.set("value", &1u32));
        holder.join().unwrap();
        assert!(
            attempt.is_ok(),
            "short writer contention should be retried: {attempt:?}"
        );
        assert_eq!(store.get::<u32>("value").unwrap(), 1);
    });
}

#[test]
fn persistent_writer_busy_is_bounded_and_does_not_partially_write() {
    in_isolated_profile(|store| {
        store.set("value", &3u32).unwrap();
        let holder = lock_other_writer(&store.home, Duration::from_millis(650));
        store.db.busy_timeout(Duration::from_millis(10)).unwrap();
        let attempt = store.transaction(|| {
            store.set("value", &99u32)?;
            store.set("second", &true)
        });
        assert!(attempt.is_err(), "persistent contention must not be hidden");
        holder.join().unwrap();
        assert_eq!(store.get::<u32>("value").unwrap(), 3);
        assert!(!store.get::<bool>("second").unwrap());
        store.transaction(|| store.set("value", &4u32)).unwrap();
        assert_eq!(store.get::<u32>("value").unwrap(), 4);
    });
}
