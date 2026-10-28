use super::{file_identity, fingerprint};
use crate::{model::*, store::Store};
use anyhow::{Result, ensure};
use std::path::Path;

/// Called under the changes lock and the import transaction. A managed quick
/// copy is a reference to an existing catalog app, never a new original source.
/// Match filesystem identity as well as path to protect just-renamed copies.
/// A matching launch target alone does not prove that the owned file is intact:
/// users can change a shortcut's icon without changing its launch identity.
pub(crate) fn imported_quick_copy(store: &Store, candidate: &App) -> Result<Option<App>> {
    let state: State = store.get("state")?;
    if state.owned.is_empty() {
        return Ok(None);
    }
    let source = norm(&candidate.source);
    let mut matches: Vec<_> = state
        .owned
        .iter()
        .filter(|owned| norm(&owned.path) == source)
        .collect();
    if matches.is_empty()
        && state.owned.iter().any(|owned| owned.file_id.is_some())
        && let Some(identity) = file_identity(Path::new(&candidate.source))?
    {
        matches.extend(
            state
                .owned
                .iter()
                .filter(|owned| owned.file_id.as_deref() == Some(identity.as_str())),
        );
    }
    let Some(managed) = matches.first() else {
        return Ok(None);
    };
    ensure!(
        matches.iter().all(|owned| owned.app == managed.app),
        "Several saved shortcuts match this app. Refresh the list before importing it again."
    );
    let original = store.app(&managed.app)?;
    ensure!(
        original.identity() == candidate.identity(),
        "This quick shortcut was changed. Add its original shortcut instead."
    );
    let actual_sha = fingerprint(Path::new(&candidate.source))?;
    ensure!(
        matches.iter().all(|owned| owned.sha == actual_sha),
        "This quick shortcut was changed. Add its original shortcut instead."
    );
    Ok(Some(original))
}
