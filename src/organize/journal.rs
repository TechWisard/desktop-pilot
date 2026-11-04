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

fn restore_layout(journal: &Journal, undo: bool) -> Result<()> {
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
    // newly created folders (Apply) or the original objects being restored (Undo).
    let requested: Vec<(String, Point)> = if undo {
        journal
            .movements
            .iter()
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
    };
    let mut positions = Vec::new();
    let mut skipped = 0;
    for (path, point) in requested {
        let Some(actual) = current.items.iter().find(|i| norm(&i.path) == norm(&path)) else {
            skipped += 1;
            continue;
        };
        if actual.position == point {
            continue;
        }
        if current.items.iter().any(|i| {
            norm(&i.path) != norm(&path)
                && (i64::from(i.position.x) - i64::from(point.x)).abs()
                    < i64::from(current.spacing.x) - 2
                && (i64::from(i.position.y) - i64::from(point.y)).abs()
                    < i64::from(current.spacing.y) - 2
        }) {
            skipped += 1;
            continue;
        }
        positions.push((path, point));
    }
    native::position_many(&positions)?;
    ensure!(
        skipped == 0,
        "Files are safe. {skipped} icon positions were left unchanged: occupied places or Explorer refresh pending."
    );
    Ok(())
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
    ensure!(
        !movements.is_empty(),
        "No group of objects is selected for organization"
    );
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
            ..Default::default()
        };
        if let Err(error) = restore_layout(&journal, false) {
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
            Ok(true) => outcome.restored += 1,
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
    if let Err(error) = restore_layout(journal, true) {
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
