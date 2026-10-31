//! Explicit, reversible desktop organization. Never scheduled automatically.
mod fs;
mod journal;
mod metadata;
mod plan;
#[cfg(test)]
mod tests;

pub use journal::{accept_current, apply, undo};
pub use plan::{analyze, find_free_zone};

use crate::{
    model::{Config, Point, Policy, State, norm},
    native,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const ACTIVE: &str = "organization.active.v1";
const HISTORY: &str = "organization.history.v1";
const FOLDERS: &str = "organization.folders.v1";
const KEEP: &str = "organization.keep.v1";
const LIMIT: usize = 6000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Games,
    Programs,
    Sites,
    Documents,
    Images,
    Media,
    Archives,
}
impl Group {
    pub fn label(self, russian: bool) -> &'static str {
        match (self, russian) {
            (Self::Games, true) => "Игры",
            (Self::Games, false) => "Games",
            (Self::Programs, true) => "Программы",
            (Self::Programs, false) => "Programs",
            (Self::Sites, true) => "Сайты",
            (Self::Sites, false) => "Sites",
            (Self::Documents, true) => "Документы",
            (Self::Documents, false) => "Documents",
            (Self::Images, true) => "Изображения",
            (Self::Images, false) => "Images",
            (Self::Media, true) => "Медиа",
            (Self::Media, false) => "Media",
            (Self::Archives, true) => "Архивы",
            (Self::Archives, false) => "Archives",
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Options {
    pub include_files: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Item {
    pub name: String,
    pub group: Group,
    pub selected: bool,
    pub position: Option<Point>,
    stamp: fs::Stamp,
    launch: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct Folder {
    group: Group,
    name: String,
    id: Option<String>,
    created: bool,
    position: Option<Point>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Plan {
    pub items: Vec<Item>,
    pub skipped: Vec<Skipped>,
    pub options: Options,
    pub russian: bool,
    root: String,
    root_id: String,
    folders: Vec<Folder>,
    layout: Option<native::Layout>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Movement {
    item: Item,
    folder: String,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
enum Phase {
    Prepared,
    Committed,
    Undoing,
    Undone,
    Accepted,
}

#[derive(Clone, Serialize, Deserialize)]
struct Journal {
    id: String,
    root: String,
    root_id: String,
    phase: Phase,
    movements: Vec<Movement>,
    folders: Vec<Folder>,
    layout: Option<native::Layout>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Status {
    pub pending: bool,
    pub undo: Option<String>,
}

#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct Outcome {
    pub moved: usize,
    pub restored: usize,
    pub folders_created: usize,
    pub warnings: Vec<String>,
}

fn root(store: &Store) -> Result<PathBuf> {
    let custom: String = store.get("test_desktop")?;
    let root = if custom.is_empty() {
        native::desktop()?
    } else {
        custom.into()
    };
    let root = crate::profile::ordinary_path(std::fs::canonicalize(root)?);
    let text = root.to_str().context("The desktop path cannot be represented losslessly")?;
    ensure!(
        !norm(text).starts_with(r"\\"),
        "Network desktops are not moved automatically. Use a local desktop."
    );
    Ok(root)
}

fn layout(store: &Store) -> Result<Option<native::Layout>> {
    if store.get::<String>("test_desktop")?.is_empty() {
        Ok(Some(native::layout()?))
    } else {
        Ok(None)
    }
}

fn key(id: &str) -> Result<String> {
    ensure!(
        !id.is_empty() && id.bytes().all(|c| c.is_ascii_digit() || c == b'-'),
        "Invalid organization operation identifier"
    );
    Ok(format!("organization.operation.{id}"))
}

fn load(store: &Store, id: &str) -> Result<Journal> {
    store
        .get::<Option<Journal>>(&key(id)?)?
        .context("Organization journal is missing")
}

fn save(store: &Store, journal: &Journal) -> Result<()> {
    store.set(&key(&journal.id)?, journal)
}

pub fn status(store: &Store) -> Result<Status> {
    let active: Option<String> = store.get(ACTIVE)?;
    if let Some(id) = active {
        return Ok(Status {
            pending: true,
            undo: Some(id),
        });
    }
    let history: Vec<String> = store.get(HISTORY)?;
    for id in history.iter().rev() {
        if load(store, id)?.phase == Phase::Committed {
            return Ok(Status {
                pending: false,
                undo: Some(id.clone()),
            });
        }
    }
    Ok(Status::default())
}

pub fn ensure_ready(store: &Store) -> Result<()> {
    ensure!(
        store.get::<Option<String>>(ACTIVE)?.is_none(),
        "Desktop organization was interrupted. Open Organize desktop to recover it first."
    );
    Ok(())
}

fn no_placement_in_progress(store: &Store) -> Result<()> {
    let pending: bool = store.db.query_row(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE status IN ('prepared','rolling_back'))",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        !pending,
        "Finish or undo interrupted shortcut placement before organizing the desktop"
    );
    Ok(())
}

pub fn clear_exclusions(store: &Store) -> Result<()> {
    let _changes = native::exclusive("changes", &store.home)?;
    ensure_ready(store)?;
    store.set(KEEP, &Vec::<String>::new())
}

// Durable catalog barriers protect scanner/import/rule writers across a crash.
// Removing them and changing source paths is one SQLite transaction, so a
// failed commit restores both the prior catalog and its barriers.
fn install_catalog_barrier(store: &Store) -> Result<()> {
    for event in ["INSERT", "UPDATE", "DELETE"] {
        store.db.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS desktop_pilot_organization_guard_{event}_v1 BEFORE {event} ON apps BEGIN SELECT RAISE(ABORT, 'Desktop organization needs recovery. Open Organize desktop.'); END;"
        ))?;
    }
    Ok(())
}

fn remove_catalog_barrier(store: &Store) -> Result<()> {
    for event in ["INSERT", "UPDATE", "DELETE"] {
        store.db.execute_batch(&format!(
            "DROP TRIGGER IF EXISTS desktop_pilot_organization_guard_{event}_v1;"
        ))?;
    }
    Ok(())
}
