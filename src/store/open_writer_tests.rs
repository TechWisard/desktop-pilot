use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// An existing WAL profile is a readable snapshot while another process is
/// saving a change. Opening Settings must not demand a second writer lock.
#[test]
fn a_verified_profile_opens_while_another_connection_holds_the_writer() {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-profile-open-with-writer-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed),
    ));
    let holder = Store::open(&home).unwrap();
    holder.set("config", &Config::default()).unwrap();
    holder.db.execute_batch("BEGIN IMMEDIATE").unwrap();
    holder.db.execute(
        "INSERT INTO events(at,kind,message) VALUES(0,'fixture','not committed')",
        [],
    ).unwrap();

    // This is the real Store::open path, not a mock of the database. The held
    // writer is intentionally not released until after the second open.
    let opened = Store::open(&home)
        .expect("An existing WAL profile must reopen while another writer is active");
    assert_eq!(opened.get::<Config>("config").unwrap(), Config::default());
    let visible: i64 = opened.db.query_row(
        "SELECT COUNT(*) FROM events",
        [],
        |row| row.get(0),
    ).unwrap();
    assert_eq!(visible, 0, "Uncommitted writer data must remain invisible");

    holder.db.execute_batch("ROLLBACK").unwrap();
    drop(opened);
    drop(holder);
    std::fs::remove_dir_all(home).unwrap();
}
