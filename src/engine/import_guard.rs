use super::file_identity;
use crate::{model::*, store::Store};
use anyhow::{Result, ensure};
use std::{collections::HashSet, path::Path};

/// Called under the changes lock and the import transaction. A managed quick
/// copy is a reference to an existing catalog app, never a new original source.
/// Match file identity as well as path so a just-renamed copy is protected even
/// before the regular manual-change reconciliation has observed its new name.
pub(crate) fn imported_quick_copy(store: &Store, candidate: &App) -> Result<Option<App>> {
    let state: State = store.get("state")?;
    if state.owned.is_empty() {
        return Ok(None);
    }
    let source = norm(&candidate.source);
    let mut owners: HashSet<_> = state
        .owned
        .iter()
        .filter(|owned| norm(&owned.path) == source)
        .map(|owned| owned.app.as_str())
        .collect();
    if owners.is_empty()
        && state.owned.iter().any(|owned| owned.file_id.is_some())
        && let Some(identity) = file_identity(Path::new(&candidate.source))?
    {
        owners.extend(
            state
                .owned
                .iter()
                .filter(|owned| owned.file_id.as_deref() == Some(identity.as_str()))
                .map(|owned| owned.app.as_str()),
        );
    }
    let Some(id) = owners.iter().next() else {
        return Ok(None);
    };
    ensure!(
        owners.len() == 1,
        "Several saved shortcuts match this app. Refresh the list before importing it again."
    );
    let original = store.app(id)?;
    ensure!(
        original.identity() == candidate.identity(),
        "This quick shortcut was changed. Add its original shortcut instead."
    );
    Ok(Some(original))
}
