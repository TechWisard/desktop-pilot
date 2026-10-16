use crate::model::*;
use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

pub struct Store {
    pub db: Connection,
    pub home: PathBuf,
}
impl Store {
    pub fn open(home: &Path) -> Result<Self> {
        std::fs::create_dir_all(home)?;
        let db = Connection::open(home.join("usage.sqlite"))?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; CREATE TABLE IF NOT EXISTS kv(k TEXT PRIMARY KEY,v TEXT NOT NULL); CREATE TABLE IF NOT EXISTS apps(id TEXT PRIMARY KEY,data TEXT NOT NULL); CREATE TABLE IF NOT EXISTS days(app TEXT,date INTEGER,sessions INTEGER DEFAULT 0,launches INTEGER DEFAULT 0,seconds INTEGER DEFAULT 0,PRIMARY KEY(app,date)); CREATE TABLE IF NOT EXISTS events(id INTEGER PRIMARY KEY AUTOINCREMENT,at INTEGER NOT NULL,kind TEXT NOT NULL,message TEXT NOT NULL); CREATE TABLE IF NOT EXISTS commands(id INTEGER PRIMARY KEY AUTOINCREMENT,action TEXT,status TEXT DEFAULT 'pending',message TEXT DEFAULT ''); CREATE TABLE IF NOT EXISTS transactions(id TEXT PRIMARY KEY,at INTEGER,status TEXT,data TEXT);")?;
        Ok(Self {
            db,
            home: home.into(),
        })
    }
    pub fn get<T: DeserializeOwned + Default>(&self, k: &str) -> Result<T> {
        let result = self.db.query_row("SELECT v FROM kv WHERE k=?1", [k], |r| {
            r.get::<_, String>(0)
        });
        match result {Ok(s)=>Ok(serde_json::from_str(&s).context(format!("Повреждены данные {k}; автоматическое восстановление значениями по умолчанию запрещено"))?),Err(rusqlite::Error::QueryReturnedNoRows)=>Ok(T::default()),Err(e)=>Err(e.into())}
    }
    pub fn set<T: Serialize>(&self, k: &str, v: &T) -> Result<()> {
        self.db.execute(
            "INSERT INTO kv VALUES(?1,?2) ON CONFLICT(k) DO UPDATE SET v=excluded.v",
            params![k, serde_json::to_string(v)?],
        )?;
        Ok(())
    }
    pub fn apps(&self) -> Result<Vec<App>> {
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
        let date = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_local_timezone(chrono::Local)
            .earliest()
            .context("Нет начала локального дня")?
            .timestamp();
        self.db.execute("INSERT INTO days VALUES(?1,?2,?3,?4,?5) ON CONFLICT(app,date) DO UPDATE SET sessions=sessions+excluded.sessions,launches=launches+excluded.launches,seconds=seconds+excluded.seconds",params![id,date,u32::from(session),u32::from(launch),seconds])?;
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
    pub fn finish(&self, id: i64, result: &Result<String>) -> Result<()> {
        let (status, message) = match result {
            Ok(s) => ("done", s.clone()),
            Err(e) => ("error", format!("{e:#}")),
        };
        self.db.execute(
            "UPDATE commands SET status=?1,message=?2 WHERE id=?3",
            params![status, message, id],
        )?;
        self.event(status, &message)
    }
    pub fn housekeeping(&self) -> Result<()> {
        self.db
            .execute("DELETE FROM days WHERE date<?1", [now() - 90 * 86400])?;
        self.db.execute("DELETE FROM events WHERE id NOT IN (SELECT id FROM events ORDER BY id DESC LIMIT 2000)",[])?;
        Ok(())
    }
}
