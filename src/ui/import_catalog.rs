use super::preserve_app_preferences;
use crate::model::App;
use anyhow::{Result, ensure};
use std::collections::{HashMap, HashSet};

/// One transaction-local index. Legacy launch identities must be looked up
/// without walking every saved app for each new file in a large import.
/// Ambiguous legacy identities remain ambiguous; exact saved IDs still win.
pub(super) struct ImportCatalog {
    apps: HashMap<String, App>,
    identities: HashMap<String, HashSet<String>>,
}

impl ImportCatalog {
    pub(super) fn new(apps: impl IntoIterator<Item = App>) -> Self {
        let mut catalog = Self {
            apps: HashMap::new(),
            identities: HashMap::new(),
        };
        for app in apps {
            catalog.insert(app.id.clone(), app);
        }
        catalog
    }

    pub(super) fn get(&self, id: &str) -> Option<&App> {
        self.apps.get(id)
    }

    pub(super) fn prepare(&self, app: &mut App) -> Result<()> {
        let previous = if let Some(previous) = self.apps.get(&app.id) {
            Some(previous)
        } else if let Some(ids) = self.identities.get(&app.identity()) {
            ensure!(
                ids.len() == 1,
                "Several saved shortcuts match this app. Refresh the list before importing it again."
            );
            ids.iter().next().and_then(|id| self.apps.get(id))
        } else {
            None
        };
        preserve_app_preferences(app, previous);
        Ok(())
    }

    pub(super) fn insert(&mut self, id: String, app: App) {
        let identity = app.identity();
        if let Some(previous) = self.apps.insert(id.clone(), app) {
            let old_identity = previous.identity();
            let empty = self.identities.get_mut(&old_identity).is_some_and(|ids| {
                ids.remove(&id);
                ids.is_empty()
            });
            if empty {
                self.identities.remove(&old_identity);
            }
        }
        self.identities.entry(identity).or_default().insert(id);
    }
}
