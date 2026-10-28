use super::import_ownership_tests::with_profile;
use super::*;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(super) struct ImportMetrics {
    identity_calls: usize,
    import_seconds: f64,
    transaction_seconds: f64,
}

static METRICS: LazyLock<Mutex<HashMap<PathBuf, Option<ImportMetrics>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

// Only explicitly registered test profiles retain diagnostics. All counters
// and the hook calling this function are absent from the shipped executable.
pub(super) fn record(home: &Path, identity_calls: usize, total: Duration, transaction: Duration) {
    if let Some(slot) = METRICS.lock().unwrap().get_mut(home) {
        *slot = Some(ImportMetrics {
            identity_calls,
            import_seconds: total.as_secs_f64(),
            transaction_seconds: transaction.as_secs_f64(),
        });
    }
}

fn import(
    settings: &mut Settings,
    paths: Vec<PathBuf>,
) -> std::result::Result<(ImportResult, ImportMetrics), String> {
    let home = settings.store.home.clone();
    METRICS.lock().unwrap().insert(home.clone(), None);
    settings.add_shortcuts(paths);
    let job = settings.job.take().expect("real import job must start");
    let response = job
        .receiver
        .recv_timeout(Duration::from_secs(180))
        .expect("bounded import completion");
    let metrics = METRICS.lock().unwrap().remove(&home).flatten();
    match response? {
        JobResult::Added(result) => {
            Ok((result, metrics.expect("production import instrumentation")))
        }
        _ => panic!("unexpected import result"),
    }
}

fn shortcut(path: &Path, target: &str) -> App {
    std::fs::write(path, format!("[InternetShortcut]\r\nURL={target}\r\n")).unwrap();
    native::read_link(path).unwrap()
}

fn protected_data(settings: &Settings, anchor: &App) -> serde_json::Value {
    serde_json::json!({
        "app": settings.store.app(&anchor.id).unwrap(),
        "days": settings.store.days().unwrap(),
        "config": settings.store.get::<Config>("config").unwrap(),
        "state": settings.store.get::<State>("state").unwrap(),
    })
}

#[test]
fn large_manual_import_and_saved_restart_scale_linearly() {
    let mut bounded = true;
    for count in [100usize, 1_000, 5_000] {
        with_profile(|settings, anchor| {
            let before = protected_data(settings, anchor);
            let quick = settings.store.get::<State>("state").unwrap().owned[0].clone();
            let original_bytes = std::fs::read(&anchor.source).unwrap();
            let batch = settings.store.home.join("Batch");
            std::fs::create_dir(&batch).unwrap();
            let paths: Vec<_> = (0..count)
                .map(|i| {
                    let path = batch.join(format!("Batch-{i:05}.url"));
                    shortcut(&path, &format!("https://example.invalid/Project/{i}"));
                    path
                })
                .collect();
            let (result, metrics) = import(settings, paths.clone()).unwrap();
            assert!(result.failed.is_empty());
            assert_eq!(result.created, count);
            assert_eq!(result.updated, 0);
            assert_eq!(result.added.len(), count);
            assert_eq!(settings.store.apps().unwrap().len(), count + 1);
            assert_eq!(protected_data(settings, anchor), before);
            bounded &= metrics.identity_calls <= 8 * (count + 1) + 16;
            let epoch: u64 = settings.store.get("catalog_epoch").unwrap();
            let (repeat, _) = import(settings, paths[..count.min(32)].to_vec()).unwrap();
            assert!(repeat.failed.is_empty());
            assert_eq!((repeat.created, repeat.updated), (0, 0));
            assert_eq!(settings.store.get::<u64>("catalog_epoch").unwrap(), epoch);

            // Measure actual Settings construction and cached search on a new
            // connection, not a substitute search/ranking model or cold disk cache.
            let query = format!("Batch-{:05}", count - 1);
            let restart = Instant::now();
            let mut reopened = Settings::new(settings.store.home.clone()).unwrap();
            reopened.catalog.update(
                &reopened.apps,
                &reopened.ranking,
                &query,
                0,
                0,
                &reopened.highlighted_apps,
            );
            let saved_search_ms = restart.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(reopened.catalog.indices.len(), 1);
            assert_eq!(reopened.apps[reopened.catalog.indices[0]].name, query);
            let preview = reopened
                .preview
                .as_ref()
                .expect("cached actionable preview");
            assert!(preview.added.is_empty() && preview.removed.is_empty());
            assert!(reopened.can_undo);
            drop(reopened);

            // Cached originals stay searchable before revalidation when their
            // optional directory is offline; manual placement must not change.
            let offline = settings.store.home.join("Batch.offline");
            std::fs::rename(&batch, &offline).unwrap();
            let restart = Instant::now();
            let mut reopened = Settings::new(settings.store.home.clone()).unwrap();
            reopened.catalog.update(
                &reopened.apps,
                &reopened.ranking,
                &query,
                0,
                0,
                &reopened.highlighted_apps,
            );
            let offline_search_ms = restart.elapsed().as_secs_f64() * 1000.0;
            assert_eq!(reopened.catalog.indices.len(), 1);
            assert_eq!(protected_data(&reopened, anchor), before);
            drop(reopened);
            std::fs::rename(&offline, &batch).unwrap();
            engine::command(&settings.store, "rollback").unwrap();
            assert!(!Path::new(&quick.path).exists());
            assert_eq!(std::fs::read(&anchor.source).unwrap(), original_bytes);
            assert!(paths.iter().all(|p| p.is_file()));
            assert_eq!(
                settings.store.app(&anchor.id).unwrap().policy,
                Policy::Always
            );
            println!(
                "IMPORT_SCALING_METRIC {}",
                serde_json::json!({
                    "entries": count, "metrics": metrics,
                    "saved_settings_search_ms": saved_search_ms,
                    "offline_cached_settings_search_ms": offline_search_ms,
                    "new_entries": result.created, "unexpected_placements": 0,
                    "originals_preserved": true, "undo_passed": true,
                })
            );
        });
    }
    assert!(
        bounded,
        "bulk import must not compare every new shortcut to the growing saved catalog"
    );
}

#[test]
fn indexed_import_keeps_legacy_history_rules_and_in_batch_duplicates() {
    with_profile(|settings, anchor| {
        let first = settings.store.home.join("Legacy.url");
        let mut legacy = shortcut(&first, "https://example.invalid/Legacy/CaseSensitive");
        let canonical_id = legacy.id.clone();
        legacy.id = "old-format-id".into();
        legacy.policy = Policy::Never;
        legacy.note = "Deliberate exception".into();
        settings.store.put_app(&legacy).unwrap();
        settings.store.observe(&legacy.id, 77, true, false).unwrap();
        let days = serde_json::to_value(settings.store.days().unwrap()).unwrap();
        let alias = settings.store.home.join("Legacy alias.url");
        std::fs::copy(&first, &alias).unwrap();
        let new_path = settings.store.home.join("New.url");
        let new_app = shortcut(&new_path, "https://example.invalid/New");
        let new_alias = settings.store.home.join("New alias.url");
        std::fs::copy(&new_path, &new_alias).unwrap();
        let quick = settings.store.get::<State>("state").unwrap().owned[0]
            .path
            .clone();
        let (result, _) = import(
            settings,
            vec![first, new_path, alias.clone(), new_alias, quick.into()],
        )
        .unwrap();
        assert!(result.failed.is_empty());
        assert_eq!(result.added.len(), 3);
        assert_eq!((result.created, result.updated), (1, 1));
        assert_eq!(settings.store.apps().unwrap().len(), 3);
        assert!(
            !settings
                .store
                .apps()
                .unwrap()
                .iter()
                .any(|a| a.id == canonical_id)
        );
        let saved = settings.store.app(&legacy.id).unwrap();
        assert_eq!(saved.policy, Policy::Never);
        assert_eq!(saved.note, legacy.note);
        assert_eq!(norm(&saved.source), norm(&alias.to_string_lossy()));
        assert_eq!(
            serde_json::to_value(settings.store.days().unwrap()).unwrap(),
            days
        );
        assert_eq!(
            settings.store.app(&anchor.id).unwrap().source,
            anchor.source
        );
        assert!(settings.store.app(&new_app.id).is_ok());
        engine::command(&settings.store, "rollback").unwrap();
        assert!(Path::new(&anchor.source).exists());
    });
}

#[test]
fn indexed_import_rejects_ambiguous_legacy_identity_but_preserves_exact_id_precedence() {
    with_profile(|settings, _| {
        let path = settings.store.home.join("Ambiguous.url");
        let canonical = shortcut(&path, "https://example.invalid/Ambiguous");
        for (id, policy) in [("legacy-a", Policy::Always), ("legacy-b", Policy::Never)] {
            let mut saved = canonical.clone();
            saved.id = id.into();
            saved.policy = policy;
            saved.note = id.into();
            settings.store.put_app(&saved).unwrap();
        }
        let before: Vec<_> = ["legacy-a", "legacy-b"]
            .iter()
            .map(|id| settings.store.app(id).unwrap())
            .collect();
        let valid = settings.store.home.join("Independent.url");
        shortcut(&valid, "https://example.invalid/Independent");
        let (result, _) = import(settings, vec![path.clone(), valid]).unwrap();
        assert_eq!(result.failed.len(), 1);
        assert!(result.failed[0].1.contains("Several saved shortcuts"));
        assert_eq!(result.created, 1);
        assert_eq!(result.updated, 0);
        for app in &before {
            assert_eq!(settings.store.app(&app.id).unwrap(), *app);
        }
        let mut exact = canonical.clone();
        exact.policy = Policy::Never;
        exact.note = "Exact saved ID remains authoritative".into();
        settings.store.put_app(&exact).unwrap();
        let (result, _) = import(settings, vec![path]).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!((result.created, result.updated), (0, 0));
        assert_eq!(settings.store.app(&exact.id).unwrap(), exact);
        for app in &before {
            assert_eq!(settings.store.app(&app.id).unwrap(), *app);
        }
    });
}

#[test]
fn failed_indexed_batch_rolls_back_every_entry_and_can_be_retried() {
    with_profile(|settings, anchor| {
        let first = settings.store.home.join("First.url");
        shortcut(&first, "https://example.invalid/First");
        let last = settings.store.home.join("Last.url");
        let rejected = shortcut(&last, "https://example.invalid/Last");
        let before = protected_data(settings, anchor);
        let apps = settings.store.apps().unwrap();
        let epoch: u64 = settings.store.get("catalog_epoch").unwrap();
        settings.store.db.execute_batch(&format!(
            "CREATE TRIGGER reject_bulk BEFORE INSERT ON apps WHEN NEW.id='{}' BEGIN SELECT RAISE(ABORT, 'forced bulk write failure'); END;", rejected.id
        )).unwrap();
        let paths = vec![first.clone(), last.clone()];
        let result = import(settings, paths.clone());
        assert!(result.is_err());
        assert_eq!(settings.store.apps().unwrap(), apps);
        assert_eq!(protected_data(settings, anchor), before);
        assert_eq!(settings.store.get::<u64>("catalog_epoch").unwrap(), epoch);
        settings
            .store
            .db
            .execute_batch("DROP TRIGGER reject_bulk;")
            .unwrap();
        let (result, _) = import(settings, paths.clone()).unwrap();
        assert!(result.failed.is_empty());
        assert_eq!((result.created, result.updated), (2, 0));
        assert_eq!(
            settings.store.get::<u64>("catalog_epoch").unwrap(),
            epoch + 1
        );
        let (repeat, _) = import(settings, paths).unwrap();
        assert!(repeat.failed.is_empty());
        assert_eq!((repeat.created, repeat.updated), (0, 0));
        assert_eq!(
            settings.store.get::<u64>("catalog_epoch").unwrap(),
            epoch + 1
        );
        assert!(first.is_file() && last.is_file());
        assert_eq!(protected_data(settings, anchor), before);
    });
}
