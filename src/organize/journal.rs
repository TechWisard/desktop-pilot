use super::*;
use anyhow::bail;
use std::collections::{HashMap, HashSet};

fn checked_root(journal: &Journal, store: &Store) -> Result<std::fs::File> {
    ensure!(
        norm(&root(store)?.to_string_lossy()) == norm(&journal.root),
        "The desktop location changed. Restore its original location before recovery."
    );
    let handle = fs::directory(Path::new(&journal.root))?;
    ensure!(
        fs::identity(&handle)? == journal.root_id,
        "The desktop directory was replaced; no objects were moved"
    );
    Ok(handle)
}

fn checked_folder(root: &Path, folder: &Folder) -> Result<std::fs::File> {
    fs::leaf(&folder.name)?;
    let handle = fs::directory(&root.join(&folder.name))?;
    ensure!(
        folder.id.as_ref() == Some(&fs::identity(&handle)?),
        "The category directory changed; it was preserved"
    );
    Ok(handle)
}

fn update_catalog(store: &Store, journal: &Journal, undo: bool) -> Result<()> {
    let mut apps = store.apps()?;
    let mut sources = HashMap::<String, Vec<usize>>::new();
    for (index, app) in apps.iter().enumerate() {
        sources.entry(norm(&app.source)).or_default().push(index);
    }
    for movement in &journal.movements {
        let original = Path::new(&journal.root).join(&movement.item.name);
        let organized = Path::new(&journal.root)
            .join(&movement.folder)
            .join(&movement.item.name);
        let (from, to) = if undo {
            (&organized, &original)
        } else {
            (&original, &organized)
        };
        if let Some(indices) = sources.get(&norm(&from.to_string_lossy())) {
            for &index in indices {
                let app = &mut apps[index];
                if movement.item.launch.as_ref() == Some(&app.identity()) {
                    // Update only the source path, never preferences/history.
                    app.source = to.to_string_lossy().into_owned();
                    store.put_app(app)?;
                }
            }
        }
    }
    Ok(())
}

fn layout_equal(a: &native::Layout, b: &native::Layout) -> bool {
    if a.topology != b.topology || a.flags != b.flags || a.items.len() != b.items.len() {
        return false;
    }
    let entries = |layout: &native::Layout| {
        let mut entries: Vec<_> = layout
            .items
            .iter()
            .map(|i| (norm(&i.path), i.position.x, i.position.y))
            .collect();
        entries.sort();
        entries
    };
    entries(a) == entries(b)
}

// Only restore the icon positions of files this Undo actually moved. A user
// may have dragged an original back before Undo and chosen a new position;
// treating that already-restored file as an Undo target would override it.
fn layout_targets(
    journal: &Journal,
    undo: bool,
    restored: &HashSet<String>,
) -> Vec<(String, Point)> {
    if undo {
        journal
            .movements
            .iter()
            .filter(|m| restored.contains(&m.item.name))
            .filter_map(|m| {
                m.item.position.clone().map(|point| {
                    (
                        Path::new(&journal.root)
                            .join(&m.item.name)
                            .to_string_lossy()
                            .into_owned(),
                        point,
                    )
                })
            })
            .collect()
    } else {
        journal
            .folders
            .iter()
            .filter(|f| f.created)
            .filter_map(|f| {
                f.position.clone().map(|point| {
                    (
                        Path::new(&journal.root)
                            .join(&f.name)
                            .to_string_lossy()
                            .into_owned(),
                        point,
                    )
                })
            })
            .collect()
    }
}

// Plan positions using the current Explorer layout. A returned icon may occupy
// another returned icon's old cell: both can be moved together. An unrelated or
// non-moving icon always wins. Dependencies propagate so a failed move never
// leaves another restore targeting the stationary icon's cell.
fn restore_position_plan(
    current: &native::Layout,
    requested: Vec<(String, Point)>,
) -> (Vec<(String, Point)>, usize) {
    let mut skipped = 0;
    let by_path: HashMap<String, &native::Item> = current
        .items
        .iter()
        .map(|item| (norm(&item.path), item))
        .collect();
    let mut proposals = Vec::new();
    for (path, point) in requested {
        match by_path.get(&norm(&path)) {
            Some(item) if item.position == point => {}
            Some(_) => proposals.push((path, point)),
            None => skipped += 1,
        }
    }
    let proposed_ids: HashSet<String> = proposals.iter().map(|(p, _)| norm(p)).collect();
    let sx = i64::from(current.spacing.x.max(3));
    let sy = i64::from(current.spacing.y.max(3));
    let cell = |point: &Point| {
        (
            i64::from(point.x).div_euclid(sx),
            i64::from(point.y).div_euclid(sy),
        )
    };
    let overlaps = |a: &Point, b: &Point| {
        (i64::from(a.x) - i64::from(b.x)).abs() < sx - 2
            && (i64::from(a.y) - i64::from(b.y)).abs() < sy - 2
    };
    let mut occupied: HashMap<(i64, i64), Vec<&native::Item>> = HashMap::new();
    for item in &current.items {
        occupied.entry(cell(&item.position)).or_default().push(item);
    }
    let mut targets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, (_, target)) in proposals.iter().enumerate() {
        targets.entry(cell(target)).or_default().push(index);
    }

    let mut blocked = HashSet::<String>::new();
    let mut dependents: HashMap<String, Vec<String>> = HashMap::new();
    for (index, (path, target)) in proposals.iter().enumerate() {
        let id = norm(path);
        let (x, y) = cell(target);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let neighbor = (x + dx, y + dy);
                for other in targets.get(&neighbor).into_iter().flatten() {
                    if *other != index && overlaps(target, &proposals[*other].1) {
                        // Never request two icons in the same destination cell.
                        blocked.insert(id.clone());
                    }
                }
                for item in occupied.get(&neighbor).into_iter().flatten() {
                    let occupant = norm(&item.path);
                    if occupant == id || !overlaps(target, &item.position) {
                        continue;
                    }
                    if proposed_ids.contains(&occupant) {
                        dependents.entry(occupant).or_default().push(id.clone());
                    } else {
                        blocked.insert(id.clone());
                    }
                }
            }
        }
    }
    let mut queue: Vec<String> = blocked.iter().cloned().collect();
    while let Some(id) = queue.pop() {
        if let Some(waiting) = dependents.get(&id) {
            for other in waiting {
                if blocked.insert(other.clone()) {
                    queue.push(other.clone());
                }
            }
        }
    }
    skipped += blocked.len();
    (
        proposals
            .into_iter()
            .filter(|(path, _)| !blocked.contains(&norm(path)))
            .collect(),
        skipped,
    )
}

fn restore_layout(journal: &Journal, undo: bool, restored: &HashSet<String>) -> Result<()> {
    let Some(before) = &journal.layout else {
        return Ok(());
    };
    native::refresh(Path::new(&journal.root));
    let current = native::layout()?;
    ensure!(
        current.topology == before.topology && current.flags & 1 == 0,
        "Files are safe, but the display layout changed. Icon positions were not restored."
    );
    // Never revert a later manual movement of an unrelated icon. Position only
    // newly created folders (Apply) or originals actually moved by this Undo.
    let (positions, skipped) =
        restore_position_plan(&current, layout_targets(journal, undo, restored));
    native::position_many(&positions)?;
    ensure!(
        skipped == 0,
        "Files are safe. {skipped} icon positions were left unchanged: occupied places or Explorer refresh pending."
    );
    Ok(())
}

// An unchecked proposal is also user consent. Never persist its stale
// filesystem identity while applying unrelated selected moves.
fn recheck_rejected(root: &Path, plan: &Plan) -> Result<()> {
    for item in plan.items.iter().filter(|item| !item.selected) {
        fs::leaf(&item.name)?;
        let actual = fs::inspect(&root.join(&item.name), item.stamp.digest.is_some())
            .context("An excluded object changed after preview; refresh the plan")?;
        ensure!(
            actual == item.stamp,
            "An excluded object changed after preview; refresh the plan"
        );
    }
    Ok(())
}

// Keep-only consent changes SQLite, not desktop paths. The normal worker
// remains running; no file-move startup/background lock is required.
fn remember_keep_locked(store: &Store, plan: &Plan) -> Result<Outcome> {
    ensure!(
        plan.movements().is_empty(),
        "Keep-only consent cannot move desktop objects"
    );
    // Rejecting every suggestion is still a useful user decision. Save it
    // without creating a fake move, a recovery journal or any new folder.
    let rejected: Vec<_> = plan.items.iter().filter(|item| !item.selected).collect();
    ensure!(
        !rejected.is_empty(),
        "Select an object to organize or keep on the desktop"
    );
    let actual_root = root(store)?;
    let directory = fs::directory(&actual_root)?;
    ensure!(
        fs::identity(&directory)? == plan.root_id
            && norm(&actual_root.to_string_lossy()) == norm(&plan.root),
        "The desktop changed after preview; refresh the plan"
    );
    // A failed check must leave every existing exclusion unchanged.
    recheck_rejected(&actual_root, plan)?;
    store.transaction(|| {
        ensure_ready(store)?;
        let mut keep: HashSet<String> = store.get::<Vec<String>>(KEEP)?.into_iter().collect();
        keep.extend(rejected.iter().map(|item| item.stamp.id.clone()));
        let mut keep: Vec<_> = keep.into_iter().collect();
        keep.sort();
        store.set(KEEP, &keep)
    })?;
    Ok(Outcome {
        kept: rejected.len(),
        ..Default::default()
    })
}

/// Persist explicit leave-on-desktop choices without stopping foreground
/// collection or restarting the background worker. All competing organizers
/// and shortcut writers still share the normal changes lock.
pub(crate) fn remember_keep(store: &Store, plan: &Plan) -> Result<Outcome> {
    let _changes = native::exclusive("changes", &store.home)?;
    let _desktop = native::exclusive("organization", Path::new(&plan.root))?;
    ensure_ready(store)?;
    no_placement_in_progress(store)?;
    remember_keep_locked(store, plan)
}

#[cfg(test)]
pub fn apply(store: &Store, plan: &Plan, cancel: &AtomicBool) -> Result<Outcome> {
    let _stopped = native::exclusive("background", &store.home)?;
    apply_stopped(store, plan, cancel)
}

/// Caller must hold the background singleton and startup lock together.
pub(crate) fn apply_stopped(store: &Store, plan: &Plan, cancel: &AtomicBool) -> Result<Outcome> {
    let _changes = native::exclusive("changes", &store.home)?;
    let _desktop = native::exclusive("organization", Path::new(&plan.root))?;
    ensure_ready(store)?;
    no_placement_in_progress(store)?;
    let movements = plan.movements();
    if movements.is_empty() {
        return remember_keep_locked(store, plan);
    }
    let mut journal = Journal {
        id: fs::operation_id()?,
        root: plan.root.clone(),
        root_id: plan.root_id.clone(),
        phase: Phase::Prepared,
        movements,
        folders: plan.folders.clone(),
        layout: plan.layout.clone(),
    };
    journal
        .folders
        .retain(|f| journal.movements.iter().any(|m| m.folder == f.name));
    for folder in journal.folders.iter_mut().filter(|f| f.created) {
        folder.position = journal
            .movements
            .iter()
            .find(|m| m.folder == folder.name)
            .and_then(|m| m.item.position.clone());
    }
    let _root_handle = checked_root(&journal, store)?;
    if let Some(before) = &journal.layout {
        let current = native::layout()?;
        ensure!(
            current.flags & 1 == 0,
            "Turn off Windows auto-arrange before moving desktop objects"
        );
        ensure!(
            layout_equal(before, &current),
            "The desktop changed after preview; refresh the plan"
        );
    }
    let root_path = PathBuf::from(&journal.root);
    let root = root_path.as_path();
    let state: State = store.get("state")?;
    let apps = store.apps()?;
    // A preview is not durable permission. Another window can save a newer
    // Keep choice and undo its moves while leaving the old preview open.
    // Read the current choices under the same changes lock as every writer.
    let keep: HashSet<String> = store.get::<Vec<String>>(KEEP)?.into_iter().collect();
    let paths = plan::paths(root, cancel)?;
    let dependencies = plan::dependencies(root, &paths, &apps, cancel)?;
    let managed_paths: HashSet<_> = state.owned.iter().map(|o| norm(&o.path)).collect();
    let managed_ids: HashSet<_> = state
        .owned
        .iter()
        .filter_map(|o| o.file_id.as_deref())
        .collect();
    let manual_paths: HashSet<_> = apps
        .iter()
        .filter(|a| a.policy != Policy::Auto)
        .map(|a| norm(&a.source))
        .collect();
    for movement in &journal.movements {
        fs::leaf(&movement.item.name)?;
        fs::leaf(&movement.folder)?;
        let path = root.join(&movement.item.name);
        let actual = fs::inspect(&path, movement.item.stamp.digest.is_some())?;
        ensure!(
            actual == movement.item.stamp,
            "An object changed after preview; refresh the plan"
        );
        ensure!(
            !keep.contains(&actual.id),
            "Your keep-on-desktop choice changed after preview; refresh the plan"
        );
        let normalized = norm(&path.to_string_lossy());
        ensure!(
            !managed_ids.contains(actual.id.as_str()) && !managed_paths.contains(&normalized),
            "This object became a managed quick shortcut; refresh the plan"
        );
        ensure!(
            !manual_paths.contains(&normalized),
            "A manual rule changed after preview; refresh the plan"
        );
        dependencies.check(&path, plan::is_shortcut(&path))?;
        ensure!(
            plan::classify(&path, &plan.options)?
                == (movement.item.group, movement.item.launch.clone()),
            "A shortcut changed after preview"
        );
        ensure!(
            fs::absent(&root.join(&movement.folder).join(&movement.item.name))?,
            "A destination already exists. Nothing will be overwritten."
        );
    }
    // Moving selected items must not silently store Keep for a deleted,
    // replaced or edited unchecked object from the old preview.
    recheck_rejected(root, plan)?;
    for folder in &journal.folders {
        fs::leaf(&folder.name)?;
        if folder.created {
            ensure!(
                fs::absent(&root.join(&folder.name))?,
                "A folder appeared after preview; refresh the plan"
            );
        } else {
            checked_folder(root, folder)?;
        }
    }
    ensure!(
        !cancel.load(Ordering::Relaxed),
        "Organization cancelled before changing files"
    );
    // Flush durable intent before creating a folder or renaming the first file.
    store.db.pragma_update(None, "synchronous", "FULL")?;
    store.transaction(|| {
        ensure_ready(store)?;
        no_placement_in_progress(store)?;
        install_catalog_barrier(store)?;
        save(store, &journal)?;
        store.set(ACTIVE, &Some(&journal.id))
    })?;
    let result = (|| -> Result<Outcome> {
        for index in 0..journal.folders.len() {
            ensure!(!cancel.load(Ordering::Relaxed), "Organization cancelled");
            if journal.folders[index].created {
                std::fs::create_dir(root.join(&journal.folders[index].name))?;
                let handle = fs::directory(&root.join(&journal.folders[index].name))?;
                journal.folders[index].id = Some(fs::identity(&handle)?);
                save(store, &journal)?;
            }
        }
        for (index, movement) in journal.movements.iter().enumerate() {
            ensure!(!cancel.load(Ordering::Relaxed), "Organization cancelled");
            let folder = journal
                .folders
                .iter()
                .find(|f| f.name == movement.folder)
                .context("Missing folder reservation")?;
            let directory = checked_folder(root, folder)?;
            fs::rename_file(
                &root.join(&movement.item.name),
                &directory,
                &movement.item.name,
                &movement.item.stamp,
                true,
            )?;
            if let Some(expected) = &movement.item.launch {
                let destination = root.join(&movement.folder).join(&movement.item.name);
                ensure!(
                    metadata::read(&destination)?.identity() == *expected,
                    "The relocated shortcut changed launch identity; restoring the operation"
                );
            }
            #[cfg(test)]
            tests::checkpoint(index)?;
            #[cfg(not(test))]
            let _ = index;
        }
        ensure!(!cancel.load(Ordering::Relaxed), "Organization cancelled");
        let mut outcome = Outcome {
            moved: journal.movements.len(),
            folders_created: journal.folders.iter().filter(|f| f.created).count(),
            kept: plan.items.iter().filter(|item| !item.selected).count(),
            ..Default::default()
        };
        if let Err(error) = restore_layout(&journal, false, &HashSet::new()) {
            outcome.warnings.push(format!("{error:#}"));
        }
        store.transaction(|| {
            remove_catalog_barrier(store)?;
            update_catalog(store, &journal, false)?;
            let mut folders: Vec<Folder> = store.get(FOLDERS)?;
            for folder in &journal.folders {
                folders.retain(|old| old.group != folder.group);
                folders.push(folder.clone());
            }
            store.set(FOLDERS, &folders)?;
            let mut keep: HashSet<String> = store.get::<Vec<String>>(KEEP)?.into_iter().collect();
            keep.extend(
                plan.items
                    .iter()
                    .filter(|i| !i.selected)
                    .map(|i| i.stamp.id.clone()),
            );
            let mut keep: Vec<_> = keep.into_iter().collect();
            keep.sort();
            store.set(KEEP, &keep)?;
            journal.phase = Phase::Committed;
            save(store, &journal)?;
            let mut history: Vec<String> = store.get(HISTORY)?;
            history.push(journal.id.clone());
            store.set(HISTORY, &history)?;
            store.event(
                "organization",
                &format!(
                    "Organized {} objects into {} folders",
                    outcome.moved,
                    journal.folders.len()
                ),
            )?;
            store.set(ACTIVE, &Option::<String>::None)
        })?;
        Ok(outcome)
    })();
    match result {
        Ok(outcome) => Ok(outcome),
        Err(error) => {
            // Use committed intent, not an in-memory phase changed by a failed
            // SQLite commit. A missing completion write never licenses deletion.
            let mut durable = load(store, &journal.id)?;
            match undo_locked(store, &mut durable) {
                Ok(_) => {
                    Err(error.context("Organization did not complete; moved objects were restored"))
                }
                Err(recovery) => Err(error.context(format!(
                    "Recovery needs attention; no collisions were overwritten: {recovery:#}"
                ))),
            }
        }
    }
}

fn undo_locked(store: &Store, journal: &mut Journal) -> Result<Outcome> {
    let root_handle = checked_root(journal, store)?;
    let root_path = PathBuf::from(&journal.root);
    let root = root_path.as_path();
    if journal.phase == Phase::Committed {
        let mut verified_folders = HashSet::new();
        // Placement may legitimately reuse the vacated original filename. Do
        // not begin partial organization Undo or install its durable barrier:
        // that would also prevent the quick-placement Undo needed to free it.
        // Verify current ownership by file ID, never by identical shortcut bytes.
        let state: State = store.get("state")?;
        for movement in &journal.movements {
            fs::leaf(&movement.item.name)?;
            let original = root.join(&movement.item.name);
            // A person may have dragged the original file back after Apply.
            // Identity, not mere name occupancy, distinguishes an already
            // restored original from a foreign collision. The Undo loop below
            // already treats that verified source as an idempotent no-op.
            if fs::matches_id(&original, &movement.item.stamp.id)? {
                continue;
            }
            let original_path = norm(&original.to_string_lossy());
            if let Some(copy) = state.owned.iter().find(|o| norm(&o.path) == original_path)
                && let Some(id) = &copy.file_id
                && id != &movement.item.stamp.id
                && fs::matches_id(&original, id)?
            {
                let message = if store.get::<Config>("config")?.language == "ru" {
                    "Сначала отмените размещение быстрых ярлыков на главной странице, затем повторите отмену уборки. Быстрый ярлык занимает прежнее имя оригинала. Файлы не изменены."
                } else {
                    "Undo quick shortcut placement on Home first, then retry organization Undo. A quick shortcut occupies an original name. No files were changed."
                };
                bail!("{message}");
            }
            // A newer user-owned file can also occupy a vacated name. Check
            // every original name before the durable Undo barrier: otherwise
            // a later collision would leave preceding moves partially undone
            // and block unrelated quick-placement recovery.
            if !fs::absent(&original)? {
                if store.get::<Config>("config")?.language == "ru" {
                    bail!(
                        "Исходное имя на рабочем столе занято другим объектом. Переместите или переименуйте его и повторите отмену уборки. Файлы не изменены."
                    );
                }
                bail!(
                    "An original desktop name is occupied by another object. Move or rename it, then retry organization Undo. No files were changed."
                );
            }
            // Missing/replaced organized sources and folders are also detectable
            // before any Undo move. Otherwise a later missing item leaves earlier
            // items partly restored and keeps the recovery barrier active.
            let folder = journal
                .folders
                .iter()
                .find(|folder| folder.name == movement.folder)
                .context("Missing folder reservation")?;
            if verified_folders.insert(&movement.folder) {
                checked_folder(root, folder)?;
            }
            let organized = root.join(&movement.folder).join(&movement.item.name);
            if !fs::matches_id(&organized, &movement.item.stamp.id)? {
                if store.get::<Config>("config")?.language == "ru" {
                    bail!(
                        "Перемещённый объект отсутствует или заменён. Верните оригинал в папку после уборки и повторите отмену. Файлы не изменены."
                    );
                }
                bail!(
                    "An organized object is missing or has been replaced. Restore the original to its organized folder, then retry Undo. No files were changed."
                );
            }
        }
    }
    journal.phase = Phase::Undoing;
    store.db.pragma_update(None, "synchronous", "FULL")?;
    store.transaction(|| {
        install_catalog_barrier(store)?;
        save(store, journal)?;
        store.set(ACTIVE, &Some(&journal.id))
    })?;
    let mut outcome = Outcome::default();
    let mut conflicts = Vec::new();
    let mut restored_names = HashSet::new();
    for movement in journal.movements.iter().rev() {
        let result = (|| -> Result<bool> {
            fs::leaf(&movement.item.name)?;
            fs::leaf(&movement.folder)?;
            let original = root.join(&movement.item.name);
            let organized = root.join(&movement.folder).join(&movement.item.name);
            if fs::matches_id(&original, &movement.item.stamp.id)? {
                return Ok(false);
            }
            ensure!(
                fs::absent(&original)?,
                "Original name is occupied by another object"
            );
            ensure!(
                fs::matches_id(&organized, &movement.item.stamp.id)?,
                "Organized object is missing or has been replaced"
            );
            let folder = journal
                .folders
                .iter()
                .find(|f| f.name == movement.folder)
                .context("Missing folder identity")?;
            let _folder = checked_folder(root, folder)?;
            // Same-file edits survive Undo; a replacement is never moved.
            fs::rename_file(
                &organized,
                &root_handle,
                &movement.item.name,
                &movement.item.stamp,
                false,
            )?;
            Ok(true)
        })();
        match result {
            Ok(true) => {
                outcome.restored += 1;
                restored_names.insert(movement.item.name.clone());
            }
            Ok(false) => {}
            Err(error) => conflicts.push(format!("{}: {error:#}", movement.item.name)),
        }
    }
    if !conflicts.is_empty() {
        bail!(
            "Some objects could not be restored. Resolve these conflicts and retry:\n{}",
            conflicts.join("\n")
        );
    }
    for folder in journal.folders.iter().filter(|f| f.created) {
        let path = root.join(&folder.name);
        if fs::absent(&path)? {
            continue;
        }
        if folder.id.is_none() {
            // A crash between mkdir and recording its ID does not authorize
            // deleting a folder. No file could yet have been moved into it.
            outcome.warnings.push(format!(
                "Unverified category folder was preserved: {}",
                folder.name
            ));
            continue;
        }
        let checked = checked_folder(root, folder);
        match checked {
            Ok(handle) => {
                drop(handle);
                if let Err(error) =
                    fs::remove_empty_directory(&path, folder.id.as_deref().unwrap_or(""))
                {
                    outcome
                        .warnings
                        .push(format!("Folder was preserved: {} ({error})", folder.name));
                }
            }
            Err(error) => outcome
                .warnings
                .push(format!("Folder was preserved: {} ({error})", folder.name)),
        }
    }
    if let Err(error) = restore_layout(journal, true, &restored_names) {
        outcome.warnings.push(format!("{error:#}"));
    }
    store.transaction(|| {
        remove_catalog_barrier(store)?;
        update_catalog(store, journal, true)?;
        journal.phase = Phase::Undone;
        save(store, journal)?;
        store.event(
            "organization",
            &format!("Restored {} organized objects", outcome.restored),
        )?;
        store.set(ACTIVE, &Option::<String>::None)
    })?;
    Ok(outcome)
}

#[cfg(test)]
pub fn undo(store: &Store, id: &str) -> Result<Outcome> {
    let _stopped = native::exclusive("background", &store.home)?;
    undo_stopped(store, id)
}

/// Caller must hold the background singleton and startup lock together.
pub(crate) fn undo_stopped(store: &Store, id: &str) -> Result<Outcome> {
    let _changes = native::exclusive("changes", &store.home)?;
    let mut journal = load(store, id)?;
    let _desktop = native::exclusive("organization", Path::new(&journal.root))?;
    ensure!(
        status(store)?.undo.as_deref() == Some(id),
        "A newer operation exists; refresh organization history"
    );
    ensure!(
        matches!(
            journal.phase,
            Phase::Prepared | Phase::Committed | Phase::Undoing
        ),
        "This operation cannot be undone"
    );
    no_placement_in_progress(store)?;
    undo_locked(store, &mut journal)
}

/// Explicit recovery escape hatch, with a separate confirmation in the UI.
/// No filesystem writes: reconcile verified paths and retain the journal.
#[cfg(test)]
pub fn accept_current(store: &Store, id: &str) -> Result<()> {
    let _stopped = native::exclusive("background", &store.home)?;
    accept_current_stopped(store, id)
}

/// Caller must hold the background singleton and startup lock together.
pub(crate) fn accept_current_stopped(store: &Store, id: &str) -> Result<()> {
    let _changes = native::exclusive("changes", &store.home)?;
    let mut journal = load(store, id)?;
    let _desktop = native::exclusive("organization", Path::new(&journal.root))?;
    let _root = checked_root(&journal, store)?;
    ensure!(
        store.get::<Option<String>>(ACTIVE)?.as_deref() == Some(id),
        "The interrupted operation changed"
    );
    store.db.pragma_update(None, "synchronous", "FULL")?;
    store.transaction(|| {
        remove_catalog_barrier(store)?;
        for movement in journal.movements.clone() {
            let original = Path::new(&journal.root).join(&movement.item.name);
            let organized = Path::new(&journal.root)
                .join(&movement.folder)
                .join(&movement.item.name);
            let mut single = journal.clone();
            single.movements = vec![movement.clone()];
            if fs::matches_id(&original, &movement.item.stamp.id)? {
                update_catalog(store, &single, true)?;
            } else if fs::matches_id(&organized, &movement.item.stamp.id)? {
                let folder = journal
                    .folders
                    .iter()
                    .find(|f| f.name == movement.folder)
                    .context("Missing folder identity")?;
                checked_folder(Path::new(&journal.root), folder)?;
                update_catalog(store, &single, false)?;
            } else {
                // Do not dismiss crash recovery when an original is absent
                // from both recorded locations (or replaced by another file).
                // The transaction must retain its barrier and journal so the
                // user can put that original back and retry safely.
                bail!(
                    "An original desktop object is missing from both recorded locations. Restore the original file to its desktop name or organized folder before accepting the current locations."
                );
            }
        }
        journal.phase = Phase::Accepted;
        save(store, &journal)?;
        store.event(
            "organization",
            "Current file locations were explicitly accepted; journal retained",
        )?;
        store.set(ACTIVE, &Option::<String>::None)
    })
}

#[cfg(test)]
mod layout_target_tests {
    use super::*;

    #[test]
    fn undo_preserves_the_position_of_an_original_manually_restored_first() {
        let moved = |name: &str, x: i32| Movement {
            item: Item {
                name: name.into(),
                group: Group::Sites,
                selected: true,
                position: Some(Point { x, y: 30 }),
                stamp: fs::Stamp {
                    id: format!("id-{name}"),
                    size: 100,
                    modified: 42,
                    digest: None,
                },
                launch: None,
            },
            folder: "Sites".into(),
        };
        let journal = Journal {
            id: "synthetic".into(),
            root: r"C:\Example\Desktop".into(),
            root_id: "desktop".into(),
            phase: Phase::Committed,
            movements: vec![moved("Alpha.url", 10), moved("Beta.url", 20)],
            folders: Vec::new(),
            layout: None,
        };
        // Alpha was returned by hand to an intentionally different grid cell.
        // Only Beta was actually moved by Undo.
        let restored = HashSet::from(["Beta.url".to_owned()]);
        assert_eq!(
            layout_targets(&journal, true, &restored),
            vec![(
                r"C:\Example\Desktop\Beta.url".into(),
                Point { x: 20, y: 30 }
            )]
        );
        // Replaying Undo after interruption cannot relocate already returned
        // originals merely because the old journal includes them.
        assert!(layout_targets(&journal, true, &HashSet::new()).is_empty());
    }
    fn path(name: &str) -> String {
        format!(r"C:\Example\Desktop\{name}")
    }

    fn icon(name: &str, x: i32) -> native::Item {
        native::Item {
            path: path(name),
            name: name.into(),
            position: Point { x, y: 0 },
        }
    }

    fn current(items: Vec<native::Item>) -> native::Layout {
        native::Layout {
            items,
            spacing: Point { x: 100, y: 100 },
            origin: Point { x: 0, y: 0 },
            work: [0, 0, 1000, 1000],
            flags: 0,
            topology: "test".into(),
            icon_size: 32,
        }
    }

    #[test]
    fn undo_swapped_cells_positions_both_returned_icons_in_one_batch() {
        // Explorer reused each other icon's original grid cell after files
        // reappeared. Both icons are moving, so neither is a foreign blocker.
        let layout = current(vec![icon("Alpha.url", 100), icon("Beta.url", 0)]);
        let requested = vec![
            (path("Alpha.url"), Point { x: 0, y: 0 }),
            (path("Beta.url"), Point { x: 100, y: 0 }),
        ];
        let (moves, skipped) = restore_position_plan(&layout, requested.clone());
        assert_eq!(moves, requested);
        assert_eq!(skipped, 0);
    }

    #[test]
    fn undo_never_displaces_a_stationary_icon_or_a_blocked_dependency_chain() {
        // The first target is occupied by an unrelated icon; the other
        // requested icons depend on that move and must also stay put.
        let layout = current(vec![
            icon("Alpha.url", 0),
            icon("Beta.url", 100),
            icon("Gamma.url", 200),
            icon("Unrelated.url", 300),
        ]);
        let requested = vec![
            (path("Alpha.url"), Point { x: 100, y: 0 }),
            (path("Beta.url"), Point { x: 200, y: 0 }),
            (path("Gamma.url"), Point { x: 300, y: 0 }),
        ];
        let (moves, skipped) = restore_position_plan(&layout, requested);
        assert!(moves.is_empty());
        assert_eq!(skipped, 3);

        // A free independent destination can still be restored without
        // moving the unrelated icon or any blocked dependency.
        let (moves, skipped) = restore_position_plan(
            &layout,
            vec![
                (path("Alpha.url"), Point { x: 200, y: 0 }),
                (path("Beta.url"), Point { x: 500, y: 0 }),
            ],
        );
        assert_eq!(moves, vec![(path("Beta.url"), Point { x: 500, y: 0 })]);
        assert_eq!(skipped, 1);
    }

    #[test]
    fn overlapping_restoration_destinations_are_never_requested_together() {
        let layout = current(vec![icon("Alpha.url", 0), icon("Beta.url", 100)]);
        let (moves, skipped) = restore_position_plan(
            &layout,
            vec![
                (path("Alpha.url"), Point { x: 300, y: 0 }),
                (path("Beta.url"), Point { x: 300, y: 0 }),
            ],
        );
        assert!(moves.is_empty());
        assert_eq!(skipped, 2);
    }

}
