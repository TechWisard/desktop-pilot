use crate::{learning::Progress, model::*, native, store::Store};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::params;
use serde::{Deserialize, Serialize};
#[cfg(windows)]
use std::os::windows::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawHandle,
};
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub kind: String,
    pub file: Owned,
    pub backup: String,
    /// Prepared copy on the destination volume. Its file ID survives publication.
    #[serde(default)]
    pub staging: Option<String>,
    #[serde(default)]
    pub restore_staging: Option<String>,
    #[serde(default)]
    pub restore_file_id: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub before: State,
    pub after: State,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub before_layout: Option<native::Layout>,
    #[serde(default)]
    pub undo_layout: Option<native::Layout>,
}
#[derive(Serialize)]
pub struct Preview {
    pub selected: Vec<String>,
    pub ranking: Vec<Ranked>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}
pub fn root(store: &Store) -> Result<PathBuf> {
    let override_root: String = store.get("test_desktop")?;
    if override_root.is_empty() {
        native::desktop()
    } else {
        Ok(override_root.into())
    }
}
fn real_desktop(store: &Store) -> Result<bool> {
    Ok(store.get::<String>("test_desktop")?.is_empty())
}
fn safe_file(path: &Path, root: &Path) -> Result<()> {
    ensure!(
        path.parent()
            .is_some_and(|p| norm(&p.to_string_lossy()) == norm(&root.to_string_lossy())),
        "Отказ: путь вне управляемого рабочего стола"
    );
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url")),
        "Only .lnk and .url shortcuts can be managed"
    );
    match path.symlink_metadata() {
        Ok(m) => ensure!(
            !is_reparse(&m) && m.is_file(),
            "Отказ: путь является ссылкой файловой системы или каталогом"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}
fn file_identity(path: &Path) -> Result<Option<String>> {
    let file = std::fs::File::open(path)?;
    identity_of(&file)
}
fn identity_of(file: &std::fs::File) -> Result<Option<String>> {
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle},
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        unsafe {
            GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
        }
        if info.nFileIndexHigh == 0 && info.nFileIndexLow == 0 {
            return Ok(None);
        }
        Ok(Some(format!(
            "{:08x}-{:08x}{:08x}",
            info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
        )))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(Some(format!("{}-{}", metadata.dev(), metadata.ino())))
    }
}
fn fingerprint(path: &Path) -> Result<String> {
    Ok(hash(&std::fs::read(path)?))
}
fn move_new(source: &Path, destination: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use windows::{
            Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_NORMAL, MOVEFILE_WRITE_THROUGH, MoveFileExW, SetFileAttributesW,
            },
            core::PCWSTR,
        };
        unsafe {
            // Deliberately omit REPLACE_EXISTING: collisions must preserve user files.
            MoveFileExW(
                PCWSTR(native::wide(&source.to_string_lossy()).as_ptr()),
                PCWSTR(native::wide(&destination.to_string_lossy()).as_ptr()),
                MOVEFILE_WRITE_THROUGH,
            )?;
            // Desktop staging files are hidden. Publication must expose the final
            // shortcut without briefly displaying temporary icons in Explorer.
            if !destination
                .file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(".desktop-pilot-"))
            {
                SetFileAttributesW(
                    PCWSTR(native::wide(&destination.to_string_lossy()).as_ptr()),
                    FILE_ATTRIBUTE_NORMAL,
                )?;
            }
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(source, destination)?;
        std::fs::remove_file(source)?;
    }
    Ok(())
}
fn copy_new(source: &Path, destination: &Path) -> Result<()> {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let bytes = std::fs::read(source)?;
    let temp = destination.with_file_name(format!(
        ".desktop-pilot-{}-{}-{}.tmp",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let mut options = std::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    options.attributes(0x2); // FILE_ATTRIBUTE_HIDDEN, including before publication.
    let mut file = options.open(&temp)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        move_new(&temp, destination)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
fn remove_owned(path: &Path, owned: &Owned) -> Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    options.access_mode(0x8000_0000 | 0x0001_0000).share_mode(1);
    let mut file = options.open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    ensure!(
        hash(&bytes) == owned.sha,
        "Ярлык изменился во время операции"
    );
    if let Some(expected) = &owned.file_id {
        ensure!(
            identity_of(&file)?.as_ref() == Some(expected),
            "Shortcut was replaced by another file; it was preserved"
        );
    }
    #[cfg(windows)]
    {
        use windows::Win32::{
            Foundation::HANDLE,
            Storage::FileSystem::{
                FILE_DISPOSITION_INFO, FileDispositionInfo, SetFileInformationByHandle,
            },
        };
        let disposition = FILE_DISPOSITION_INFO {
            DeleteFile: true.into(),
        };
        unsafe {
            SetFileInformationByHandle(
                HANDLE(file.as_raw_handle()),
                FileDispositionInfo,
                &disposition as *const _ as *const _,
                std::mem::size_of_val(&disposition) as u32,
            )?;
        }
    }
    #[cfg(not(windows))]
    {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
pub(crate) fn ensure_no_unfinished(store: &Store) -> Result<()> {
    let unfinished: i64 = store.db.query_row(
        "SELECT COUNT(*) FROM transactions WHERE status IN ('prepared','rolling_back')",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        unfinished == 0,
        "Есть незавершённая операция. Сначала нажмите «Отменить последнее изменение»."
    );
    Ok(())
}
struct StagedCopies(Vec<(PathBuf, Owned)>);
impl Drop for StagedCopies {
    fn drop(&mut self) {
        for (path, owned) in &self.0 {
            let _ = remove_owned(path, owned);
        }
    }
}
fn shortcut_path(
    desktop: &Path,
    name: &str,
    extension: &str,
    language: &str,
    reserved: &mut HashSet<String>,
) -> Result<PathBuf> {
    let clean: String = name
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .take(64)
        .collect();
    let mut clean = clean.trim().trim_end_matches(['.', ' ']).to_owned();
    if clean.is_empty() {
        clean = "Shortcut".into();
    }
    let device = clean
        .split('.')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL", "CLOCK$", "CONIN$", "CONOUT$"].contains(&device.as_str())
        || device
            .strip_prefix("COM")
            .or_else(|| device.strip_prefix("LPT"))
            .is_some_and(|suffix| {
                ["1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³"].contains(&suffix)
            })
    {
        clean.insert(0, '_');
    }
    for suffix in 0..10_000 {
        let label = match suffix {
            0 => clean.clone(),
            1 => format!(
                "{clean} · {}",
                if language == "ru" {
                    "быстрый"
                } else {
                    "quick"
                }
            ),
            n => format!(
                "{clean} · {} ({n})",
                if language == "ru" {
                    "быстрый"
                } else {
                    "quick"
                }
            ),
        };
        let path = desktop.join(format!("{label}.{}", extension.to_ascii_lowercase()));
        let key = norm(&path.to_string_lossy());
        if !reserved.contains(&key) && !path.try_exists()? {
            safe_file(&path, desktop)?;
            reserved.insert(key);
            return Ok(path);
        }
    }
    bail!("Unable to find a free shortcut filename")
}
fn wait_removed(paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    for _ in 0..30 {
        let layout = native::layout()?;
        if !layout
            .items
            .iter()
            .any(|i| paths.iter().any(|p| norm(p) == norm(&i.path)))
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Проводник ещё не завершил обновление списка значков")
}

pub fn preview(store: &Store, initial: bool) -> Result<Preview> {
    let cfg: Config = store.get("config")?;
    cfg.validate()?;
    let apps = store.apps()?;
    let state: State = store.get("state")?;
    let days = store.days()?;
    preview_loaded(&cfg, &apps, &days, &state, initial, now())
}

/// Compute the exact production selection from one already-loaded Store snapshot.
/// Settings uses this after its initial catalog read, avoiding a second SQLite
/// SELECT and JSON parse of the complete catalog on every window refresh.
pub(crate) fn preview_loaded(
    cfg: &Config,
    apps: &[App],
    days: &[Day],
    state: &State,
    initial: bool,
    time: i64,
) -> Result<Preview> {
    cfg.validate()?;
    let ranking = rank(apps, days, cfg, time);
    let selected = select(apps, &ranking, state, cfg, initial);
    ensure!(
        selected.len() <= cfg.slots,
        "Ручных закреплений больше доступных мест. Увеличьте число мест или снимите закрепления."
    );
    let added = selected
        .iter()
        .filter(|id| !state.owned.iter().any(|o| o.app == **id))
        .cloned()
        .collect();
    let removed = state
        .owned
        .iter()
        .filter(|o| !selected.contains(&o.app))
        .map(|o| o.app.clone())
        .collect();
    Ok(Preview {
        selected,
        ranking,
        added,
        removed,
    })
}

#[cfg(test)]
type RefreshCommitHook = Box<dyn FnOnce(&Store)>;

#[cfg(test)]
thread_local! {
    static BEFORE_REFRESH_COMMIT: std::cell::RefCell<Option<RefreshCommitHook>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
thread_local! {
    /// Inject an actual user placement after the scan's Store/ownership snapshot
    /// but before its source I/O. This tests the real application logic.
    static BEFORE_REFRESH_DISCOVERY: std::cell::RefCell<Option<RefreshCommitHook>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
pub(crate) struct TestScanGate {
    pub home: PathBuf,
    pub entered: std::sync::mpsc::Sender<()>,
    pub release: std::sync::mpsc::Receiver<()>,
    pub completed: std::sync::mpsc::Sender<()>,
}
#[cfg(test)]
pub(crate) static TEST_SCAN_GATE: std::sync::Mutex<Option<TestScanGate>> =
    std::sync::Mutex::new(None);
#[cfg(test)]
pub(crate) struct TestMonitorCatalogObserver {
    pub home: PathBuf,
    pub sender: std::sync::mpsc::Sender<(bool, Vec<String>)>,
}
#[cfg(test)]
pub(crate) static TEST_MONITOR_CATALOG: std::sync::Mutex<Option<TestMonitorCatalogObserver>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
struct TestScanCompletion(std::sync::mpsc::Sender<()>);
#[cfg(test)]
impl Drop for TestScanCompletion {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

#[cfg(test)]
thread_local! {
    static SCAN_SHORTCUT_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
fn catalog_link(path: &Path) -> Result<App> {
    #[cfg(test)]
    SCAN_SHORTCUT_READS.with(|calls| calls.set(calls.get().saturating_add(1)));
    native::read_link(path)
}

pub fn refresh_apps(store: &Store) -> Result<()> {
    refresh_apps_guarded(store, || false)
}

/// A background scan may outlive its originating monitor after shutdown.
/// Check cancellation on the *actual* Store/catalog path, including before
/// committing, so late filesystem results cannot overwrite a newer session.
fn refresh_apps_guarded(store: &Store, cancelled: impl Fn() -> bool) -> Result<()> {
    // A placement or manual relocation can finish while the filesystem scan
    // still uses an older ownership snapshot. Retake that snapshot once instead
    // of promoting one of our own new desktop copies to an original.
    for attempt in 0..2 {
        if refresh_apps_once(store, &cancelled)? {
            return Ok(());
        }
        if attempt == 1 {
            bail!(
                "Состав быстрых ярлыков изменился во время проверки. Обновите список ещё раз; ручные изменения сохранены."
            );
        }
    }
    unreachable!("Two bounded scan attempts return or report a conflict")
}

fn refresh_apps_once(store: &Store, cancelled: &impl Fn() -> bool) -> Result<bool> {
    ensure!(!cancelled(), "Background catalog scan cancelled");
    ensure_no_unfinished(store)?;
    let real = real_desktop(store)?;
    let config: Config = store.get("config")?;
    let steam = if real {
        native::steam_locations()
    } else {
        HashMap::new()
    };
    let mut warnings = Vec::new();
    let mut unreadable_sources = HashSet::new();
    let pins = if real && config.respect_taskbar_pins {
        match native::pin_identities() {
            Ok(pins) => Some(pins),
            Err(e) => {
                warnings.push(format!("Taskbar pins could not be refreshed: {e:#}"));
                None
            }
        }
    } else {
        Some(Vec::new())
    };
    let state: State = store.get("state")?;
    let scanned_at = now();
    let mut known = store.apps()?;
    // These originals were already fully parsed and verified on this scan.
    // Reopening the very same file during folder discovery doubles Shell/URL
    // parsing for a warm catalog, without contributing different information.
    // Other aliases, changed targets and unreadable sources still take the
    // existing discovery and recovery paths below.
    let mut verified_saved_sources = HashSet::new();
    // A long filesystem scan can overlap a user's explicit shortcut import.
    // Record only the sources seen at scan start; the import transaction may
    // select a different original before these results are ready to persist.
    let scanned_sources: HashMap<_, _> = known
        .iter()
        .map(|app| (app.id.clone(), norm(&app.source)))
        .collect();
    #[cfg(test)]
    BEFORE_REFRESH_DISCOVERY.with(|slot| {
        let action = slot.borrow_mut().take();
        if let Some(action) = action {
            action(store);
        }
    });
    // This test-only boundary recreates a blocked shortcut filesystem read.
    // The rest of the scan, command queue and Store are production paths.
    #[cfg(test)]
    let _scan_completion = if let Some(gate) = {
        let mut slot = TEST_SCAN_GATE.lock().unwrap();
        if slot.as_ref().is_some_and(|gate| gate.home == store.home) {
            slot.take()
        } else {
            None
        }
    } {
        let _ = gate.entered.send(());
        let _ = gate.release.recv_timeout(Duration::from_secs(45));
        Some(TestScanCompletion(gate.completed))
    } else {
        None
    };
    for app in &mut known {
        ensure!(!cancelled(), "Background catalog scan cancelled");
        match catalog_link(Path::new(&app.source)) {
            Ok(actual) if actual.identity() != app.identity() => {
                // A different launch identity is an actual change, not a temporary
                // unavailable drive. Do not keep managing the obsolete copy.
                app.valid = false;
                app.unavailable_since = 0;
            }
            Ok(actual) if actual.valid => {
                verified_saved_sources.insert(norm(&app.source));
                // Launch identity is unchanged. Refresh only the non-identity
                // metadata read from the original file. Never rewrite the
                // independently managed desktop copy or its user's position.
                app.name = actual.name;
                app.icon = actual.icon;
                app.icon_index = actual.icon_index;
                app.valid = true;
                app.unavailable_since = 0;
            }
            Ok(_) => {
                app.valid = false;
                if app.unavailable_since == 0 || app.unavailable_since > scanned_at {
                    app.unavailable_since = scanned_at;
                }
            }
            Err(error) => {
                unreadable_sources.insert(app.id.clone());
                let missing = match std::fs::metadata(&app.source) {
                    Ok(metadata) => !metadata.is_file(),
                    Err(error) => error.kind() == std::io::ErrorKind::NotFound,
                };
                if missing {
                    app.valid = false;
                    if app.unavailable_since == 0 || app.unavailable_since > scanned_at {
                        app.unavailable_since = scanned_at;
                    }
                } else {
                    // A sharing violation or an installer/sync client's partial
                    // write does not prove that the existing quick copy is broken.
                    // New copies still re-read and verify the original in apply().
                    warnings.push(format!(
                        "Source shortcut could not be refreshed; its last verified state was kept: {}: {error:#}",
                        app.source
                    ));
                }
            }
        }
        if !app.valid
            && app.policy != Policy::Never
            && app.unavailable_since > 0
            && let Some(copy) = state.owned.iter().find(|o| o.app == app.id)
        {
            if app.policy == Policy::Always || copy.manual {
                warnings.push(format!(
                    "The manually protected quick shortcut stays in place until you change its rule or remove it. The original source or target is unavailable: {}",
                    app.source
                ));
            } else if scanned_at.saturating_sub(app.unavailable_since) < UNAVAILABLE_GRACE_SECONDS {
                warnings.push(format!(
                    "The existing quick copy is held for up to 72 hours; new copies are not created. Original source or target is temporarily unavailable: {}",
                    app.source
                ));
            }
        }
        if let Some(pins) = &pins {
            app.pinned = real && config.respect_taskbar_pins && native::is_pinned(app, pins);
        }
        if real && let Some(id) = app.target.strip_prefix("steam://rungameid/") {
            app.observed_roots = steam.get(id).cloned().into_iter().collect();
        }
    }
    // A locked or inaccessible unrelated subfolder must not stop the catalog.
    // Keep discovery bounded, but include the documented 5,000-entry catalog
    // workflow; the previous 3,000-link ceiling silently hid later shortcuts.
    const MAX_DISCOVERED_SHORTCUTS: usize = 6_000;
    fn visit(
        path: &Path,
        depth: usize,
        out: &mut Vec<PathBuf>,
        warnings: &mut Vec<String>,
    ) -> Result<()> {
        if depth > 5 || out.len() >= MAX_DISCOVERED_SHORTCUTS {
            return Ok(());
        }
        for entry in std::fs::read_dir(path)? {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    warnings.push(e.to_string());
                    continue;
                }
            };
            let p = entry.path();
            let m = match p.symlink_metadata() {
                Ok(m) => m,
                Err(e) => {
                    warnings.push(e.to_string());
                    continue;
                }
            };
            if is_reparse(&m) {
                continue;
            }
            if m.is_dir() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if [
                    "служебное",
                    "node_modules",
                    "архив ярлыков",
                    ".git",
                    "venv",
                    ".venv",
                ]
                .contains(&name.as_str())
                {
                    continue;
                }
                if let Err(e) = visit(&p, depth + 1, out, warnings) {
                    warnings.push(format!("{}: {e:#}", p.display()));
                }
            } else if m.is_file()
                && p.extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url"))
            {
                out.push(p);
                if out.len() >= MAX_DISCOVERED_SHORTCUTS {
                    warnings.push(format!("Shortcut scan limit reached ({MAX_DISCOVERED_SHORTCUTS}); existing catalog entries were retained"));
                    break;
                }
            }
        }
        Ok(())
    }
    let desktop = root(store)?;
    let mut roots = vec![(desktop.clone(), "Desktop")];
    if real {
        for (optional, category) in [
            (native::common_desktop(), "Desktop"),
            (native::programs(), "Start menu"),
            (native::common_programs(), "Start menu"),
        ] {
            match optional {
                Ok(path) if path.is_dir() => roots.push((path, category)),
                Ok(_) => {}
                Err(e) => warnings.push(format!("{category} could not be scanned: {e:#}")),
            }
        }
    }
    let mut paths = Vec::new();
    for (index, (root, _)) in roots.iter().enumerate() {
        if let Err(e) = visit(root, 0, &mut paths, &mut warnings) {
            if index == 0 {
                return Err(e);
            }
            warnings.push(format!("{}: {e:#}", root.display()));
        }
    }
    paths.sort_by_key(|p| norm(&p.to_string_lossy()));
    paths.dedup_by(|a, b| norm(&a.to_string_lossy()) == norm(&b.to_string_lossy()));
    // A user may rename a managed quick copy before manual reconciliation.
    // A missing or replaced saved path alone does not mean the copy became
    // an independent original. Its filesystem identity remains authoritative.
    // The common case (all managed files still at their saved paths) performs
    // no identity checks on discovered shortcuts.
    let displaced_owned_ids: HashSet<_> = state
        .owned
        .iter()
        .filter_map(|owned| {
            let expected = owned.file_id.as_ref()?;
            match file_identity(Path::new(&owned.path)) {
                Ok(Some(actual)) if &actual == expected => None,
                _ => Some(expected.clone()),
            }
        })
        .collect();
    let executable = norm(&std::env::current_exe()?.to_string_lossy());
    let installed = norm(&store.home.join("desktop-pilot.exe").to_string_lossy());
    // Scan discovery used to compare every parsed shortcut against the entire
    // saved catalog, repeatedly rebuilding each identity (quadratic work for
    // large custom directories). Keep the first matching row just as the
    // original Vec::position did, including old profiles with duplicate IDs.
    // Every new identity is indexed as it enters the live discovered catalog.
    let mut by_identity: HashMap<String, usize> = HashMap::with_capacity(known.len());
    for (index, saved) in known.iter().enumerate() {
        by_identity.entry(saved.identity()).or_insert(index);
    }
    for path in paths {
        ensure!(!cancelled(), "Background catalog scan cancelled");
        if verified_saved_sources.contains(&norm(&path.to_string_lossy())) {
            continue;
        }
        if state
            .owned
            .iter()
            .any(|o| norm(&o.path) == norm(&path.to_string_lossy()))
        {
            continue;
        }
        if !displaced_owned_ids.is_empty() {
            match file_identity(&path) {
                Ok(Some(id)) if displaced_owned_ids.contains(&id) => continue,
                Ok(_) => {}
                Err(error) => {
                    // Unverifiable ownership must not silently be promoted to
                    // a new source while a managed file is displaced.
                    warnings.push(format!(
                        "Shortcut identity could not be checked: {}: {error:#}",
                        path.display()
                    ));
                    continue;
                }
            }
        }
        if let Ok(mut app) = catalog_link(&path) {
            if [executable.as_str(), installed.as_str()].contains(&norm(&app.target).as_str()) {
                continue;
            }
            // Match logical identities as well as IDs so legacy URL identities keep
            // their statistics and policies after correcting URL case handling.
            let identity = app.identity();
            let old = by_identity.get(&identity).copied();
            if let Some(index) = old {
                // Keep a stable verified source, but do not let its cached
                // validity hide a readable alias during a lock or partial write.
                // Full launch identity was matched above; an invalid alias must
                // never replace the last verified state of an unreadable source.
                if known[index].valid
                    && (!unreadable_sources.contains(&known[index].id) || !app.valid)
                {
                    continue;
                }
                let previous = &known[index];
                app.id = previous.id.clone();
                app.policy = previous.policy.clone();
                app.seed = previous.seed;
                app.seed_at = previous.seed_at;
                app.unavailable_since = if app.valid {
                    0
                } else if previous.unavailable_since > 0 && previous.unavailable_since <= scanned_at
                {
                    previous.unavailable_since
                } else {
                    scanned_at
                };
                app.observed_roots = previous.observed_roots.clone();
                app.note = previous.note.clone();
            }
            let catalog_root = roots.iter().find(|(root, _)| path.starts_with(root));
            app.category = catalog_root
                .and_then(|(root, _)| path.parent()?.strip_prefix(root).ok())
                .filter(|relative| !relative.as_os_str().is_empty())
                .map(|relative| relative.to_string_lossy().into_owned())
                .unwrap_or_else(|| {
                    catalog_root
                        .map_or("Desktop", |(_, category)| *category)
                        .into()
                });
            if let Some(pins) = &pins {
                app.pinned = real && config.respect_taskbar_pins && native::is_pinned(&app, pins);
            } else if let Some(index) = old {
                app.pinned = known[index].pinned;
            }
            if real && let Some(id) = app.target.strip_prefix("steam://rungameid/") {
                app.observed_roots = steam.get(id).cloned().into_iter().collect();
            }
            if !app.valid && app.unavailable_since == 0 {
                app.unavailable_since = scanned_at;
            }
            // A valid registered URI or Shell application is not an unsafe
            // shortcut merely because it lacks an ordinary executable path.
            // Keep the conservative opt-in rule for unresolved targets only.
            if old.is_none()
                && !app.valid
                && !Path::new(&expand_env(&app.target)).is_file()
                && !app.target.contains("://")
            {
                app.note = "Специальный запуск: включение только вручную".into();
                app.policy = Policy::Never;
            }
            if let Some(index) = old {
                if app.valid {
                    unreadable_sources.remove(&app.id);
                }
                known[index] = app;
            } else {
                by_identity.entry(identity).or_insert(known.len());
                known.push(app);
            }
        }
    }
    #[cfg(test)]
    BEFORE_REFRESH_COMMIT.with(|callback| {
        if let Some(callback) = callback.borrow_mut().take() {
            callback(store);
        }
    });
    store.transaction(|| {
        ensure!(!cancelled(), "Background catalog scan cancelled");
        // This is a real BEGIN IMMEDIATE transaction. Once the current
        // ownership is checked, no other UI/worker connection can commit
        // placement changes until this scan's writes have completed.
        //
        // Keeping a pre-placement snapshot can turn a newly created (or newly
        // renamed) managed quick copy into its own original if that original
        // became unavailable during source I/O. Discard the entire stale scan
        // and retry against fresh ownership, rather than merely skipping an
        // individual entry or guessing from its bytes.
        let current: State = store.get("state")?;
        if current.owned != state.owned {
            return Ok(false);
        }
        // A placement may have entered its prepared state while scanning.
        // Recheck under the writer before changing any catalog row.
        ensure_no_unfinished(store)?;
        let pins_verified = !real || !config.respect_taskbar_pins || pins.is_some();
        if store.get::<bool>("pins_verified")? != pins_verified {
            store.set("pins_verified", &pins_verified)?;
        }
        for mut app in known {
            ensure!(!cancelled(), "Background catalog scan cancelled");
            if let Ok(latest) = store.app(&app.id) {
                // A newer import wins over a stale scanner snapshot. This also
                // protects an app first discovered during the scan: if it was
                // imported meanwhile, it did not exist in scanned_sources.
                if scanned_sources
                    .get(&app.id)
                    .is_none_or(|source| source != &norm(&latest.source))
                {
                    continue;
                }
                // A policy changed while the filesystem scan was running belongs to
                // the user; refresh only filesystem-derived fields.
                app.policy = latest.policy.clone();
                app.seed = latest.seed;
                app.seed_at = latest.seed_at;
                app.note = latest.note.clone();
                // Do not rewrite unchanged catalog rows during routine rescans.
                if app == latest {
                    continue;
                }
            }
            store.put_app(&app)?;
        }
        ensure!(!cancelled(), "Background catalog scan cancelled");
        let warning = warnings.join("\n");
        if store.get::<String>("catalog_warning")? != warning {
            store.set("catalog_warning", &warning)?;
            if !warning.is_empty() {
                store.event("warning", &warning)?;
            }
        }
        Ok(true)
    })
}

pub fn reconcile_manual(store: &Store) -> Result<()> {
    let _lock = native::exclusive("changes", &store.home)?;
    reconcile_manual_locked(store)
}
fn reconcile_manual_locked(store: &Store) -> Result<()> {
    // A half-applied transaction is not evidence of the user's manual intentions.
    ensure_no_unfinished(store)?;
    let mut state: State = store.get("state")?;
    if state.owned.is_empty() {
        return Ok(());
    }
    let desktop = root(store)?;
    let snap = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    if let Some(l) = &snap
        && !state.topology.is_empty()
        && l.topology != state.topology
    {
        bail!(
            "Изменилась геометрия экрана. Нажмите «Принять текущее размещение» в настройках; существующие значки не перемещались."
        );
    }
    let apps = store.apps()?;
    let sources: HashSet<_> = apps.iter().map(|a| norm(&a.source)).collect();
    let managed: HashSet<_> = state.owned.iter().map(|o| norm(&o.path)).collect();
    let mut kept = Vec::new();
    let mut policies = Vec::new();
    let mut messages = Vec::new();
    for mut o in state.owned.clone() {
        safe_file(Path::new(&o.path), &desktop)?;
        let app = apps.iter().find(|a| a.id == o.app);
        let display_name = app.map_or(o.app.as_str(), |a| a.name.as_str()).to_owned();
        if sources.contains(&norm(&o.path)) {
            // Repair old profiles that accidentally claimed an original copy.
            policies.push((o.app.clone(), Policy::Never));
            messages.push(format!(
                "Original shortcut released from management: {display_name}"
            ));
            continue;
        }
        if !Path::new(&o.path).try_exists()? {
            let mut renamed = Vec::new();
            if let Some(expected) = &o.file_id {
                for entry in std::fs::read_dir(&desktop)? {
                    let p = entry?.path();
                    let key = norm(&p.to_string_lossy());
                    if sources.contains(&key)
                        || managed.contains(&key)
                        || kept.iter().any(|item: &Owned| norm(&item.path) == key)
                    {
                        continue;
                    }
                    if safe_file(&p, &desktop).is_ok()
                        && file_identity(&p).is_ok_and(|id| id.as_ref() == Some(expected))
                        && fingerprint(&p).is_ok_and(|sha| sha == o.sha)
                    {
                        renamed.push(p);
                    }
                }
            }
            if renamed.len() == 1 {
                o.path = renamed[0].to_string_lossy().into_owned();
                o.manual = true;
                if !app.is_some_and(|a| a.policy == Policy::Never) {
                    policies.push((o.app.clone(), Policy::Always));
                }
                messages.push(format!("Переименование принято: {display_name}"));
            } else {
                policies.push((o.app.clone(), Policy::Never));
                messages.push(format!(
                    "Ручное удаление/перенос: {display_name} исключён из возврата"
                ));
                continue;
            }
        }
        let actual_id = file_identity(Path::new(&o.path))?;
        if fingerprint(Path::new(&o.path))? != o.sha
            || o.file_id
                .as_ref()
                .is_some_and(|expected| actual_id.as_ref() != Some(expected))
        {
            policies.push((o.app.clone(), Policy::Never));
            messages.push("Изменённый пользователем ярлык оставлен без управления".into());
            continue;
        }
        o.file_id = actual_id;
        if let Some(l) = &snap
            && let Some(item) = l.items.iter().find(|i| norm(&i.path) == norm(&o.path))
            && item.position != o.position
        {
            o.position = item.position.clone();
            o.manual = true;
            if !app.is_some_and(|a| a.policy == Policy::Never) {
                policies.push((o.app.clone(), Policy::Always));
            }
            messages.push(format!("Ручное положение закреплено: {display_name}"));
        }
        kept.push(o);
    }
    state.owned = kept;
    store.transaction(|| {
        for (id, policy) in policies {
            if apps.iter().any(|a| a.id == id) {
                store.policy(&id, policy)?;
            }
        }
        for message in messages {
            store.event("manual", &message)?;
        }
        store.set("state", &state)
    })
}

fn grid_offset(layout: &native::Layout) -> Result<(i64, i64)> {
    let sx = i64::from(layout.spacing.x);
    let sy = i64::from(layout.spacing.y);
    ensure!(
        sx > 0
            && sy > 0
            && layout.icon_size > 0
            && i64::from(layout.icon_size) <= sx
            && i64::from(layout.icon_size) < sy,
        "Icon size does not fit the desktop grid"
    );
    let base = layout
        .items
        .iter()
        .find(|i| Path::new(&i.path).is_dir())
        .or_else(|| layout.items.first());
    Ok((
        base.map_or((sx - i64::from(layout.icon_size)) / 2, |b| {
            i64::from(b.position.x).rem_euclid(sx)
        }),
        base.map_or(2, |b| i64::from(b.position.y).rem_euclid(sy)),
    ))
}
/// Exact cells used by both the preview UI and the placement engine. Occupied or
/// off-screen candidates are retained here so the UI can explain the conflict.
pub(crate) fn zone_candidates(layout: &native::Layout, cfg: &Config) -> Result<Vec<Point>> {
    ensure!(
        (1..=8).contains(&cfg.slots)
            && (1..=8).contains(&cfg.zone_columns)
            && (0..=40).contains(&cfg.zone_col)
            && (0..=40).contains(&cfg.zone_row),
        "Недопустимая область размещения"
    );
    let (bx, by) = grid_offset(layout)?;
    let (sx, sy) = (i64::from(layout.spacing.x), i64::from(layout.spacing.y));
    let start_col = -(-i64::from(layout.work[0])).div_euclid(sx);
    let start_row = -(-i64::from(layout.work[1])).div_euclid(sy);
    (0..cfg.slots as i64)
        .map(|index| {
            Ok(Point {
                x: i32::try_from(
                    bx + (start_col
                        + i64::from(cfg.zone_col)
                        + index % i64::from(cfg.zone_columns))
                        * sx,
                )
                .context("Desktop coordinates overflow")?,
                y: i32::try_from(
                    by + (start_row
                        + i64::from(cfg.zone_row)
                        + index / i64::from(cfg.zone_columns))
                        * sy,
                )
                .context("Desktop coordinates overflow")?,
            })
        })
        .collect()
}
pub(crate) fn reserve_positions(
    layout: &native::Layout,
    cfg: &Config,
    state: &State,
    removed: &[String],
    needed: usize,
) -> Result<Vec<Point>> {
    if needed == 0 {
        return Ok(vec![]);
    }
    let (bx, by) = grid_offset(layout)?;
    let (sx, sy) = (i64::from(layout.spacing.x), i64::from(layout.spacing.y));
    let retiring: HashSet<_> = state
        .owned
        .iter()
        .filter(|o| removed.contains(&o.app))
        .map(|o| norm(&o.path))
        .collect();
    let mut result = Vec::new();
    for point in zone_candidates(layout, cfg)? {
        let (x, y) = (i64::from(point.x), i64::from(point.y));
        if x - bx < i64::from(layout.work[0])
            || y - by < i64::from(layout.work[1])
            || x - bx + sx > i64::from(layout.work[2])
            || y - by + sy > i64::from(layout.work[3])
        {
            continue;
        }
        if layout.items.iter().any(|i| {
            !retiring.contains(&norm(&i.path))
                && (i64::from(i.position.x) - x).abs() < sx - 2
                && (i64::from(i.position.y) - y).abs() < sy - 2
        }) {
            continue;
        }
        result.push(point);
        if result.len() == needed {
            break;
        }
    }
    ensure!(
        result.len() >= needed,
        "В выбранной области не хватает свободных мест. Существующие значки не перемещены."
    );
    Ok(result)
}

pub fn apply(store: &Store, initial: bool) -> Result<String> {
    let _lock = native::exclusive("changes", &store.home)?;
    ensure!(
        store
            .get::<Progress>("learning")?
            .ready(&store.get::<Config>("config")?),
        "Passive observation is still in progress. No shortcuts were changed."
    );
    ensure_no_unfinished(store)?;
    refresh_apps(store)?;
    ensure!(
        store.get::<bool>("pins_verified")?,
        "Taskbar pins could not be verified. Refresh the list or turn off taskbar pin filtering in Settings."
    );
    reconcile_manual_locked(store)?;
    let cfg: Config = store.get("config")?;
    let before: State = store.get("state")?;
    let choice = preview(store, initial || !before.initialized)?;
    let desktop = root(store)?;
    let snap = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    if let Some(l) = &snap {
        ensure!(
            l.flags & 1 == 0,
            "Автоупорядочивание Проводника включено. Изменения остановлены."
        );
        ensure!(
            before.topology.is_empty() || l.topology == before.topology,
            "Изменилась геометрия экрана. Нажмите «Принять текущее размещение»."
        );
    }
    let mut after = before.clone();
    after.last_review = now();
    after.initialized = before.initialized || !choice.selected.is_empty();
    if let Some(l) = &snap {
        after.topology = l.topology.clone();
    }
    if choice.added.is_empty() && choice.removed.is_empty() {
        store.set("state", &after)?;
        return Ok("Проверено: текущий состав сохранён".into());
    }
    let points = if let Some(l) = &snap {
        reserve_positions(l, &cfg, &before, &choice.removed, choice.added.len())?
    } else {
        (0..choice.added.len())
            .map(|i| Point {
                x: i as i32 * 76,
                y: 2,
            })
            .collect()
    };
    let id = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S-%f"),
        std::process::id()
    );
    let directory = store.home.join("history").join(&id);
    std::fs::create_dir_all(&directory)?;
    let mut tx = Transaction {
        id: id.clone(),
        before: before.clone(),
        after: State::default(),
        changes: vec![],
        before_layout: snap.clone(),
        undo_layout: None,
    };
    let mut staging = StagedCopies(Vec::new());
    for owned in &before.owned {
        if choice.removed.contains(&owned.app) {
            let p = Path::new(&owned.path);
            safe_file(p, &desktop)?;
            ensure!(
                fingerprint(p)? == owned.sha,
                "Ярлык изменился: удаление отменено"
            );
            let extension = p
                .extension()
                .and_then(|e| e.to_str())
                .context("Shortcut extension is missing")?;
            let backup = directory.join(format!("before-{}.{extension}", owned.app));
            copy_new(p, &backup)?;
            tx.changes.push(Change {
                kind: "remove".into(),
                file: owned.clone(),
                backup: backup.to_string_lossy().into_owned(),
                staging: None,
                restore_staging: None,
                restore_file_id: None,
            });
        }
    }
    after.owned.retain(|o| !choice.removed.contains(&o.app));
    let mut reserved = HashSet::new();
    for (id, point) in choice.added.iter().zip(points) {
        let app = store.app(id)?;
        ensure!(
            Path::new(&app.source).is_file(),
            "Исходный ярлык недоступен"
        );
        let extension = Path::new(&app.source)
            .extension()
            .and_then(|e| e.to_str())
            .context("Shortcut extension is missing")?;
        ensure!(
            extension.eq_ignore_ascii_case("lnk") || extension.eq_ignore_ascii_case("url"),
            "Unsupported shortcut extension"
        );
        let file = shortcut_path(&desktop, &app.name, extension, &cfg.language, &mut reserved)?;
        let stage = directory.join(format!("new-{id}.{extension}"));
        let source = Path::new(&app.source);
        ensure!(
            native::read_link(source)?.identity() == app.identity(),
            "Source shortcut changed before copying"
        );
        copy_new(source, &stage)?;
        ensure!(
            native::read_link(&stage)?.identity() == app.identity(),
            "Copied shortcut identity changed"
        );
        let publish = desktop.join(format!(".desktop-pilot-{}-{id}.tmp", tx.id));
        copy_new(&stage, &publish)?;
        let owned = Owned {
            app: id.clone(),
            path: file.to_string_lossy().into_owned(),
            sha: fingerprint(&stage)?,
            position: point,
            manual: app.policy == Policy::Always,
            since: now(),
            file_id: file_identity(&publish)?,
        };
        staging.0.push((publish.clone(), owned.clone()));
        ensure!(
            owned.file_id.is_some(),
            "The desktop filesystem cannot verify shortcut ownership; placement was cancelled"
        );
        tx.changes.push(Change {
            kind: "add".into(),
            file: owned.clone(),
            backup: stage.to_string_lossy().into_owned(),
            staging: Some(publish.to_string_lossy().into_owned()),
            restore_staging: None,
            restore_file_id: None,
        });
        after.owned.push(owned);
    }
    after.last_change = now();
    tx.after = after;
    store.db.execute(
        "INSERT INTO transactions VALUES(?1,?2,'prepared',?3)",
        params![tx.id, now(), serde_json::to_string(&tx)?],
    )?;
    let operation = (|| -> Result<()> {
        for c in &tx.changes {
            let file = Path::new(&c.file.path);
            safe_file(file, &desktop)?;
            if c.kind == "remove" {
                remove_owned(file, &c.file)?;
            } else {
                move_new(
                    Path::new(c.staging.as_ref().context("Prepared shortcut is missing")?),
                    file,
                )?;
                ensure!(
                    fingerprint(file)? == c.file.sha,
                    "Запись ярлыка не подтверждена"
                );
            }
        }
        if let Some(layout) = &snap {
            native::refresh(&desktop);
            wait_removed(
                &tx.changes
                    .iter()
                    .filter(|c| c.kind == "remove")
                    .map(|c| c.file.path.clone())
                    .collect::<Vec<_>>(),
            )?;
            let mut positions: Vec<_> = layout
                .items
                .iter()
                .filter(|i| {
                    !tx.changes
                        .iter()
                        .any(|c| c.kind == "remove" && norm(&c.file.path) == norm(&i.path))
                })
                .map(|i| (i.path.clone(), i.position.clone()))
                .collect();
            positions.extend(
                tx.changes
                    .iter()
                    .filter(|c| c.kind == "add")
                    .map(|c| (c.file.path.clone(), c.file.position.clone())),
            );
            let mut last = None;
            for _ in 0..20 {
                match native::position_many(&positions) {
                    Ok(()) => {
                        last = None;
                        break;
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            if let Some(e) = last {
                return Err(e);
            }
        }
        store.transaction(|| {
            store.set("state", &tx.after)?;
            store.db.execute(
                "UPDATE transactions SET status='committed' WHERE id=?1",
                [&tx.id],
            )?;
            Ok(())
        })?;
        Ok(())
    })();
    if let Err(e) = operation {
        let undo = rollback(store);
        bail!(
            "Изменение прервано: {e:#}. Автоматический откат: {}",
            undo.unwrap_or_else(|e| format!("{e:#}"))
        );
    }
    let message = format!(
        "Добавлено: {}; убрано быстрых копий: {}. Исходные ярлыки сохранены.",
        choice.added.len(),
        choice.removed.len()
    );
    store.event("layout", &message)?;
    Ok(message)
}

pub fn suggest_zone(layout: &native::Layout, slots: usize) -> Result<(i32, i32, i32)> {
    ensure!(
        layout.spacing.x > 0 && layout.spacing.y > 0,
        "Invalid desktop grid"
    );
    let columns = ((layout.work[2] - layout.work[0]) / layout.spacing.x).clamp(0, 40);
    let rows = ((layout.work[3] - layout.work[1]) / layout.spacing.y).clamp(0, 40);
    for width in [2, 1, 4] {
        for row in 0..rows {
            for col in 0..columns {
                let cfg = Config {
                    slots,
                    zone_col: col,
                    zone_row: row,
                    zone_columns: width,
                    ..Default::default()
                };
                if reserve_positions(layout, &cfg, &State::default(), &[], slots).is_ok() {
                    return Ok((col, row, width));
                }
            }
        }
    }
    bail!("No free area for the selected number of shortcuts. Reduce the number of slots.")
}

pub fn setup_complete(store: &Store) -> Result<bool> {
    Ok(store.get::<bool>("setup_complete")? || store.get::<State>("state")?.initialized)
}

fn safe_staging(path: &Path, desktop: &Path) -> Result<()> {
    ensure!(
        path.parent().is_some_and(
            |parent| norm(&parent.to_string_lossy()) == norm(&desktop.to_string_lossy())
        ) && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with(".desktop-pilot-"))
            && path.extension().is_some_and(|extension| extension == "tmp"),
        "Invalid transaction staging path"
    );
    match path.symlink_metadata() {
        Ok(metadata) => ensure!(
            metadata.is_file() && !is_reparse(&metadata),
            "Invalid transaction staging file"
        ),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
pub fn rollback(store: &Store) -> Result<String> {
    // Wall time may move backwards after a clock correction. Journal insertion
    // order is the order of changes; unfinished work still takes priority.
    let row = store.db.query_row(
        "SELECT id,data,status FROM transactions WHERE status IN ('prepared','rolling_back','committed') ORDER BY CASE WHEN status IN ('prepared','rolling_back') THEN 0 ELSE 1 END,rowid DESC LIMIT 1",
        [], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)));
    let (id, data, status) = match row {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok("Нет изменений для отката".into()),
        Err(e) => return Err(e.into()),
    };
    let mut tx: Transaction = serde_json::from_str(&data)?;
    ensure!(
        tx.id == id && !id.is_empty() && !id.contains(['/', '\\', ':']),
        "Invalid transaction identifier"
    );
    let desktop = root(store)?;
    let sources: HashSet<_> = store.apps()?.iter().map(|app| norm(&app.source)).collect();
    let current_state: State = store.get("state")?;
    let current_layout = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    let geometry_matches = current_layout.as_ref().is_none_or(|layout| {
        tx.before_layout
            .as_ref()
            .is_none_or(|before| before.topology == layout.topology)
    });
    if status != "rolling_back" {
        tx.undo_layout = if status == "prepared" && geometry_matches {
            tx.before_layout.clone().or_else(|| current_layout.clone())
        } else {
            current_layout.clone()
        };
    }
    let mut seen = HashSet::new();
    let mut foreign_adds = HashSet::new();
    // Validate the complete undo before changing even one user-visible file.
    for change in &mut tx.changes {
        ensure!(
            matches!(change.kind.as_str(), "add" | "remove"),
            "Invalid transaction operation"
        );
        let path = Path::new(&change.file.path);
        safe_file(path, &desktop)?;
        let key = norm(&change.file.path);
        ensure!(
            seen.insert(key.clone()),
            "Transaction contains conflicting shortcut paths"
        );
        ensure!(
            !sources.contains(&key),
            "An original shortcut cannot be modified by Undo"
        );
        let backup = Path::new(&change.backup);
        ensure!(
            backup
                .parent()
                .is_some_and(|parent| norm(&parent.to_string_lossy())
                    == norm(&store.home.join("history").join(&id).to_string_lossy())),
            "Invalid transaction backup path"
        );
        let metadata = backup.symlink_metadata()?;
        ensure!(
            metadata.is_file() && !is_reparse(&metadata) && fingerprint(backup)? == change.file.sha,
            "Повреждена резервная копия"
        );
        for staged in [&change.staging, &change.restore_staging]
            .into_iter()
            .flatten()
        {
            safe_staging(Path::new(staged), &desktop)?;
        }
        if path.try_exists()? {
            let actual_id = file_identity(path)?;
            if let Some(current) = current_state.owned.iter().find(|owned| {
                owned.app == change.file.app
                    && norm(&owned.path) == key
                    && owned.sha == change.file.sha
                    && owned.since == change.file.since
                    && owned.file_id.is_some()
                    && owned.file_id == actual_id
            }) {
                // An earlier Undo may have restored this same managed copy with
                // a new filesystem identity. Trust only the currently recorded
                // ownership and verify its exact ID; identical user replacements
                // still fail. Persist this identity with the undo intent below.
                change.file.file_id = current.file_id.clone();
            }
            let same_original = change
                .file
                .file_id
                .as_ref()
                .is_none_or(|expected| actual_id.as_ref() == Some(expected));
            let same_restoration = change
                .restore_file_id
                .as_ref()
                .is_some_and(|expected| actual_id.as_ref() == Some(expected));
            if change.kind == "add" && status != "committed" && !same_original {
                // A create-new collision never transferred ownership of this file.
                foreign_adds.insert(key);
                continue;
            }
            ensure!(
                (same_original || same_restoration) && fingerprint(path)? == change.file.sha,
                "Пользователь изменил файл; откат остановлен без перезаписи: {}",
                path.display()
            );
        }
    }
    let mut restore_staging = StagedCopies(Vec::new());
    for (index, change) in tx.changes.iter_mut().enumerate() {
        if change.kind != "remove" || Path::new(&change.file.path).try_exists()? {
            continue;
        }
        let staging = change
            .restore_staging
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| desktop.join(format!(".desktop-pilot-{id}-undo-{index}.tmp")));
        safe_staging(&staging, &desktop)?;
        if staging.try_exists()? {
            ensure!(
                fingerprint(&staging)? == change.file.sha
                    && change
                        .restore_file_id
                        .as_ref()
                        .is_some_and(|expected| file_identity(&staging)
                            .is_ok_and(|actual| actual.as_ref() == Some(expected))),
                "Undo staging file was changed; it was preserved"
            );
        } else {
            copy_new(Path::new(&change.backup), &staging)?;
        }
        let mut restored = change.file.clone();
        restored.file_id = file_identity(&staging)?;
        change.restore_file_id = restored.file_id.clone();
        change.restore_staging = Some(staging.to_string_lossy().into_owned());
        restore_staging.0.push((staging, restored));
    }
    // Persist the undo intent before touching files. An interrupted Undo is resumed
    // before older history and can never become evidence of a manual deletion.
    store.transaction(|| {
        let mut config: Config = store.get("config")?;
        config.automatic = false;
        store.set("config", &config)?;
        store.db.execute(
            "UPDATE transactions SET status='rolling_back',data=?2 WHERE id=?1",
            params![id, serde_json::to_string(&tx)?],
        )?;
        Ok(())
    })?;
    for change in tx.changes.iter().rev() {
        let path = Path::new(&change.file.path);
        if change.kind == "add" {
            if !foreign_adds.contains(&norm(&change.file.path)) && path.try_exists()? {
                remove_owned(path, &change.file)?;
            }
        } else if !path.try_exists()? {
            move_new(
                Path::new(
                    change
                        .restore_staging
                        .as_ref()
                        .context("Undo staging file is missing")?,
                ),
                path,
            )?;
        }
    }
    for change in &tx.changes {
        if let Some(staging) = &change.staging {
            let path = Path::new(staging);
            if path.try_exists()? {
                remove_owned(path, &change.file)?;
            }
        }
    }
    if let Some(layout) = &tx.undo_layout {
        native::refresh(&desktop);
        wait_removed(
            &tx.changes
                .iter()
                .filter(|c| c.kind == "add" && !foreign_adds.contains(&norm(&c.file.path)))
                .map(|c| c.file.path.clone())
                .collect::<Vec<_>>(),
        )?;
        let mut positions: Vec<_> = layout
            .items
            .iter()
            .filter(|item| {
                !tx.changes.iter().any(|c| {
                    c.kind == "add"
                        && !foreign_adds.contains(&norm(&c.file.path))
                        && norm(&c.file.path) == norm(&item.path)
                })
            })
            .filter(|item| Path::new(&item.path).exists() || item.path.starts_with("::"))
            .map(|item| (item.path.clone(), item.position.clone()))
            .collect();
        if geometry_matches {
            for owned in &tx.before.owned {
                if Path::new(&owned.path).exists()
                    && !positions
                        .iter()
                        .any(|(path, _)| norm(path) == norm(&owned.path))
                {
                    positions.push((owned.path.clone(), owned.position.clone()));
                }
            }
        }
        if layout.flags & 1 == 0 {
            let mut last = None;
            for _ in 0..20 {
                match native::position_many(&positions) {
                    Ok(()) => {
                        last = None;
                        break;
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            if let Some(e) = last {
                return Err(e);
            }
        }
    }
    let actual_layout = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    let mut restored = tx.before.clone();
    let mut kept = Vec::new();
    for mut owned in restored.owned {
        let was_removed = tx
            .changes
            .iter()
            .any(|change| change.kind == "remove" && change.file.app == owned.app);
        if !was_removed
            && let Some(current) = current_state
                .owned
                .iter()
                .find(|current| current.app == owned.app)
        {
            owned = current.clone();
        }
        let path = Path::new(&owned.path);
        if !path.try_exists()? || sources.contains(&norm(&owned.path)) {
            continue;
        }
        safe_file(path, &desktop)?;
        let actual_id = file_identity(path)?;
        if fingerprint(path)? != owned.sha {
            continue;
        }
        if !was_removed
            && owned
                .file_id
                .as_ref()
                .is_some_and(|expected| actual_id.as_ref() != Some(expected))
        {
            continue;
        }
        owned.file_id = actual_id;
        if let Some(layout) = &actual_layout
            && let Some(item) = layout
                .items
                .iter()
                .find(|item| norm(&item.path) == norm(&owned.path))
        {
            owned.position = item.position.clone();
        }
        kept.push(owned);
    }
    restored.owned = kept;
    if let Some(layout) = actual_layout {
        restored.topology = layout.topology;
    }
    store.transaction(|| {
        store.set("state", &restored)?;
        store.db.execute(
            "UPDATE transactions SET status='rolled_back' WHERE id=?1",
            [&id],
        )?;
        store.event(
            "rollback",
            &format!("Откат {id}; автоматика приостановлена"),
        )
    })?;
    Ok("Предыдущее размещение восстановлено. Автоматика приостановлена.".into())
}

pub fn command(store: &Store, action: &str) -> Result<String> {
    if let Some(rest) = action.strip_prefix("policy:") {
        let _lock = native::exclusive("changes", &store.home)?;
        ensure_no_unfinished(store)?;
        let (id, mode) = rest.split_once(':').context("Некорректная команда")?;
        let policy = match mode {
            "Auto" => Policy::Auto,
            "Always" => Policy::Always,
            "Never" => Policy::Never,
            _ => bail!("Неизвестное правило"),
        };
        store.transaction(|| {
            // Validate the application before altering the matching ownership flag.
            store.app(id)?;
            let mut s: State = store.get("state")?;
            for o in &mut s.owned {
                if o.app == id {
                    o.manual = policy == Policy::Always;
                }
            }
            store.set("state", &s)?;
            store.policy(id, policy)
        })?;
        return Ok("Правило сохранено. Нажмите «Применить сейчас» для изменения состава.".into());
    }
    match action {
        "apply" => apply(store, !store.get::<State>("state")?.initialized),
        "rollback" => {
            let _lock = native::exclusive("changes", &store.home)?;
            rollback(store)
        }
        "accept_geometry" => {
            let _lock = native::exclusive("changes", &store.home)?;
            ensure_no_unfinished(store)?;
            let mut s: State = store.get("state")?;
            let l = native::layout()?;
            s.topology = l.topology;
            for o in &mut s.owned {
                if let Some(i) = l.items.iter().find(|i| norm(&i.path) == norm(&o.path)) {
                    o.position = i.position.clone();
                }
            }
            store.set("state", &s)?;
            Ok("Текущая геометрия принята; существующие позиции сохранены".into())
        }
        "rescan" => {
            let _lock = native::exclusive("changes", &store.home)?;
            refresh_apps(store)?;
            Ok("Список ярлыков и закрепления обновлены".into())
        }
        "clear_stats" => {
            store.transaction(|| {
                store.db.execute("DELETE FROM days", [])?;
                for mut a in store.apps()? {
                    a.seed = 0.0;
                    a.seed_at = 0;
                    store.put_app(&a)?;
                }
                let mut progress: Progress = store.get("learning")?;
                progress.finish_if_ready(&store.get("config")?, now());
                store.set(
                    "learning",
                    &Progress {
                        started_at: now(),
                        completed_at: progress.completed_at,
                        ..Default::default()
                    },
                )?;
                store.set("app_session_times", &HashMap::<String, i64>::new())
            })?;
            Ok("История использования очищена; ярлыки и правила сохранены".into())
        }
        _ => bail!("Неизвестная команда"),
    }
}

pub fn observe_match(apps: &[App], process: &str) -> Option<String> {
    let process = norm(process);
    let mut matching = Vec::new();
    let mut profile_sensitive_target = false;
    for app in apps {
        let direct = norm(&app.target) == process;
        // A foreground process path contains no shortcut launch arguments.
        // Even an unavailable saved profile may still be running, so do not
        // infer a profile merely because another profile is currently valid.
        if direct && !app.arguments.trim().is_empty() {
            profile_sensitive_target = true;
        }
        if !app.valid {
            continue;
        }
        // A distinctive observed application root can still identify a
        // launched game even when its shortcut uses a URI or arguments.
        let rooted = !direct
            && app.observed_roots.iter().any(|root| {
                let root = norm(root).trim_end_matches('\\').to_owned();
                root.len() > 5 && process.starts_with(&(root + "\\"))
            });
        if direct || rooted {
            matching.push((app, direct));
        }
    }
    let matching: Vec<_> = matching
        .into_iter()
        .filter(|(_, direct)| !profile_sensitive_target || !*direct)
        .map(|(app, _)| app)
        .collect();
    let identities: HashSet<_> = matching.iter().map(|app| app.identity()).collect();
    if identities.len() == 1 {
        matching.first().map(|app| app.id.clone())
    } else {
        None
    }
}

fn observe_app_id(apps: &[App], id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    let ids: HashSet<_> = apps
        .iter()
        .filter(|a| {
            a.valid
                && [&a.target, &a.arguments].into_iter().any(|value| {
                    let value = value.trim().trim_matches('"').to_lowercase();
                    value
                        .strip_prefix("shell:appsfolder\\")
                        .or_else(|| {
                            value.strip_prefix("::{4234d49b-0245-4df3-b780-3893943456e1}\\")
                        })
                        .is_some_and(|suffix| suffix.eq_ignore_ascii_case(id))
                })
        })
        .map(|a| a.id.clone())
        .collect();
    if ids.len() == 1 {
        ids.into_iter().next()
    } else {
        None
    }
}

pub fn automatic_review(store: &Store, quiet: bool) -> Result<bool> {
    if store.get::<bool>("shutdown")? {
        return Ok(false);
    }
    let config: Config = store.get("config")?;
    config.validate()?;
    let state: State = store.get("state")?;
    let mut progress: Progress = store.get("learning")?;
    progress.finish_if_ready(&config, now());
    store.set("learning", &progress)?;
    let review_at = now();
    let review_due = !state.initialized
        || review_at
            >= state
                .last_review
                .saturating_add(config.interval_days * 86400)
        // A large backwards wall-clock correction must not block automatic
        // reviews for weeks. The usual quiet/learning checks still apply.
        || state.last_review > review_at.saturating_add(86400);
    if config.automatic
        && progress.ready(&config)
        && review_due
        && quiet
        && (state.initialized || !preview(store, true)?.selected.is_empty())
    {
        apply(store, !state.initialized)?;
        return Ok(true);
    }
    Ok(false)
}
const SHORT_VISIT_MEMORY_SECONDS: f64 = 20.0 * 60.0;
const MAX_SHORT_VISIT_APPS: usize = 128;

#[derive(Default)]
struct UsageWindow {
    seconds: f64,
    recorded: u32,
    last_seen: f64,
}

#[derive(Default)]
struct UsageAccumulator {
    previous: Option<String>,
    elapsed: f64,
    windows: HashMap<String, UsageWindow>,
}
impl UsageAccumulator {
    fn sample(&mut self, id: Option<&str>, elapsed: f64) -> Option<(String, u32, bool)> {
        // Never convert a delayed wake-up or invalid tick into foreground usage.
        if !elapsed.is_finite() || !(0.0..5.0).contains(&elapsed) {
            *self = Self::default();
            return None;
        }
        self.elapsed += elapsed;
        self.windows
            .retain(|_, window| self.elapsed - window.last_seen < SHORT_VISIT_MEMORY_SECONDS);
        let Some(id) = id.filter(|id| !id.is_empty()) else {
            self.previous = None;
            return None;
        };
        let continuing = self.previous.as_deref() == Some(id);
        self.previous = Some(id.to_owned());
        if !self.windows.contains_key(id)
            && self.windows.len() >= MAX_SHORT_VISIT_APPS
            && let Some(oldest) = self
                .windows
                .iter()
                .min_by(|a, b| a.1.last_seen.total_cmp(&b.1.last_seen))
                .map(|(key, _)| key.clone())
        {
            self.windows.remove(&oldest);
        }
        let window = self.windows.entry(id.to_owned()).or_default();
        window.last_seen = self.elapsed;
        // The first sample after a switch is ambiguous. Repeated confirmed
        // short visits to one app may count, but never the switching tick.
        if !continuing {
            return None;
        }
        window.seconds += elapsed;
        let whole = window.seconds.floor().min(f64::from(u32::MAX)) as u32;
        if whole < 10 || whole <= window.recorded {
            return None;
        }
        let first = window.recorded == 0;
        let delta = whole - window.recorded;
        window.recorded = whole;
        Some((id.to_owned(), delta, first))
    }
}
// The usage write is one SQLite transaction; retry only lock contention, not
// corruption, permissions, disk-full errors or an ambiguous non-SQLite failure.
fn retry_locked_usage(mut write: impl FnMut() -> Result<()>) -> Result<()> {
    const ATTEMPTS: usize = 3;
    for attempt in 0..ATTEMPTS {
        match write() {
            Ok(()) => return Ok(()),
            Err(error)
                if attempt + 1 < ATTEMPTS
                    && error.chain().any(|source| {
                        source
                            .downcast_ref::<rusqlite::Error>()
                            .and_then(rusqlite::Error::sqlite_error_code)
                            .is_some_and(|code| {
                                matches!(
                                    code,
                                    rusqlite::ErrorCode::DatabaseBusy
                                        | rusqlite::ErrorCode::DatabaseLocked
                                )
                            })
                    }) =>
            {
                std::thread::sleep(Duration::from_millis(125));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("the last attempt always returns")
}
fn record_usage(
    store: &Store,
    session_times: &mut HashMap<String, i64>,
    id: &str,
    seconds: u32,
    first: bool,
    time: i64,
) -> Result<()> {
    let session = first
        && session_times.get(id).is_none_or(|last| {
            // A clearly future checkpoint can result from correcting the
            // wall clock. Keep the usual gap for small time adjustments;
            // only an independently confirmed first visit can count.
            time.saturating_sub(*last) >= 1200 || last.saturating_sub(time) >= 1200
        });
    // Stage the in-memory session state until both usage and the checkpoint
    // have committed. A storage error must not advance only one of the two.
    let next_times = store.transaction(|| {
        store.observe(id, seconds, session, false)?;
        // Measure the gap from the last confirmed activity, not the beginning
        // of a possibly hours-long session. Persist it with the usage counters
        // so a brief focus switch or worker restart cannot inflate the ranking.
        let mut updated = session_times.clone();
        updated.insert(id.into(), time);
        // Per-visit timestamps cannot establish the age of unrelated apps
        // after a forward clock correction. Retention belongs to the guarded
        // SQLite housekeeping path, not to the foreground usage writer.
        store.set("app_session_times", &updated)?;
        Ok(updated)
    })?;
    *session_times = next_times;
    Ok(())
}
fn housekeeping_sessions(store: &Store, sessions: &mut HashMap<String, i64>) -> Result<()> {
    store.housekeeping()?;
    // The worker must discard any entries expired by the authoritative cleanup,
    // rather than accidentally resurrecting them from its in-memory cache.
    *sessions = store.get("app_session_times")?;
    Ok(())
}

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(15);

/// Keep startup and explicit commands responsive without rewriting the same
/// SQLite heartbeat on every two-second idle monitor tick.
fn publish_heartbeat_if_due(
    store: &Store,
    last_written: &mut Option<Instant>,
    tick: Instant,
    force: bool,
) -> Result<bool> {
    if !force
        && last_written
            .as_ref()
            .is_some_and(|last| tick.saturating_duration_since(*last) < HEARTBEAT_INTERVAL)
    {
        return Ok(false);
    }
    store.set("heartbeat", &now())?;
    // A failed SQLite write never advances the next-eligible checkpoint.
    *last_written = Some(tick);
    Ok(true)
}

pub fn monitor(home: PathBuf) -> Result<()> {
    let _com = native::Com::new()?;
    let store = Store::open(&home)?;
    store.recover_commands()?;
    let mut progress: Progress = store.get("learning")?;
    if progress.started_at == 0 && store.get::<State>("state")?.initialized {
        progress.completed_at = now();
        progress.started_at = now();
        store.set("learning", &progress)?;
    }
    // Read the revision before the catalog. If a manual import commits between
    // these two reads, at worst the next poll refreshes the same entry twice.
    // The reverse order could miss an import until a six-hour filesystem scan.
    let mut catalog_epoch: u64 = store.get("catalog_epoch")?;
    let mut apps = store.apps()?;
    let mut focus = crate::activity::FocusEvidence::default();
    let mut usage = UsageAccumulator::default();
    let mut learning_fraction = 0.0f64;
    let mut session_times: HashMap<String, i64> = store.get("app_session_times")?;
    let mut last_tick = Instant::now();
    let mut last_heartbeat = None;
    let mut maintenance: Option<Instant> = None;
    let mut scanned: Option<Instant> = None;
    let mut scan: Option<scan_worker::ScanWorker> = None;
    let mut last_autostart = None;
    let mut autostart_retry: Option<Instant> = None;
    let mut previous_collect = None;
    loop {
        // Keep stop intent visible until the process releases its mutex. Only
        // the next explicit startup may clear it and advertise readiness.
        if store.get::<bool>("shutdown")? {
            return Ok(());
        }
        // Filesystem enumeration runs separately from the polling/command loop.
        // The UI can still change rules or stop monitoring while one source hangs.
        if let Some(worker) = &mut scan {
            let finished = match worker.receiver.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(anyhow::anyhow!(
                    "Background shortcut scanner stopped unexpectedly"
                ))),
            };
            if let Some(result) = finished {
                let requested = std::mem::take(&mut worker.requests);
                scan = None;
                let completed: Result<String> =
                    result.map(|_| "Список ярлыков и закрепления обновлены".into());
                for id in requested {
                    store.finish(id, &completed)?;
                }
                match completed {
                    Ok(_) => {
                        apps = store.apps()?;
                        scanned = Some(Instant::now());
                        housekeeping_sessions(&store, &mut session_times)?;
                        // Apply manual reconciliation and automatic review only
                        // after the complete verified scan has been committed.
                        maintenance = None;
                    }
                    Err(error) => {
                        store.set("status", &format!("Изменения приостановлены: {error:#}"))?;
                        // Retry a failed periodic scan on the next maintenance
                        // interval, not in an uncontrolled two-second loop.
                        maintenance = Some(Instant::now());
                    }
                }
            }
        }
        for (id, action) in store.pending()? {
            if store.get::<bool>("shutdown")? {
                return Ok(());
            }
            if action == "rescan" {
                if store.claim_command(id)? {
                    let _ = publish_heartbeat_if_due(
                        &store,
                        &mut last_heartbeat,
                        Instant::now(),
                        true,
                    )?;
                    scan.get_or_insert_with(|| scan_worker::start(&store.home))
                        .requests
                        .push(id);
                }
                continue;
            }
            if !store.claim_command(id)? {
                continue;
            }
            let _ = publish_heartbeat_if_due(&store, &mut last_heartbeat, Instant::now(), true)?;
            let result = command(&store, &action);
            store.finish(id, &result)?;
            apps = store.apps()?;
            if action == "clear_stats" && result.is_ok() {
                usage = UsageAccumulator::default();
                learning_fraction = 0.0;
                session_times.clear();
            }
            maintenance = None;
        }
        if store.get::<bool>("shutdown")? {
            return Ok(());
        }
        // Settings can import shortcuts directly without a command. The
        // saved catalog should become usable for foreground matching on the
        // next poll, even when the separate filesystem scan is still running.
        let current_catalog_epoch: u64 = store.get("catalog_epoch")?;
        if current_catalog_epoch != catalog_epoch {
            apps = store.apps()?;
            catalog_epoch = current_catalog_epoch;
        }
        let config: Config = store.get("config")?;
        config.validate()?;
        crate::activity::set_enabled(config.collect);
        if previous_collect != Some(config.collect) {
            previous_collect = Some(config.collect);
            usage = UsageAccumulator::default();
            learning_fraction = 0.0;
            focus = crate::activity::FocusEvidence::default();
        }
        if last_autostart != Some(config.autostart)
            && autostart_retry.is_none_or(|last| last.elapsed() >= Duration::from_secs(30))
        {
            autostart_retry = Some(Instant::now());
            let result = if real_desktop(&store)? {
                native::autostart(&std::env::current_exe()?, &store.home, config.autostart)
            } else {
                Ok(())
            };
            let warning = match result {
                Ok(()) => {
                    last_autostart = Some(config.autostart);
                    String::new()
                }
                Err(error) => format!("Autostart could not be updated: {error:#}"),
            };
            if store.get::<String>("autostart_warning")? != warning {
                store.set("autostart_warning", &warning)?;
                if !warning.is_empty() {
                    store.event("warning", &warning)?;
                }
            }
        }
        let elapsed = last_tick.elapsed().as_secs_f64();
        last_tick = Instant::now();
        let mut active_id = None;
        let mut learning_active = false;
        if config.collect && elapsed < 5.0 {
            if let Ok(foreground) = native::foreground() {
                let input = crate::activity::snapshot();
                let tick = crate::activity::tick();
                let engaged = focus.active(foreground.window, input, tick, config.idle_seconds)
                    && foreground.idle < config.idle_seconds
                    && input.recent(tick, config.idle_seconds);
                if engaged {
                    learning_active = true;
                    active_id = observe_app_id(&apps, &foreground.app_id)
                        .or_else(|| observe_match(&apps, &foreground.process));
                }
            }
        } else {
            focus = crate::activity::FocusEvidence::default();
        }
        if config.learning_enabled && learning_active {
            learning_fraction += elapsed;
            let seconds = learning_fraction.floor() as u32;
            if seconds > 0 {
                learning_fraction -= f64::from(seconds);
                store.transaction(|| {
                    let mut progress: Progress = store.get("learning")?;
                    if progress.completed_at == 0 {
                        progress.observe(
                            now(),
                            &chrono::Local::now().date_naive().to_string(),
                            seconds.min(4),
                        );
                        progress.finish_if_ready(&config, now());
                        store.set("learning", &progress)?;
                    }
                    Ok(())
                })?;
            }
        } else {
            learning_fraction = 0.0;
        }
        if let Some((id, seconds, first)) = usage.sample(active_id.as_deref(), elapsed) {
            retry_locked_usage(|| {
                record_usage(&store, &mut session_times, &id, seconds, first, now())
            })?;
        }
        let _ = publish_heartbeat_if_due(&store, &mut last_heartbeat, Instant::now(), false)?;
        if maintenance.is_none_or(|at| at.elapsed() >= Duration::from_secs(60)) {
            maintenance = Some(Instant::now());
            if scan.is_none() && scanned.is_none_or(|at| at.elapsed() >= Duration::from_secs(21600))
            {
                scan = Some(scan_worker::start(&store.home));
            }
            if scan.is_none() {
                let result = (|| -> Result<()> {
                    reconcile_manual(&store)?;
                    automatic_review(&store, native::quiet_for_changes())?;
                    Ok(())
                })();
                let mut status = match result {
                    Ok(()) if config.collect => "Фоновый учёт работает".into(),
                    Ok(()) => "Учёт использования приостановлен".into(),
                    Err(e) => format!("Изменения приостановлены: {e:#}"),
                };
                for key in [
                    "autostart_warning",
                    "tray_warning",
                    "settings_shortcut_warning",
                ] {
                    let warning: String = store.get(key)?;
                    if !warning.is_empty() {
                        status.push_str(&format!("\n{warning}"));
                    }
                }
                store.set("status", &status)?;
            }
        }
        #[cfg(test)]
        if let Some(observer) = TEST_MONITOR_CATALOG.lock().unwrap().as_ref()
            && observer.home == store.home
        {
            let _ = observer.sender.send((
                scanned.is_some() && scan.is_none(),
                apps.iter().map(|app| app.id.clone()).collect(),
            ));
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn with_store(test: impl FnOnce(&Store)) {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let home = std::env::temp_dir().join(format!(
            "desktop-pilot-engine-test-{}-{}-{}",
            std::process::id(),
            now(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        let store = Store::open(&home).unwrap();
        let desktop = home.join("Desktop");
        std::fs::create_dir_all(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store
            .set(
                "config",
                &Config {
                    learning_enabled: false,
                    respect_taskbar_pins: false,
                    ..Default::default()
                },
            )
            .unwrap();
        test(&store);
        drop(store);
        std::fs::remove_dir_all(home).unwrap();
    }
    fn add_test_app(store: &Store, name: &str, target: &str) -> App {
        let source = store
            .home
            .join(format!("source-{}.url", store.apps().unwrap().len()));
        std::fs::write(&source, format!("[InternetShortcut]\r\nURL={target}\r\n")).unwrap();
        let mut app = native::read_link(&source).unwrap();
        app.name = name.into();
        app.policy = Policy::Always;
        store.put_app(&app).unwrap();
        app
    }
    fn replace_with_identical_file(path: &Path) {
        let replacement = path.with_extension("replacement");
        std::fs::copy(path, &replacement).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::rename(replacement, path).unwrap();
    }
    #[test]
    fn same_display_names_get_distinct_files_and_reversible_ownership() {
        with_store(|store| {
            add_test_app(store, "Same name", "steam://rungameid/101");
            add_test_app(store, "Same name", "steam://rungameid/102");
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            assert_eq!(state.owned.len(), 2);
            assert_ne!(norm(&state.owned[0].path), norm(&state.owned[1].path));
            for owned in &state.owned {
                let app = store.app(&owned.app).unwrap();
                assert_eq!(
                    std::fs::read(&owned.path).unwrap(),
                    std::fs::read(&app.source).unwrap()
                );
            }
            rollback(store).unwrap();
            assert!(
                state
                    .owned
                    .iter()
                    .all(|owned| !Path::new(&owned.path).exists())
            );
            assert!(
                store
                    .apps()
                    .unwrap()
                    .iter()
                    .all(|app| Path::new(&app.source).exists())
            );
        });
    }
    #[test]
    fn empty_apply_keeps_first_fill_available_when_evidence_arrives() {
        with_store(|store| {
            // This fixture tests user-applied recommendations in Observe mode.
            // Manual control intentionally never adds ranked Auto candidates.
            let mut cfg: Config = store.get("config").unwrap();
            cfg.collect = true;
            store.set("config", &cfg).unwrap();
            apply(store, false).unwrap();
            assert!(!store.get::<State>("state").unwrap().initialized);
            for (name, target) in [
                ("First", "steam://rungameid/501"),
                ("Second", "steam://rungameid/502"),
            ] {
                let mut app = add_test_app(store, name, target);
                app.policy = Policy::Auto;
                app.seed = 20.0;
                app.seed_at = now();
                store.put_app(&app).unwrap();
            }
            apply(store, false).unwrap();
            let state: State = store.get("state").unwrap();
            assert!(state.initialized);
            assert_eq!(state.owned.len(), 2);
        });
    }
    #[test]
    fn filenames_handle_devices_empty_names_and_multiple_existing_copies() {
        with_store(|store| {
            let desktop = root(store).unwrap();
            let mut reserved = HashSet::new();
            assert_eq!(
                shortcut_path(&desktop, "CON", "url", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "_CON.url"
            );
            assert_eq!(
                shortcut_path(&desktop, "...", "lnk", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "Shortcut.lnk"
            );
            assert_eq!(
                shortcut_path(&desktop, "Name", "url", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "Name.url"
            );
            assert_eq!(
                shortcut_path(&desktop, "Name", "url", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "Name · quick.url"
            );
            assert_eq!(
                shortcut_path(&desktop, "Name", "url", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "Name · quick (2).url"
            );
            assert_eq!(
                shortcut_path(&desktop, "LPT1.txt", "lnk", "en", &mut reserved)
                    .unwrap()
                    .file_name()
                    .unwrap(),
                "_LPT1.txt.lnk"
            );
        });
    }
    #[test]
    fn deleting_a_quick_copy_never_claims_the_byte_identical_original() {
        with_store(|store| {
            let mut app = add_test_app(store, "Original", "steam://rungameid/103");
            let original = root(store).unwrap().join("Original.url");
            std::fs::rename(&app.source, &original).unwrap();
            app.source = original.to_string_lossy().into_owned();
            store.put_app(&app).unwrap();
            let bytes = std::fs::read(&original).unwrap();
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            assert_ne!(Path::new(&state.owned[0].path), original);
            std::fs::remove_file(&state.owned[0].path).unwrap();
            reconcile_manual(store).unwrap();
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
            apply(store, false).unwrap();
            assert_eq!(std::fs::read(&original).unwrap(), bytes);
        });
    }
    #[test]
    fn renamed_file_keeps_its_identity_but_a_replacement_is_released() {
        with_store(|store| {
            let app = add_test_app(store, "Rename", "steam://rungameid/104");
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            let renamed = root(store).unwrap().join("Renamed by user.url");
            std::fs::rename(&before.owned[0].path, &renamed).unwrap();
            reconcile_manual(store).unwrap();
            let after: State = store.get("state").unwrap();
            assert_eq!(Path::new(&after.owned[0].path), renamed);
            assert_eq!(after.owned[0].file_id, before.owned[0].file_id);
            assert!(after.owned[0].manual);
            replace_with_identical_file(&renamed);
            reconcile_manual(store).unwrap();
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
            assert!(renamed.is_file());
        });
    }
    #[test]
    fn unfinished_transactions_are_not_interpreted_as_manual_deletions() {
        with_store(|store| {
            // This fixture exercises usage-guided placement in Observe mode.
            let mut cfg: Config = store.get("config").unwrap();
            cfg.collect = true;
            store.set("config", &cfg).unwrap();
            let mut app = add_test_app(store, "Prepared", "steam://rungameid/105");
            app.policy = Policy::Auto;
            app.seed = 20.0;
            app.seed_at = now();
            store.put_app(&app).unwrap();
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            store
                .db
                .execute("UPDATE transactions SET status='prepared'", [])
                .unwrap();
            std::fs::remove_file(&state.owned[0].path).unwrap();
            assert!(reconcile_manual(store).is_err());
            assert!(refresh_apps(store).is_err());
            assert_eq!(store.app(&app.id).unwrap().policy, Policy::Auto);
            rollback(store).unwrap();
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
        });
    }
    #[test]
    fn undo_preflight_preserves_all_files_if_one_file_was_edited() {
        with_store(|store| {
            add_test_app(store, "First", "steam://rungameid/106");
            add_test_app(store, "Second", "steam://rungameid/107");
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            let first = std::fs::read(&state.owned[0].path).unwrap();
            std::fs::write(&state.owned[1].path, b"User changes").unwrap();
            assert!(rollback(store).is_err());
            assert_eq!(std::fs::read(&state.owned[0].path).unwrap(), first);
            assert_eq!(
                std::fs::read(&state.owned[1].path).unwrap(),
                b"User changes"
            );
            assert_eq!(store.get::<State>("state").unwrap().owned.len(), 2);
        });
    }
    #[test]
    fn an_unreadable_original_does_not_remove_its_working_quick_copy() {
        with_store(|store| {
            let app = add_test_app(store, "Temporary update", "steam://rungameid/703");
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            let source = std::fs::read(&app.source).unwrap();
            // An installer or sync client can temporarily leave an incomplete
            // shortcut while replacing it. The published copy is still intact.
            std::fs::write(&app.source, b"[InternetShortcut]\r\n").unwrap();
            assert!(native::read_link(Path::new(&app.source)).is_err());
            apply(store, false).unwrap();
            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned.len(), 1);
            assert_eq!(after.owned[0].file_id, before.owned[0].file_id);
            assert_eq!(std::fs::read(&after.owned[0].path).unwrap(), source);
            assert!(store.app(&app.id).unwrap().valid);
            assert!(
                store.events().unwrap().iter().any(|(_, kind, message)| {
                    kind == "warning" && message.contains(&app.source)
                })
            );
            std::fs::write(&app.source, source).unwrap();
            refresh_apps(store).unwrap();
            assert!(store.app(&app.id).unwrap().valid);
        });
    }
    #[cfg(windows)]
    #[test]
    fn a_sharing_violation_on_the_original_keeps_the_managed_copy() {
        with_store(|store| {
            let app = add_test_app(store, "Locked original", "steam://rungameid/704");
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            let locked = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&app.source)
                .unwrap();
            assert!(native::read_link(Path::new(&app.source)).is_err());
            apply(store, false).unwrap();
            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned.len(), 1);
            assert_eq!(after.owned[0].file_id, before.owned[0].file_id);
            assert!(store.app(&app.id).unwrap().valid);
            assert!(!store.get::<String>("catalog_warning").unwrap().is_empty());
            drop(locked);
            refresh_apps(store).unwrap();
            assert!(store.get::<String>("catalog_warning").unwrap().is_empty());
        });
    }
    #[test]
    fn a_new_copy_requires_a_readable_current_original() {
        with_store(|store| {
            let app = add_test_app(store, "Incomplete original", "steam://rungameid/705");
            std::fs::write(&app.source, b"[InternetShortcut]\r\n").unwrap();
            assert!(apply(store, true).is_err());
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert_eq!(std::fs::read_dir(root(store).unwrap()).unwrap().count(), 0);
            assert_eq!(
                store
                    .db
                    .query_row("SELECT COUNT(*) FROM transactions", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
        });
    }
    #[test]
    fn brief_switches_after_long_use_and_restart_do_not_create_new_sessions() {
        with_store(|store| {
            let mut times = HashMap::new();
            let start = 10000;
            for index in 0..=1800 {
                record_usage(store, &mut times, "app", 2, index == 0, start + index * 2).unwrap();
            }
            assert_eq!(store.days().unwrap()[0].sessions, 1);
            // An hour of uninterrupted work, a brief Alt-Tab, and a newly
            // qualified return still belong to the same foreground session.
            record_usage(store, &mut times, "app", 10, true, start + 3612).unwrap();
            assert_eq!(store.days().unwrap()[0].sessions, 1);
            let mut resumed = store.get("app_session_times").unwrap();
            record_usage(store, &mut resumed, "app", 10, true, start + 4000).unwrap();
            assert_eq!(store.days().unwrap()[0].sessions, 1);
            // A full twenty minutes without confirmed app use permits a new one.
            record_usage(store, &mut resumed, "app", 10, true, start + 5200).unwrap();
            assert_eq!(store.days().unwrap()[0].sessions, 2);
            assert_eq!(
                store
                    .get::<HashMap<String, i64>>("app_session_times")
                    .unwrap()["app"],
                start + 5200
            );
        });
    }
    #[test]
    fn a_stopping_monitor_keeps_shutdown_signalled_until_the_process_exits() {
        with_store(|store| {
            // The process owns this mutex until after its monitor has joined.
            // Startup must not mistake the remaining process and old heartbeat
            // for a worker that is ready to accept new commands.
            let _process = native::exclusive("background", &store.home).unwrap();
            store.set("heartbeat", &now()).unwrap();
            store.set("shutdown", &true).unwrap();
            monitor(store.home.clone()).unwrap();
            assert!(store.get::<bool>("shutdown").unwrap());
        });
    }

    #[test]
    fn disconnected_source_recovers_without_replacing_the_owned_desktop_copy() {
        with_store(|store| {
            let app = add_test_app(store, "Offline source", "steam://rungameid/709");
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            let original = std::fs::read(&app.source).unwrap();
            std::fs::remove_file(&app.source).unwrap();

            apply(store, false).unwrap();
            let unavailable = store.app(&app.id).unwrap();
            assert!(!unavailable.valid);
            assert!(unavailable.unavailable_since > 0);
            assert_eq!(preview(store, false).unwrap().removed.len(), 0);
            let during: State = store.get("state").unwrap();
            assert_eq!(during.owned.len(), 1);
            assert_eq!(during.owned[0].file_id, before.owned[0].file_id);
            assert_eq!(std::fs::read(&during.owned[0].path).unwrap(), original);
            assert!(
                store
                    .get::<String>("catalog_warning")
                    .unwrap()
                    .contains("manually protected")
            );

            std::fs::write(&app.source, &original).unwrap();
            apply(store, false).unwrap();
            let restored = store.app(&app.id).unwrap();
            assert!(restored.valid);
            assert_eq!(restored.unavailable_since, 0);
            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned.len(), 1);
            assert_eq!(after.owned[0].file_id, before.owned[0].file_id);
            assert!(store.get::<String>("catalog_warning").unwrap().is_empty());
        });
    }

    #[test]
    fn disconnected_target_does_not_remove_the_originally_valid_quick_copy() {
        with_store(|store| {
            let target = store.home.join("external-target.exe");
            std::fs::write(&target, b"test executable placeholder").unwrap();
            let app = add_test_app(store, "External target", &target.to_string_lossy());
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            std::fs::remove_file(&target).unwrap();
            apply(store, false).unwrap();
            assert!(!store.app(&app.id).unwrap().valid);
            let during: State = store.get("state").unwrap();
            assert_eq!(during.owned.len(), 1);
            assert_eq!(during.owned[0].file_id, before.owned[0].file_id);
            std::fs::write(&target, b"target restored").unwrap();
            apply(store, false).unwrap();
            assert!(store.app(&app.id).unwrap().valid);
            assert_eq!(store.app(&app.id).unwrap().unavailable_since, 0);
            assert_eq!(
                store.get::<State>("state").unwrap().owned[0].file_id,
                before.owned[0].file_id
            );
        });
    }

    #[test]
    fn missing_and_changed_originals_are_still_invalidated() {
        with_store(|store| {
            // This fixture exercises usage-guided placement in Observe mode.
            let mut cfg: Config = store.get("config").unwrap();
            cfg.collect = true;
            store.set("config", &cfg).unwrap();
            let mut missing = add_test_app(store, "Missing", "steam://rungameid/706");
            // This fixture tests the 72-hour grace expiry for automatic
            // placement, not a shortcut the user explicitly pinned.
            missing.policy = Policy::Auto;
            missing.seed = 30.0;
            missing.seed_at = now();
            store.put_app(&missing).unwrap();
            let changed = add_test_app(store, "Changed", "steam://rungameid/707");
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            std::fs::remove_file(&missing.source).unwrap();
            std::fs::write(
                &changed.source,
                b"[InternetShortcut]\r\nURL=steam://rungameid/708\r\n",
            )
            .unwrap();
            apply(store, false).unwrap();
            assert!(!store.app(&missing.id).unwrap().valid);
            assert!(!store.app(&changed.id).unwrap().valid);
            let during: State = store.get("state").unwrap();
            assert_eq!(during.owned.len(), 1);
            assert_eq!(during.owned[0].app, missing.id);
            assert!(Path::new(&during.owned[0].path).exists());
            assert!(
                before
                    .owned
                    .iter()
                    .filter(|owned| owned.app == changed.id)
                    .all(|owned| !Path::new(&owned.path).exists())
            );
            assert!(Path::new(&changed.source).is_file());

            let mut expired = store.app(&missing.id).unwrap();
            expired.unavailable_since = now() - UNAVAILABLE_GRACE_SECONDS - 10;
            store.put_app(&expired).unwrap();
            apply(store, false).unwrap();
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert!(!Path::new(&during.owned[0].path).exists());
        });
    }
    #[test]
    fn prepared_create_collision_does_not_delete_an_identical_user_file() {
        with_store(|store| {
            add_test_app(store, "Collision", "steam://rungameid/108");
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            let path = Path::new(&state.owned[0].path);
            let bytes = std::fs::read(path).unwrap();
            replace_with_identical_file(path);
            store
                .db
                .execute("UPDATE transactions SET status='prepared'", [])
                .unwrap();
            rollback(store).unwrap();
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
        });
    }
    #[test]
    fn interrupted_undo_resumes_the_same_transaction_and_restored_file_identity() {
        with_store(|store| {
            let first = add_test_app(store, "First", "steam://rungameid/109");
            apply(store, true).unwrap();
            store.policy(&first.id, Policy::Never).unwrap();
            let second = add_test_app(store, "Second", "steam://rungameid/110");
            apply(store, false).unwrap();
            let id: String = store
                .db
                .query_row(
                    "SELECT id FROM transactions ORDER BY rowid DESC LIMIT 1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            rollback(store).unwrap();
            let restored: State = store.get("state").unwrap();
            assert_eq!(restored.owned.len(), 1);
            assert_eq!(restored.owned[0].app, first.id);
            assert_eq!(
                file_identity(Path::new(&restored.owned[0].path)).unwrap(),
                restored.owned[0].file_id
            );
            // Simulate the final DB commit being interrupted after file restoration.
            store
                .db
                .execute(
                    "UPDATE transactions SET status='rolling_back' WHERE id=?1",
                    [&id],
                )
                .unwrap();
            store
                .db
                .execute(
                    "UPDATE transactions SET at=?1 WHERE status='committed'",
                    [now() + 10000],
                )
                .unwrap();
            rollback(store).unwrap();
            let retried: State = store.get("state").unwrap();
            assert_eq!(retried.owned.len(), 1);
            assert_eq!(retried.owned[0].app, first.id);
            assert!(retried.owned.iter().all(|owned| owned.app != second.id));
            assert!(Path::new(&retried.owned[0].path).is_file());
        });
    }
    #[test]
    fn consecutive_undo_can_remove_a_copy_restored_by_the_previous_undo() {
        with_store(|store| {
            let first = add_test_app(store, "First", "steam://rungameid/601");
            apply(store, true).unwrap();
            let initial: State = store.get("state").unwrap();
            let path = initial.owned[0].path.clone();
            // Keep the old inode/file record alive so a filesystem cannot reuse
            // its identifier for the newly restored file and hide the regression.
            std::fs::hard_link(&path, store.home.join("old-copy-identity")).unwrap();
            store.policy(&first.id, Policy::Never).unwrap();
            add_test_app(store, "Second", "steam://rungameid/602");
            apply(store, false).unwrap();
            rollback(store).unwrap();
            assert!(Path::new(&path).is_file());
            assert_ne!(
                file_identity(Path::new(&path)).unwrap(),
                initial.owned[0].file_id
            );
            rollback(store).unwrap();
            assert!(!Path::new(&path).exists());
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert!(Path::new(&first.source).is_file());
        });
    }
    #[test]
    fn undo_follows_commit_order_after_the_system_clock_moves_backwards() {
        with_store(|store| {
            let first = add_test_app(store, "First", "steam://rungameid/701");
            apply(store, true).unwrap();
            let first_state: State = store.get("state").unwrap();
            store.policy(&first.id, Policy::Never).unwrap();
            add_test_app(store, "Second", "steam://rungameid/702");
            apply(store, false).unwrap();
            let second_state: State = store.get("state").unwrap();
            // Actual insertion order survives clock correction; wall timestamps do not.
            store
                .db
                .execute("UPDATE transactions SET at=10000", [])
                .unwrap();
            store.db.execute(
                "UPDATE transactions SET at=9000 WHERE rowid=(SELECT MAX(rowid) FROM transactions)",
                [],
            ).unwrap();
            rollback(store).unwrap();
            let restored: State = store.get("state").unwrap();
            assert_eq!(restored.owned.len(), 1);
            assert_eq!(restored.owned[0].app, first.id);
            assert!(Path::new(&first_state.owned[0].path).is_file());
            assert!(!Path::new(&second_state.owned[0].path).exists());
            rollback(store).unwrap();
            assert!(store.get::<State>("state").unwrap().owned.is_empty());
            assert!(!Path::new(&first_state.owned[0].path).exists());
        });
    }
    #[test]
    fn consecutive_undo_still_preserves_a_user_replacement_of_a_restored_copy() {
        with_store(|store| {
            let first = add_test_app(store, "First", "steam://rungameid/603");
            apply(store, true).unwrap();
            let initial: State = store.get("state").unwrap();
            let path = Path::new(&initial.owned[0].path);
            std::fs::hard_link(path, store.home.join("old-copy-identity")).unwrap();
            store.policy(&first.id, Policy::Never).unwrap();
            add_test_app(store, "Second", "steam://rungameid/604");
            apply(store, false).unwrap();
            rollback(store).unwrap();
            let restored_id = file_identity(path).unwrap();
            let bytes = std::fs::read(path).unwrap();
            replace_with_identical_file(path);
            assert_ne!(file_identity(path).unwrap(), restored_id);
            assert!(rollback(store).is_err());
            assert_eq!(std::fs::read(path).unwrap(), bytes);
            assert!(Path::new(&first.source).is_file());
        });
    }
    #[test]
    fn atomic_copy_refuses_to_overwrite_existing_files() {
        with_store(|store| {
            let source = store.home.join("source");
            let destination = store.home.join("destination");
            std::fs::write(&source, b"New").unwrap();
            std::fs::write(&destination, b"User file").unwrap();
            assert!(copy_new(&source, &destination).is_err());
            assert_eq!(std::fs::read(&destination).unwrap(), b"User file");
            assert!(
                !std::fs::read_dir(&store.home)
                    .unwrap()
                    .filter_map(Result::ok)
                    .any(|entry| entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".desktop-pilot-"))
            );
        });
    }
    #[cfg(windows)]
    #[test]
    fn temporary_desktop_files_are_hidden_and_published_shortcuts_are_visible() {
        with_store(|store| {
            let source = store.home.join("source.url");
            std::fs::write(
                &source,
                b"[InternetShortcut]\r\nURL=steam://rungameid/10\r\n",
            )
            .unwrap();
            let staging = root(store).unwrap().join(".desktop-pilot-visibility.tmp");
            copy_new(&source, &staging).unwrap();
            assert_ne!(
                std::fs::metadata(&staging).unwrap().file_attributes() & 0x2,
                0
            );
            let published = root(store).unwrap().join("Visible.url");
            move_new(&staging, &published).unwrap();
            assert_eq!(
                std::fs::metadata(&published).unwrap().file_attributes() & 0x2,
                0
            );
            assert_eq!(
                std::fs::read(published).unwrap(),
                std::fs::read(source).unwrap()
            );
        });
    }
    #[test]
    fn failed_statistics_clear_keeps_the_existing_usage_history() {
        with_store(|store| {
            store.observe("broken", 30, true, false).unwrap();
            store
                .db
                .execute("INSERT INTO apps VALUES('broken','{broken')", [])
                .unwrap();
            assert!(command(store, "clear_stats").is_err());
            assert_eq!(store.days().unwrap()[0].seconds, 30);
        });
    }
    #[test]
    fn url_catalog_refresh_preserves_legacy_ids_and_case_sensitive_targets() {
        with_store(|store| {
            let source = root(store).unwrap().join("Old.url");
            std::fs::write(
                &source,
                b"[InternetShortcut]\r\nURL=https://example.com/Path?Key=Value\r\n",
            )
            .unwrap();
            let mut app = native::read_link(&source).unwrap();
            app.id = hash(format!("{}\\n\\n", norm(&app.target)).as_bytes())[..20].to_owned();
            app.policy = Policy::Always;
            app.seed = 20.0;
            app.seed_at = now();
            store.put_app(&app).unwrap();
            refresh_apps(store).unwrap();
            assert_eq!(store.apps().unwrap().len(), 1);
            assert_eq!(store.app(&app.id).unwrap().policy, Policy::Always);
            std::fs::write(
                root(store).unwrap().join("Other.url"),
                b"[InternetShortcut]\r\nURL=https://example.com/path?Key=Value\r\n",
            )
            .unwrap();
            refresh_apps(store).unwrap();
            assert_eq!(store.apps().unwrap().len(), 2);
            assert_eq!(store.app(&app.id).unwrap().seed, 20.0);
        });
    }
    #[test]
    fn protocol_catalog_refresh_preserves_legacy_rules_without_merging_other_parameters() {
        with_store(|store| {
            let source = root(store).unwrap().join("Research.url");
            std::fs::write(
                &source,
                b"[InternetShortcut]\r\nURL=obsidian://open?vault=Research&file=Topic\r\n",
            )
            .unwrap();
            let mut app = native::read_link(&source).unwrap();
            app.id = hash(format!("{}\n\n", norm(&app.target)).as_bytes())[..20].to_owned();
            app.policy = Policy::Never;
            store.put_app(&app).unwrap();
            std::fs::write(
                root(store).unwrap().join("Other vault.url"),
                b"[InternetShortcut]\r\nURL=obsidian://open?vault=research&file=topic\r\n",
            )
            .unwrap();
            refresh_apps(store).unwrap();
            assert_eq!(store.apps().unwrap().len(), 2);
            let legacy = store.app(&app.id).unwrap();
            assert_eq!(legacy.policy, Policy::Never);
            assert_eq!(legacy.target, app.target);
            assert!(
                store
                    .apps()
                    .unwrap()
                    .iter()
                    .any(|other| other.id != app.id && other.policy == Policy::Auto)
            );
        });
    }
    #[test]
    fn manually_pinned_rare_shortcut_survives_long_absence_then_obeys_release() {
        with_store(|store| {
            let mut config: Config = store.get("config").unwrap();
            config.collect = true;
            config.slots = 1;
            store.set("config", &config).unwrap();

            let rare = add_test_app(store, "Rare critical tool", "steam://rungameid/8401");
            let mut frequent = add_test_app(store, "Daily project", "steam://rungameid/8402");
            frequent.policy = Policy::Auto;
            frequent.seed = 300.0;
            frequent.seed_at = now();
            store.put_app(&frequent).unwrap();

            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            assert_eq!(before.owned.len(), 1);
            assert_eq!(before.owned[0].app, rare.id);
            let protected_path = before.owned[0].path.clone();
            let protected_bytes = std::fs::read(&protected_path).unwrap();
            let original_bytes = std::fs::read(&rare.source).unwrap();

            std::fs::remove_file(&rare.source).unwrap();
            refresh_apps(store).unwrap();
            // Replay successive reviews across weekends and an extended absence.
            // Only the outage clock is advanced; selection, copying and SQLite
            // persistence are the actual application implementations.
            for days_away in [1, 2, 3, 6, 21, 45] {
                let mut missing = store.app(&rare.id).unwrap();
                assert!(!missing.valid);
                missing.unavailable_since = now() - days_away * 86400;
                store.put_app(&missing).unwrap();
                assert_eq!(
                    preview(store, false).unwrap().selected.as_slice(),
                    std::slice::from_ref(&rare.id)
                );
                apply(store, false).unwrap();
                let warning: String = store.get("catalog_warning").unwrap();
                assert!(warning.contains("manually protected"));
                let retained: State = store.get("state").unwrap();
                assert_eq!(retained.owned.len(), 1);
                assert_eq!(retained.owned[0].path, protected_path);
                assert_eq!(retained.owned[0].position, before.owned[0].position);
                assert_eq!(retained.owned[0].file_id, before.owned[0].file_id);
                assert!(retained.owned[0].manual);
                assert_eq!(std::fs::read(&protected_path).unwrap(), protected_bytes);
            }

            std::fs::write(&rare.source, &original_bytes).unwrap();
            refresh_apps(store).unwrap();
            assert!(store.app(&rare.id).unwrap().valid);
            apply(store, false).unwrap();
            assert_eq!(std::fs::read(&protected_path).unwrap(), protected_bytes);

            command(store, &format!("policy:{}:Auto", rare.id)).unwrap();
            assert!(!store.get::<State>("state").unwrap().owned[0].manual);
            assert_eq!(
                preview(store, false).unwrap().selected.as_slice(),
                std::slice::from_ref(&frequent.id)
            );
            apply(store, false).unwrap();
            let released: State = store.get("state").unwrap();
            assert_eq!(released.owned.len(), 1);
            assert_eq!(released.owned[0].app, frequent.id);
            assert!(!Path::new(&protected_path).exists());
            assert_eq!(std::fs::read(&rare.source).unwrap(), original_bytes);
        });
    }

    #[test]
    fn reducing_slots_removes_an_unavailable_copy_not_a_working_one() {
        with_store(|store| {
            let mut config: Config = store.get("config").unwrap();
            config.collect = true;
            config.slots = 2;
            config.max_changes = 1;
            store.set("config", &config).unwrap();
            let mut offline = add_test_app(store, "Offline game", "steam://rungameid/601");
            offline.policy = Policy::Auto;
            offline.seed = 200.0;
            offline.seed_at = now();
            store.put_app(&offline).unwrap();
            let mut online = add_test_app(store, "Available editor", "steam://rungameid/602");
            online.policy = Policy::Auto;
            online.seed = 20.0;
            online.seed_at = now();
            store.put_app(&online).unwrap();
            apply(store, true).unwrap();
            let before: State = store.get("state").unwrap();
            assert_eq!(before.owned.len(), 2);
            let unavailable_copy = before.owned.iter().find(|o| o.app == offline.id).unwrap();
            let available_copy = before.owned.iter().find(|o| o.app == online.id).unwrap();
            let unavailable_path = unavailable_copy.path.clone();
            let available_path = available_copy.path.clone();
            let available_sha = available_copy.sha.clone();

            // A missing source enters the temporary grace period and keeps its
            // existing place until the user explicitly reduces the capacity.
            std::fs::remove_file(&offline.source).unwrap();
            refresh_apps(store).unwrap();
            assert!(!store.app(&offline.id).unwrap().valid);
            assert_eq!(preview(store, false).unwrap().selected.len(), 2);

            config.slots = 1;
            store.set("config", &config).unwrap();
            assert_eq!(preview(store, false).unwrap().selected, [online.id.clone()]);
            apply(store, false).unwrap();

            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned.len(), 1);
            assert_eq!(after.owned[0].app, online.id);
            assert_eq!(after.owned[0].path, available_path);
            assert_eq!(after.owned[0].sha, available_sha);
            assert!(Path::new(&available_path).is_file());
            assert!(!Path::new(&unavailable_path).exists());
            assert!(Path::new(&online.source).is_file());
        });
    }

    #[test]
    fn failed_usage_commit_does_not_advance_in_memory_session_history() {
        with_store(|store| {
            store
                .set("app_session_times", &HashMap::<String, i64>::new())
                .unwrap();
            store
                .db
                .execute_batch(
                    "CREATE TRIGGER reject_session_write BEFORE UPDATE ON kv
                     WHEN NEW.k = 'app_session_times'
                     BEGIN SELECT RAISE(ABORT, 'simulated storage failure'); END;",
                )
                .unwrap();
            let mut session_times = HashMap::new();
            let at = now();
            let mut attempts = 0;
            assert!(
                retry_locked_usage(|| {
                    attempts += 1;
                    record_usage(store, &mut session_times, "editor", 10, true, at)
                })
                .is_err()
            );
            assert_eq!(attempts, 1, "constraint violations must not be retried");
            assert!(session_times.is_empty());
            assert!(store.days().unwrap().is_empty());

            store
                .db
                .execute_batch("DROP TRIGGER reject_session_write;")
                .unwrap();
            record_usage(store, &mut session_times, "editor", 10, true, at).unwrap();
            let days = store.days().unwrap();
            assert_eq!(days.len(), 1);
            assert_eq!(days[0].seconds, 10);
            assert_eq!(days[0].sessions, 1);
            assert_eq!(session_times["editor"], at);
        });
    }

    #[test]
    fn offline_clock_rollback_preview_matches_safe_apply_and_manual_release() {
        with_store(|store| {
            let mut cfg: Config = store.get("config").unwrap();
            cfg.collect = true;
            cfg.slots = 1;
            store.set("config", &cfg).unwrap();

            let rare = add_test_app(store, "Rare critical tool", "steam://rungameid/8051");
            let mut frequent = add_test_app(store, "Frequent tool", "steam://rungameid/8052");
            frequent.policy = Policy::Auto;
            frequent.seed = 300.0;
            frequent.seed_at = now();
            store.put_app(&frequent).unwrap();
            apply(store, true).unwrap();

            let before: State = store.get("state").unwrap();
            assert_eq!(before.owned.len(), 1);
            assert_eq!(before.owned[0].app, rare.id);
            let path = before.owned[0].path.clone();
            let bytes = std::fs::read(&path).unwrap();

            std::fs::remove_file(&rare.source).unwrap();
            refresh_apps(store).unwrap();
            let mut unavailable = store.app(&rare.id).unwrap();
            assert!(!unavailable.valid);
            // Reopen Settings before its next catalog scan after correcting
            // a clock that was 14 days fast while the source was offline.
            unavailable.unavailable_since = now() + 14 * 86400;
            store.put_app(&unavailable).unwrap();

            let proposal = preview(store, false).unwrap();
            assert_eq!(proposal.selected.as_slice(), std::slice::from_ref(&rare.id));
            assert!(proposal.removed.is_empty());
            assert!(proposal.added.is_empty());

            // Applying performs its normal scan and preserves the identical
            // managed file. No original is recreated while unavailable.
            apply(store, false).unwrap();
            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned[0].path, path);
            assert_eq!(after.owned[0].file_id, before.owned[0].file_id);
            assert_eq!(after.owned[0].position, before.owned[0].position);
            assert_eq!(std::fs::read(&path).unwrap(), bytes);

            // A deliberate Never choice still releases the shortcut.
            command(store, &format!("policy:{}:Never", rare.id)).unwrap();
            let release = preview(store, false).unwrap();
            assert_eq!(
                release.selected.as_slice(),
                std::slice::from_ref(&frequent.id)
            );
            apply(store, false).unwrap();
            assert!(!Path::new(&path).exists());
            assert_eq!(
                store.get::<State>("state").unwrap().owned[0].app,
                frequent.id
            );
        });
    }

    #[test]
    fn a_future_review_checkpoint_does_not_freeze_automatic_recovery() {
        with_store(|store| {
            let mut cfg: Config = store.get("config").unwrap();
            cfg.automatic = true;
            cfg.interval_days = 7;
            store.set("config", &cfg).unwrap();
            let baseline = now();
            store
                .set(
                    "state",
                    &State {
                        initialized: true,
                        last_review: baseline + 14 * 86400,
                        ..Default::default()
                    },
                )
                .unwrap();

            // A corrected clock must permit exactly one normal quiet review.
            assert!(automatic_review(store, true).unwrap());
            let after: State = store.get("state").unwrap();
            assert!(after.last_review >= baseline && after.last_review <= now());
            assert!(after.owned.is_empty());
            assert!(!automatic_review(store, true).unwrap());

            // Small clock adjustments and a busy desktop are not a reason
            // to override the requested seven-day review interval.
            let mut future = after.clone();
            future.last_review = now() + 30;
            store.set("state", &future).unwrap();
            assert!(!automatic_review(store, true).unwrap());

            future.last_review = now() + 14 * 86400;
            store.set("state", &future).unwrap();
            assert!(!automatic_review(store, false).unwrap());
        });
    }

    #[test]
    fn corrected_backwards_clock_restores_new_usage_sessions_and_checkpoints() {
        with_store(|store| {
            let corrected_time = now();
            let checkpoint = HashMap::from([("editor".to_owned(), corrected_time + 2 * 3600)]);
            store.set("app_session_times", &checkpoint).unwrap();

            // A clock set two hours into the future was corrected while
            // Desktop Pilot was stopped. Restart from its real persisted state.
            let mut sessions: HashMap<String, i64> = store.get("app_session_times").unwrap();
            record_usage(store, &mut sessions, "editor", 10, true, corrected_time).unwrap();
            let history = store.days().unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].seconds, 10);
            assert_eq!(
                history[0].sessions, 1,
                "the first confirmed visit after correcting a large clock error must not be lost"
            );
            assert_eq!(sessions["editor"], corrected_time);
            let persisted: HashMap<String, i64> = store.get("app_session_times").unwrap();
            assert_eq!(persisted, sessions);

            // A worker restart immediately after the correction must not
            // manufacture a second session inside the ordinary session gap.
            record_usage(store, &mut sessions, "editor", 5, true, corrected_time + 30).unwrap();
            assert_eq!(store.days().unwrap()[0].sessions, 1);
            record_usage(
                store,
                &mut sessions,
                "editor",
                10,
                true,
                corrected_time + 1230,
            )
            .unwrap();
            let history = store.days().unwrap();
            assert_eq!(history[0].seconds, 25);
            assert_eq!(history[0].sessions, 2);
        });
    }

    #[test]
    fn small_backwards_clock_adjustments_do_not_inflate_short_sessions() {
        with_store(|store| {
            let corrected_time = now();
            let mut sessions = HashMap::from([("editor".to_owned(), corrected_time + 30)]);
            record_usage(store, &mut sessions, "editor", 10, true, corrected_time).unwrap();
            assert_eq!(store.days().unwrap()[0].sessions, 0);
            assert_eq!(sessions["editor"], corrected_time);
        });
    }

    #[test]
    fn corrected_clock_review_preserves_manual_shortcut_and_requires_user_permission() {
        with_store(|store| {
            add_test_app(store, "Hand picked", "steam://rungameid/523");
            apply(store, true).unwrap();
            let original: State = store.get("state").unwrap();
            assert_eq!(original.owned.len(), 1);
            let shortcut = original.owned[0].clone();
            let bytes = std::fs::read(&shortcut.path).unwrap();

            let mut config: Config = store.get("config").unwrap();
            config.automatic = false;
            store.set("config", &config).unwrap();

            let future = now() + 21 * 86400;
            let mut pending = original.clone();
            pending.last_review = future;
            store.set("state", &pending).unwrap();
            assert!(!automatic_review(store, true).unwrap());
            assert_eq!(store.get::<State>("state").unwrap().last_review, future);

            config.automatic = true;
            store.set("config", &config).unwrap();
            assert!(!automatic_review(store, false).unwrap());
            assert_eq!(store.get::<State>("state").unwrap().last_review, future);

            assert!(automatic_review(store, true).unwrap());
            let after: State = store.get("state").unwrap();
            assert_eq!(after.owned.len(), 1);
            assert_eq!(after.owned[0].path, shortcut.path);
            assert_eq!(after.owned[0].file_id, shortcut.file_id);
            assert_eq!(std::fs::read(&shortcut.path).unwrap(), bytes);
            assert!(!automatic_review(store, true).unwrap());
        });
    }

    #[test]
    fn sqlite_writer_lock_retries_the_real_usage_transaction_once() {
        with_store(|store| {
            // A second connection owns SQLite's writer lock. The first real
            // record_usage transaction must fail, then the lock is released.
            store.db.busy_timeout(Duration::from_millis(1)).unwrap();
            let blocker = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
            blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
            let mut blocker = Some(blocker);
            let mut attempts = 0;
            let mut sessions = HashMap::new();
            let at = now();
            retry_locked_usage(|| {
                attempts += 1;
                let attempt = record_usage(store, &mut sessions, "editor", 10, true, at);
                if attempts == 1 {
                    assert!(attempt.is_err());
                    blocker.take().unwrap().execute_batch("ROLLBACK;").unwrap();
                }
                attempt
            })
            .unwrap();
            assert_eq!(attempts, 2);
            let history = store.days().unwrap();
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].seconds, 10);
            assert_eq!(history[0].sessions, 1);
            assert_eq!(sessions["editor"], at);
        });
    }

    #[test]
    fn persistent_sqlite_lock_stops_after_three_attempts_without_partial_usage() {
        with_store(|store| {
            store.db.busy_timeout(Duration::from_millis(1)).unwrap();
            let blocker = rusqlite::Connection::open(store.home.join("usage.sqlite")).unwrap();
            blocker.execute_batch("BEGIN IMMEDIATE;").unwrap();
            let mut attempts = 0;
            let mut sessions = HashMap::new();
            let at = now();
            let failure = retry_locked_usage(|| {
                attempts += 1;
                record_usage(store, &mut sessions, "editor", 10, true, at)
            });
            assert!(failure.is_err());
            assert_eq!(attempts, 3);
            assert!(sessions.is_empty());
            assert!(store.days().unwrap().is_empty());
            blocker.execute_batch("ROLLBACK;").unwrap();
            retry_locked_usage(|| record_usage(store, &mut sessions, "editor", 10, true, at))
                .unwrap();
            assert_eq!(store.days().unwrap()[0].seconds, 10);
            assert_eq!(store.days().unwrap()[0].sessions, 1);
        });
    }

    #[test]
    fn foreground_accumulator_uses_actual_elapsed_time_and_discards_sleep() {
        let mut usage = UsageAccumulator::default();
        assert!(usage.sample(Some("app"), 2.0).is_none());
        for _ in 0..3 {
            assert!(usage.sample(Some("app"), 2.6).is_none());
        }
        assert_eq!(
            usage.sample(Some("app"), 2.6),
            Some(("app".into(), 10, true))
        );
        assert_eq!(
            usage.sample(Some("app"), 2.6),
            Some(("app".into(), 3, false))
        );
        assert!(usage.sample(Some("app"), 600.0).is_none());
        assert!(usage.sample(Some("app"), 2.0).is_none());
        assert!(usage.sample(Some("other"), 2.0).is_none());
        assert!(usage.sample(None, 2.0).is_none());
    }
    #[test]
    fn repeated_short_foreground_visits_accumulate_in_the_real_usage_store() {
        with_store(|store| {
            let mut usage = UsageAccumulator::default();
            let mut session_times = HashMap::new();
            let mut firsts = HashMap::<String, u32>::new();
            let mut timestamp = 10_000_i64;
            // Each visit is six seconds; before this fix all were discarded.
            for _ in 0..3 {
                for app in ["editor", "browser"] {
                    for _ in 0..3 {
                        timestamp += 2;
                        if let Some((id, seconds, first)) = usage.sample(Some(app), 2.0) {
                            *firsts.entry(id.clone()).or_default() += u32::from(first);
                            record_usage(store, &mut session_times, &id, seconds, first, timestamp)
                                .unwrap();
                        }
                    }
                }
            }
            let history = store.days().unwrap();
            assert_eq!(history.len(), 2);
            for app in ["editor", "browser"] {
                let day = history.iter().find(|day| day.app == app).unwrap();
                assert_eq!(day.seconds, 12);
                assert_eq!(day.sessions, 1);
                assert_eq!(firsts[app], 1);
            }
        });
    }

    #[test]
    fn incidental_switches_and_expired_short_visits_do_not_create_usage() {
        let mut usage = UsageAccumulator::default();
        // Alternating isolated ticks never confirms sustained engagement.
        for _ in 0..50 {
            assert!(usage.sample(Some("editor"), 1.0).is_none());
            assert!(usage.sample(Some("browser"), 1.0).is_none());
        }
        for _ in 0..3 {
            assert!(usage.sample(Some("editor"), 2.0).is_none());
        }
        // A period without confirmed foreground use expires incomplete visits.
        for _ in 0..601 {
            assert!(usage.sample(None, 2.0).is_none());
        }
        for _ in 0..3 {
            assert!(usage.sample(Some("editor"), 2.0).is_none());
        }
        assert!(usage.sample(Some("editor"), 600.0).is_none());
        for _ in 0..3 {
            assert!(usage.sample(Some("editor"), 2.0).is_none());
        }
    }

    #[test]
    fn rapid_switches_keep_short_visit_memory_bounded() {
        let mut usage = UsageAccumulator::default();
        for index in 0..300 {
            let app = format!("app-{index}");
            assert!(usage.sample(Some(&app), 1.0).is_none());
            assert!(usage.sample(Some(&app), 1.0).is_none());
            assert!(usage.windows.len() <= MAX_SHORT_VISIT_APPS);
        }
    }
    #[test]
    fn valid_shell_shortcuts_can_be_learned_without_overriding_manual_rules() {
        with_store(|store| {
            // This fixture exercises usage-guided placement in Observe mode.
            let mut cfg: Config = store.get("config").unwrap();
            cfg.collect = true;
            store.set("config", &cfg).unwrap();
            let desktop: String = store.get("test_desktop").unwrap();
            let source = Path::new(&desktop).join("Packaged App.url");
            std::fs::write(
                &source,
                "[InternetShortcut]\r\nURL=shell:AppsFolder\\Publisher.App!Main\r\n",
            )
            .unwrap();

            refresh_apps(store).unwrap();
            let apps = store.apps().unwrap();
            assert_eq!(apps.len(), 1);
            let app = &apps[0];
            assert!(app.valid);
            assert_eq!(app.policy, Policy::Auto);
            assert!(app.note.is_empty());
            assert_eq!(
                observe_app_id(&apps, "Publisher.App!Main"),
                Some(app.id.clone())
            );

            store.observe(&app.id, 600, true, false).unwrap();
            let first = preview(store, true).unwrap();
            assert_eq!(first.selected.len(), 1);
            assert_eq!(first.selected[0], app.id);
            apply(store, true).unwrap();
            let state: State = store.get("state").unwrap();
            assert_eq!(state.owned.len(), 1);
            assert_eq!(
                std::fs::read(&state.owned[0].path).unwrap(),
                std::fs::read(&source).unwrap()
            );

            store.policy(&app.id, Policy::Never).unwrap();
            refresh_apps(store).unwrap();
            assert_eq!(store.app(&app.id).unwrap().policy, Policy::Never);
            assert!(preview(store, false).unwrap().selected.is_empty());
        });
    }

    #[test]
    fn appsfolder_targets_can_match_without_explorer_arguments() {
        let app = App {
            id: "packaged".into(),
            target: "shell:AppsFolder\\Example.Package!App".into(),
            valid: true,
            ..Default::default()
        };
        assert_eq!(
            observe_app_id(&[app], "example.package!app"),
            Some("packaged".into())
        );
    }
    #[test]
    fn ambiguous_profiles_not_merged() {
        let mut a = App {
            target: "C:\\browser.exe".into(),
            arguments: "--profile=A".into(),
            valid: true,
            ..Default::default()
        };
        a.assign_id();
        let mut b = a.clone();
        b.arguments = "--profile=B".into();
        b.assign_id();
        assert!(observe_match(&[a, b], "c:\\browser.exe").is_none());
    }
    #[test]
    fn unknown_files_do_not_fit_zone() {
        let l = native::Layout {
            items: vec![native::Item {
                path: "existing".into(),
                name: "existing".into(),
                position: Point { x: 22, y: 2 },
            }],
            spacing: Point { x: 76, y: 84 },
            origin: Point::default(),
            work: [0, 0, 1920, 1032],
            flags: 4,
            topology: String::new(),
            icon_size: 32,
        };
        let c = Config {
            slots: 1,
            zone_col: 0,
            zone_row: 0,
            ..Default::default()
        };
        assert!(reserve_positions(&l, &c, &State::default(), &[], 1).is_err());
    }
    #[test]
    fn traversal_is_rejected() {
        assert!(safe_file(Path::new(r"C:\elsewhere\x.lnk"), Path::new(r"C:\Desktop")).is_err());
    }
    #[test]
    fn steam_process_is_matched_only_inside_its_game_folder() {
        let a = App {
            id: "game".into(),
            target: "steam://rungameid/1".into(),
            valid: true,
            observed_roots: vec![r"D:\Steam\common\Game".into()],
            ..Default::default()
        };
        assert_eq!(
            observe_match(
                std::slice::from_ref(&a),
                r"d:\steam\common\game\bin\game.exe"
            ),
            Some("game".into())
        );
        assert_eq!(observe_match(&[a], r"D:\Steam\common\Game2\game.exe"), None);
    }
    #[test]
    fn store_app_uses_identity_not_explorer_process() {
        let app = App {
            id: "music".into(),
            arguments: "\"shell:AppsFolder\\Yandex.Music_123!App\"".into(),
            valid: true,
            ..Default::default()
        };
        assert_eq!(
            observe_app_id(std::slice::from_ref(&app), "yandex.music_123!app"),
            Some("music".into())
        );
        assert_eq!(observe_app_id(&[app], "other!app"), None);
    }
    #[test]
    fn display_geometry_matrix() {
        for (width, height) in [
            (1024, 768),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
        ] {
            for scale in [100, 125, 150, 200, 300] {
                for (left, top) in [(0, 0), (1920, 0), (3840, 1080), (-1920, -1080)] {
                    let layout = native::Layout {
                        items: vec![],
                        spacing: Point {
                            x: 76 * scale / 100,
                            y: 84 * scale / 100,
                        },
                        origin: Point { x: -left, y: -top },
                        work: [left, top, left + width, top + height - 48 * scale / 100],
                        flags: 4,
                        topology: String::new(),
                        icon_size: 32 * scale / 100,
                    };
                    let (col, row, columns) = suggest_zone(&layout, 4).unwrap();
                    let cfg = Config {
                        slots: 4,
                        zone_col: col,
                        zone_row: row,
                        zone_columns: columns,
                        ..Default::default()
                    };
                    let points =
                        reserve_positions(&layout, &cfg, &State::default(), &[], 4).unwrap();
                    assert_eq!(points.len(), 4);
                    for p in &points {
                        assert!(
                            p.x >= left && p.y >= top && p.x < left + width && p.y < top + height
                        );
                    }
                    for (i, p) in points.iter().enumerate() {
                        for q in &points[i + 1..] {
                            assert!(
                                (p.x - q.x).abs() >= layout.spacing.x
                                    || (p.y - q.y).abs() >= layout.spacing.y
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn occupied_and_too_small_desktops_are_rejected() {
        let mut layout = native::Layout {
            items: vec![],
            spacing: Point { x: 76, y: 84 },
            origin: Point::default(),
            work: [0, 0, 152, 168],
            flags: 4,
            topology: String::new(),
            icon_size: 32,
        };
        for x in [22, 98] {
            for y in [2, 86] {
                layout.items.push(native::Item {
                    name: "existing".into(),
                    path: format!("item-{x}-{y}"),
                    position: Point { x, y },
                });
            }
        }
        assert!(suggest_zone(&layout, 1).is_err());
        layout.items.clear();
        layout.work = [0, 0, 40, 40];
        assert!(suggest_zone(&layout, 1).is_err());
    }
}

#[cfg(test)]
mod alias_recovery_tests;

#[cfg(test)]
mod session_retention_tests;

#[cfg(test)]
mod heartbeat_tests;

#[cfg(test)]
mod source_metadata_tests;

mod import_guard;
pub(crate) use import_guard::imported_quick_copy;

#[cfg(test)]
mod manual_mode_tests;

#[cfg(test)]
mod catalog_race_tests;

#[cfg(test)]
mod renamed_copy_scan_tests;

#[cfg(test)]
mod blocked_scan_tests;

mod scan_worker;

#[cfg(test)]
mod live_import_tests;

#[cfg(test)]
mod profile_attribution_tests;

#[cfg(test)]
mod identity_index_tests;

#[cfg(test)]
mod discovery_limit_tests;

#[cfg(test)]
mod idle_review_tests;
