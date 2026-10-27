use crate::model::*;
use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Store {
    pub db: Connection,
    pub home: PathBuf,
}

// Test-only, thread-local query count: measures actual Store::apps calls
// during one Settings render without adding production instrumentation.
#[cfg(test)]
thread_local! {
    pub(crate) static CATALOG_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

// Do not confuse a large wall-clock correction with verified inactivity.
// The seven-day observation window is a conservative policy, not a timing
// claim about users; a normal seven-day absence only delays cleanup.
const RETENTION_FORWARD_GAP_SECONDS: i64 = 7 * 86_400;
const RETENTION_HOLD_SECONDS: i64 = 7 * 86_400;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct RetentionClock {
    last_observed: i64,
    hold_until: i64,
}
struct NestedTransaction<'a> {
    db: &'a Connection,
    name: String,
    committed: bool,
}
impl Drop for NestedTransaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.db.execute_batch(&format!(
                "ROLLBACK TO {}; RELEASE {};",
                self.name, self.name
            ));
        }
    }
}

fn verify_profile_schema(db: &Connection) -> Result<()> {
    let objects: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE name NOT GLOB 'sqlite_*'",
        [],
        |row| row.get(0),
    )?;
    if objects == 0 {
        // A failed first creation can leave a valid SQLite header after its
        // schema transaction rolled back. It contains no user tables or data.
        let application_id: i64 =
            db.pragma_query_value(None, "application_id", |row| row.get(0))?;
        let user_version: i64 = db.pragma_query_value(None, "user_version", |row| row.get(0))?;
        ensure!(
            application_id == 0 && user_version == 0,
            "This folder contains a database that is not a Desktop Pilot profile; it was not changed"
        );
        return Ok(());
    }
    for (table, required) in [
        ("kv", &["k", "v"][..]),
        ("apps", &["id", "data"][..]),
        (
            "days",
            &["app", "date", "sessions", "launches", "seconds"][..],
        ),
        ("events", &["id", "at", "kind", "message"][..]),
        ("commands", &["id", "action", "status", "message"][..]),
        ("transactions", &["id", "at", "status", "data"][..]),
    ] {
        let is_table: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get(0),
        )?;
        let columns: Vec<String> = db
            .prepare(&format!("PRAGMA table_info({table})"))?
            .query_map([], |row| row.get(1))?
            .collect::<rusqlite::Result<_>>()?;
        ensure!(
            is_table
                && required
                    .iter()
                    .all(|name| columns.iter().any(|column| column == name)),
            "This folder contains a database that is not a Desktop Pilot profile; it was not changed"
        );
    }
    Ok(())
}

fn initialize_schema(db: &Connection) -> Result<()> {
    let transaction = Transaction::new_unchecked(db, TransactionBehavior::Immediate)?;
    // Verify again while holding the writer: another process could have created
    // the selected database between the read-only preflight and this connection.
    verify_profile_schema(db)?;
    db.execute_batch("CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY,v TEXT NOT NULL); CREATE TABLE IF NOT EXISTS apps(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS days(app TEXT,date INTEGER,sessions INTEGER DEFAULT 0,launches INTEGER DEFAULT 0,seconds INTEGER DEFAULT 0,PRIMARY KEY(app,date)); CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT,at INTEGER NOT NULL,kind TEXT NOT NULL,message TEXT NOT NULL); CREATE TABLE IF NOT EXISTS commands(id INTEGER PRIMARY KEY AUTOINCREMENT,action TEXT,status TEXT DEFAULT 'pending',message TEXT DEFAULT ''); CREATE TABLE IF NOT EXISTS transactions(id TEXT PRIMARY KEY,at INTEGER,status TEXT,data TEXT);")?;
    transaction.commit()?;
    Ok(())
}

fn begin_immediate_with_retry(db: &Connection) -> rusqlite::Result<Transaction<'_>> {
    const ATTEMPTS: usize = 3;
    for attempt in 0..ATTEMPTS {
        match Transaction::new_unchecked(db, TransactionBehavior::Immediate) {
            Ok(transaction) => return Ok(transaction),
            // Only retry acquiring the writer before any user closure runs.
            // Replaying arbitrary operations after a partial write is unsafe.
            Err(rusqlite::Error::SqliteFailure(error, _))
                if error.code == rusqlite::ErrorCode::DatabaseBusy && attempt + 1 < ATTEMPTS =>
            {
                std::thread::sleep(Duration::from_millis(125));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the last attempt always returns")
}

impl Store {
    pub fn open(home: &Path) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let path = home.join("usage.sqlite");
        if path.try_exists()? && std::fs::metadata(&path)?.len() > 0 {
            // Never initialize our tables inside an unrelated database selected by
            // an accidental --home path. Inspect without enabling WAL or writing.
            let existing = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            existing.busy_timeout(Duration::from_secs(5))?;
            verify_profile_schema(&existing)?;
        }
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        initialize_schema(&db)?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        Ok(Self {
            db,
            home: home.into(),
        })
    }
    pub fn get<T: DeserializeOwned + Default>(&self, k: &str) -> Result<T> {
        let result = self.db.query_row("SELECT v FROM kv WHERE k=?1", [k], |r| {
            r.get::<_, String>(0)
        });
        match result {
            Ok(s) => serde_json::from_str(&s).with_context(|| format!("Повреждены данные {k}; автоматическое восстановление значениями по умолчанию запрещено")),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(T::default()),
            Err(e) => Err(e.into()),
        }
    }
    /// Reserve the writer before reading a value that will be updated. This avoids
    /// lost updates and WAL read-to-write upgrade failures across UI/tray/worker.
    pub fn transaction<T>(&self, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        if !self.db.is_autocommit() {
            static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let name = format!(
                "desktop_pilot_{}",
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            self.db.execute_batch(&format!("SAVEPOINT {name}"))?;
            let mut savepoint = NestedTransaction {
                db: &self.db,
                name,
                committed: false,
            };
            let result = operation()?;
            self.db
                .execute_batch(&format!("RELEASE {}", savepoint.name))?;
            savepoint.committed = true;
            return Ok(result);
        }
        let tx = begin_immediate_with_retry(&self.db)?;
        let result = operation()?;
        tx.commit()?;
        Ok(result)
    }
    pub fn set<T: Serialize>(&self, k: &str, v: &T) -> Result<()> {
        self.db.execute(
            "INSERT INTO kv VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            params![k, serde_json::to_string(v)?],
        )?;
        Ok(())
    }
    pub fn apps(&self) -> Result<Vec<App>> {
        #[cfg(test)]
        CATALOG_READS.with(|reads| reads.set(reads.get().saturating_add(1)));
        let mut q = self.db.prepare("SELECT data FROM apps ORDER BY id")?;
        let rows = q.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn app(&self, id: &str) -> Result<App> {
        let s: String = self
            .db
            .query_row("SELECT data FROM apps WHERE id=?1", [id], |r| r.get(0))
            .context("Shortcut is missing from this Desktop Pilot profile. Open the original shortcut or refresh the list in Settings.")?;
        Ok(serde_json::from_str(&s)?)
    }
    pub fn put_app(&self, a: &App) -> Result<()> {
        ensure!(
            !a.id.is_empty() && !a.id.contains(['/', '\\', ':']),
            "Invalid shortcut identifier"
        );
        ensure!(
            a.seed.is_finite() && a.seed >= 0.0,
            "Invalid historical shortcut score"
        );
        self.db.execute(
            "INSERT INTO apps VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![a.id, serde_json::to_string(a)?],
        )?;
        Ok(())
    }
    pub fn policy(&self, id: &str, policy: Policy) -> Result<()> {
        let mut a = self.app(id)?;
        a.policy = policy;
        self.put_app(&a)
    }
    pub fn days(&self) -> Result<Vec<Day>> {
        let mut q = self
            .db
            .prepare("SELECT app,date,sessions,launches,seconds FROM days WHERE date>=?1")?;
        let rows = q.query_map([now() - 90 * 86400], |r| {
            Ok(Day {
                app: r.get(0)?,
                date: r.get(1)?,
                sessions: r.get(2)?,
                launches: r.get(3)?,
                seconds: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn observe(&self, id: &str, seconds: u32, session: bool, launch: bool) -> Result<()> {
        if seconds == 0 && !session && !launch {
            return Ok(());
        }
        let day = chrono::Local::now().date_naive();
        // Some timezone transitions skip midnight. Use the first existing hour
        // of that same date instead of terminating background collection.
        let date = (0..24)
            .find_map(|hour| {
                day.and_hms_opt(hour, 0, 0)?
                    .and_local_timezone(chrono::Local)
                    .earliest()
            })
            .context("Нет начала локального дня")?
            .timestamp();
        self.db.execute("INSERT INTO days(app,date,sessions,launches,seconds) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(app,date) DO UPDATE SET sessions=MIN(4294967295, sessions+excluded.sessions),launches=MIN(4294967295, launches+excluded.launches),seconds=MIN(86400, seconds+excluded.seconds)",params![id,date,u32::from(session),u32::from(launch),seconds.min(86400)])?;
        Ok(())
    }
    pub fn event(&self, kind: &str, msg: &str) -> Result<()> {
        self.db.execute(
            "INSERT INTO events(at,kind,message) VALUES(?1,?2,?3)",
            params![now(), kind, msg],
        )?;
        Ok(())
    }
    pub fn events(&self) -> Result<Vec<(i64, String, String)>> {
        let mut q = self
            .db
            .prepare("SELECT at,kind,message FROM events ORDER BY id DESC LIMIT 100")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn command(&self, action: &str) -> Result<()> {
        ensure!(
            matches!(
                action,
                "apply" | "rollback" | "accept_geometry" | "rescan" | "clear_stats"
            ) || action.strip_prefix("policy:").is_some_and(|rest| rest
                .split_once(':')
                .is_some_and(
                    |(id, policy)| !id.is_empty() && matches!(policy, "Auto" | "Always" | "Never")
                )),
            "Неизвестная команда"
        );
        self.db
            .execute("INSERT INTO commands(action) VALUES(?1)", [action])?;
        Ok(())
    }
    pub fn pending(&self) -> Result<Vec<(i64, String)>> {
        let mut q = self
            .db
            .prepare("SELECT id,action FROM commands WHERE status='pending' ORDER BY id")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn claim_command(&self, id: i64) -> Result<bool> {
        self.transaction(|| {
            if self.get::<bool>("shutdown")? {
                return Ok(false);
            }
            Ok(self.db.execute(
                "UPDATE commands SET status='running' WHERE id=?1 AND status='pending'",
                [id],
            )? == 1)
        })
    }
    pub fn recover_commands(&self) -> Result<()> {
        self.transaction(|| {
            let message = "Команда отменена после завершения предыдущего фонового процесса. Проверьте текущее состояние и при необходимости повторите действие.";
            let count = self.db.execute("UPDATE commands SET status='error',message=?1 WHERE status IN ('pending','running')", [message])?;
            if count > 0 {
                self.event("error", message)?;
            }
            Ok(())
        })
    }
    pub fn finish(&self, id: i64, result: &Result<String>) -> Result<()> {
        let (status, message) = match result {
            Ok(s) => ("done", s.clone()),
            Err(e) => ("error", format!("{e:#}")),
        };
        self.transaction(|| {
            self.db.execute(
                "UPDATE commands SET status=?1,message=?2 WHERE id=?3",
                params![status, message, id],
            )?;
            self.event(status, &message)
        })
    }
    pub fn housekeeping(&self) -> Result<()> {
        self.housekeeping_at(now())
    }
    /// Clock-injected retention is kept on the real Store/SQLite code path so
    /// wall-clock discontinuities can be reproduced without changing Windows time.
    fn housekeeping_at(&self, time: i64) -> Result<()> {
        self.transaction(|| {
            let mut clock: RetentionClock = self.get("retention_clock")?;
            if clock.last_observed > 0 {
                if time.saturating_sub(clock.last_observed) > RETENTION_FORWARD_GAP_SECONDS {
                    // After a large forward jump we cannot distinguish a genuinely
                    // long absence from an incorrect clock. Keep recent history
                    // through a bounded observation period across restarts.
                    clock.hold_until = time.saturating_add(RETENTION_HOLD_SECONDS);
                } else if clock.last_observed.saturating_sub(time) > 86_400 {
                    // The clock has been corrected back. An earlier future
                    // deadline must not suspend ordinary retention indefinitely.
                    clock.hold_until = 0;
                }
            } else {
                // Older profiles do not have a saved clock cursor yet. A large
                // gap since their newest activity may be a first-run clock error,
                // not proof that this history can be deleted.
                let latest_day: Option<i64> = self
                    .db
                    .query_row("SELECT MAX(date) FROM days", [], |row| row.get(0))?;
                if let Some(last) = latest_day
                    && time.saturating_sub(last) > RETENTION_FORWARD_GAP_SECONDS
                {
                    clock.hold_until = time.saturating_add(RETENTION_HOLD_SECONDS);
                }
            }
            clock.last_observed = time;
            self.set("retention_clock", &clock)?;

            if time >= clock.hold_until {
                let cutoff = time.saturating_sub(90 * 86_400);
                self.db.execute("DELETE FROM days WHERE date<?1", [cutoff])?;
                let mut sessions: std::collections::HashMap<String, i64> =
                    self.get("app_session_times")?;
                sessions.retain(|_, last| *last >= cutoff);
                self.set("app_session_times", &sessions)?;
            }
            // Count-bounded operational logs still need pruning during a clock
            // hold; otherwise an extended absence could grow the database.
            self.db.execute("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY id DESC LIMIT 2000)",[])?;
            self.db.execute("DELETE FROM commands WHERE status IN ('done','error') AND id NOT IN (SELECT id FROM commands ORDER BY id DESC LIMIT 2000)", [])?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn with_store(test: impl FnOnce(&Store)) {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-store-test-{}-{}-{}",
            std::process::id(),
            now(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let store = Store::open(&home).unwrap();
        test(&store);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn invalid_persisted_settings_are_reported_without_resetting_them() {
        with_store(|store| {
            store
                .db
                .execute("INSERT INTO kv VALUES('config','{broken')", [])
                .unwrap();
            assert!(store.get::<Config>("config").is_err());
            let bytes: String = store
                .db
                .query_row("SELECT v FROM kv WHERE k='config'", [], |row| row.get(0))
                .unwrap();
            assert_eq!(bytes, "{broken");
        });
    }
    #[test]
    fn opening_an_unrelated_database_does_not_change_its_bytes_or_schema() {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-foreign-db-test-{}-{}-{}",
            std::process::id(),
            now(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&home).unwrap();
        let path = home.join("usage.sqlite");
        let unrelated = Connection::open(&path).unwrap();
        unrelated.execute_batch("CREATE TABLE important_user_data(value TEXT); INSERT INTO important_user_data VALUES('preserve me')").unwrap();
        drop(unrelated);
        let before = std::fs::read(&path).unwrap();
        assert!(Store::open(&home).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(!home.join("usage.sqlite-wal").exists());
        let unrelated =
            Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        let value: String = unrelated
            .query_row("SELECT value FROM important_user_data", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, "preserve me");
        drop(unrelated);
        std::fs::remove_dir_all(home).unwrap();
    }
    #[test]
    fn failed_first_creation_rolls_back_the_schema_and_can_be_retried() {
        with_store(|store| {
            let home = store.home.join("first-creation");
            std::fs::create_dir_all(&home).unwrap();
            let db = Connection::open(home.join("usage.sqlite")).unwrap();
            // Two tables fit, but the rest of the real schema exhausts the
            // artificial disk allowance. This fails during initialization.
            db.pragma_update(None, "max_page_count", 5).unwrap();
            assert!(initialize_schema(&db).is_err());
            let tables: i64 = db
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(tables, 0);
            db.pragma_update(None, "max_page_count", 1000).unwrap();
            drop(db);
            let reopened = Store::open(&home).unwrap();
            reopened.set("config", &Config::default()).unwrap();
            assert_eq!(reopened.get::<Config>("config").unwrap(), Config::default());
        });
    }
    #[test]
    fn concurrent_first_open_preserves_a_complete_profile() {
        with_store(|store| {
            let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
            let home = store.home.join("concurrent-creation");
            let workers: Vec<_> = (0..4)
                .map(|index| {
                    let home = home.clone();
                    let barrier = barrier.clone();
                    std::thread::spawn(move || {
                        barrier.wait();
                        let store = Store::open(&home).unwrap();
                        store.set(&format!("writer-{index}"), &index).unwrap();
                    })
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
            let reopened = Store::open(&home).unwrap();
            for index in 0..4 {
                assert_eq!(
                    reopened.get::<i32>(&format!("writer-{index}")).unwrap(),
                    index
                );
            }
        });
    }
    #[test]
    fn multi_record_failure_rolls_back_all_database_changes() {
        with_store(|store| {
            store.set("counter", &1u32).unwrap();
            let result: Result<()> = store.transaction(|| {
                store.set("counter", &2u32)?;
                store.event("test", "Must not persist")?;
                anyhow::bail!("injected failure")
            });
            assert!(result.is_err());
            assert_eq!(store.get::<u32>("counter").unwrap(), 1);
            assert!(store.events().unwrap().is_empty());
        });
    }
    #[test]
    fn nested_transaction_failure_keeps_outer_changes_only() {
        with_store(|store| {
            store
                .transaction(|| {
                    store.set("outer", &1u32)?;
                    let failed: Result<()> = store.transaction(|| {
                        store.set("outer", &2u32)?;
                        store.set("inner", &3u32)?;
                        anyhow::bail!("inner failure")
                    });
                    assert!(failed.is_err());
                    assert_eq!(store.get::<u32>("outer")?, 1);
                    assert_eq!(store.get::<u32>("inner")?, 0);
                    store.transaction(|| store.set("successful_inner", &4u32))?;
                    Ok(())
                })
                .unwrap();
            assert_eq!(store.get::<u32>("outer").unwrap(), 1);
            assert_eq!(store.get::<u32>("successful_inner").unwrap(), 4);
            let failed: Result<()> = store.transaction(|| {
                store.transaction(|| store.set("outer", &10u32))?;
                anyhow::bail!("outer failure")
            });
            assert!(failed.is_err());
            assert_eq!(store.get::<u32>("outer").unwrap(), 1);
        });
    }
    #[test]
    fn interrupted_commands_are_not_replayed_on_restart() {
        with_store(|store| {
            store.command("rollback").unwrap();
            let first = store.db.last_insert_rowid();
            store.command("apply").unwrap();
            let second = store.db.last_insert_rowid();
            assert!(store.claim_command(first).unwrap());
            assert!(!store.claim_command(first).unwrap());
            store.recover_commands().unwrap();
            assert!(store.pending().unwrap().is_empty());
            for id in [first, second] {
                let status: String = store
                    .db
                    .query_row("SELECT status FROM commands WHERE id=?1", [id], |row| {
                        row.get(0)
                    })
                    .unwrap();
                assert_eq!(status, "error");
            }
            store.command("rescan").unwrap();
            let current = store.db.last_insert_rowid();
            assert!(store.claim_command(current).unwrap());
        });
    }
    #[test]
    fn shutdown_prevents_the_next_command_from_starting() {
        with_store(|store| {
            store.command("apply").unwrap();
            let id = store.db.last_insert_rowid();
            store.set("shutdown", &true).unwrap();
            assert!(!store.claim_command(id).unwrap());
            assert_eq!(store.pending().unwrap(), vec![(id, "apply".into())]);
            store.recover_commands().unwrap();
            assert!(store.pending().unwrap().is_empty());
        });
    }
    #[test]
    fn unsupported_commands_do_not_pollute_the_queue() {
        with_store(|store| {
            assert!(store.command("anything").is_err());
            assert!(store.command("policy:app:unknown").is_err());
            assert!(store.command("policy::Always").is_err());
            assert!(store.pending().unwrap().is_empty());
        });
    }
    #[test]
    fn daily_usage_is_bounded_and_empty_samples_do_not_create_days() {
        with_store(|store| {
            store.observe("app", 0, false, false).unwrap();
            assert!(store.days().unwrap().is_empty());
            store.observe("app", u32::MAX, true, true).unwrap();
            store.observe("app", u32::MAX, false, true).unwrap();
            let days = store.days().unwrap();
            assert_eq!(days.len(), 1);
            assert_eq!(days[0].seconds, 86400);
            assert_eq!(days[0].sessions, 1);
            assert_eq!(days[0].launches, 2);
        });
    }
    #[test]
    fn old_session_timestamps_are_removed_by_regular_retention() {
        with_store(|store| {
            let sessions = std::collections::HashMap::from([
                ("old", now() - 91 * 86400),
                ("recent", now() - 3600),
            ]);
            store.set("app_session_times", &sessions).unwrap();
            store.housekeeping().unwrap();
            let retained: std::collections::HashMap<String, i64> =
                store.get("app_session_times").unwrap();
            assert_eq!(retained.len(), 1);
            assert!(retained.contains_key("recent"));
        });
    }
    #[test]
    fn immediate_transactions_keep_concurrent_updates() {
        with_store(|store| {
            store.set("counter", &0u32).unwrap();
            let workers: Vec<_> = (0..2)
                .map(|_| {
                    let home = store.home.clone();
                    std::thread::spawn(move || {
                        let store = Store::open(&home).unwrap();
                        for _ in 0..20 {
                            store
                                .transaction(|| {
                                    store.set("counter", &(store.get::<u32>("counter")? + 1))
                                })
                                .unwrap();
                        }
                    })
                })
                .collect();
            for worker in workers {
                worker.join().unwrap();
            }
            assert_eq!(store.get::<u32>("counter").unwrap(), 40);
        });
    }
}

#[cfg(test)]
mod retention_tests;

#[cfg(test)]
mod contention_tests;

#[cfg(test)]
mod catalog_rescan_tests;

#[cfg(test)]
mod open_writer_tests;
