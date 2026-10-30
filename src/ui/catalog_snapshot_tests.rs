use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_saved_catalog(count: usize, test: impl FnOnce(&mut Settings, &App)) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let home = std::env::temp_dir().join(format!(
        "desktop-pilot-saved-catalog-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&home).unwrap();
    let home = crate::profile::ordinary_path(std::fs::canonicalize(&home).unwrap());
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop).unwrap();
    let store = Store::open(&home).unwrap();
    store
        .set("test_desktop", &desktop.to_string_lossy())
        .unwrap();
    store.set("setup_complete", &true).unwrap();
    store
        .set(
            "config",
            &Config {
                slots: 3,
                learning_enabled: false,
                ..Default::default()
            },
        )
        .unwrap();

    let original = home.join("Rare original.url");
    let original_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid/Rare\r\n";
    std::fs::write(&original, original_bytes).unwrap();
    let mut rare = native::read_link(&original).unwrap();
    rare.policy = Policy::Always;
    rare.note = "Rare research workspace".into();
    // A saved custom source can be offline at first display; Settings must
    // expose its existing entries before the background worker rescans it.
    store
        .transaction(|| {
            store.put_app(&rare)?;
            for index in 0..count {
                let app = App {
                    id: format!("saved-{index:05}"),
                    name: format!("Editor {}", index % 12),
                    source: format!(r"Z:\Disconnected\Project\{index:05}.lnk"),
                    target: format!(r"Z:\Disconnected\App\{index:05}.exe"),
                    arguments: format!("--profile {}", index % 2),
                    note: if index == count / 2 {
                        "Unique saved research profile".into()
                    } else {
                        String::new()
                    },
                    policy: if index == 9 {
                        Policy::Never
                    } else {
                        Policy::Auto
                    },
                    valid: true,
                    ..Default::default()
                };
                store.put_app(&app)?;
            }
            Ok(())
        })
        .unwrap();
    store.observe(&rare.id, 180, true, false).unwrap();
    drop(store);

    crate::store::CATALOG_READS.with(|reads| reads.set(0));
    let mut settings = Settings::new(home.clone()).unwrap();
    crate::store::CATALOG_READS.with(|reads| {
        assert_eq!(
            reads.get(),
            1,
            "Opening saved catalog should parse each SQLite app row only once"
        );
    });
    test(&mut settings, &rare);
    assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
    drop(settings);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn reopening_large_saved_catalog_keeps_immediate_search_and_one_catalog_read() {
    for size in [100, 1000, 5000] {
        with_saved_catalog(size, |settings, rare| {
            assert_eq!(settings.apps.len(), size + 1);
            assert!(settings.preview_error.is_empty());
            let preview = settings.preview.as_ref().unwrap();
            assert!(preview.selected.contains(&rare.id));
            assert!(preview.added.contains(&rare.id));
            assert!(preview.removed.is_empty());
            assert!(settings.state.owned.is_empty());
            assert!(settings.apps.iter().any(|app| app.policy == Policy::Never));

            // Real Settings catalog query and profile differentiation while
            // the synthetic optional drive is still unavailable.
            settings.catalog.update(
                &settings.apps,
                &settings.ranking,
                "unique saved research profile",
                0,
                0,
                &HashSet::new(),
            );
            assert_eq!(settings.catalog.indices.len(), 1);
            let found = &settings.apps[settings.catalog.indices[0]];
            assert_eq!(found.note, "Unique saved research profile");
            assert!(found.arguments.starts_with("--profile "));

            // Compare the actual Store preview before/after refactoring;
            // selection and eligibility must be identical, not merely fast.
            let reference = engine::preview(&settings.store, true).unwrap();
            assert_eq!(preview.selected, reference.selected);
            assert_eq!(preview.added, reference.added);
            assert_eq!(preview.removed, reference.removed);
            let candidates = |ranked: &[Ranked]| {
                ranked
                    .iter()
                    .map(|app| (app.id.clone(), app.eligible, app.reason.clone()))
                    .collect::<Vec<_>>()
            };
            assert_eq!(candidates(&preview.ranking), candidates(&reference.ranking));

            crate::store::CATALOG_READS.with(|reads| reads.set(0));
            settings.reload().unwrap();
            crate::store::CATALOG_READS.with(|reads| {
                assert_eq!(reads.get(), 0, "Unchanged poll must reuse saved catalog")
            });
            // Explicit invalidation still executes the entire real snapshot path.
            settings.catalog_checkpoint = None;
            settings.reload().unwrap();
            crate::store::CATALOG_READS.with(|reads| {
                assert_eq!(reads.get(), 1, "Full refresh must parse the catalog once")
            });
            assert_eq!(settings.apps.len(), size + 1);
            assert_eq!(
                settings.preview.as_ref().unwrap().selected,
                reference.selected
            );
        });
    }
}

#[test]
fn invalid_preview_settings_preserve_saved_search_without_a_second_catalog_read() {
    with_saved_catalog(100, |settings, _rare| {
        let mut invalid = settings.store.get::<Config>("config").unwrap();
        invalid.slots = 0;
        settings.store.set("config", &invalid).unwrap();

        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        settings.reload().unwrap();
        crate::store::CATALOG_READS.with(|reads| assert_eq!(reads.get(), 1));
        assert!(settings.preview.is_none());
        assert!(!settings.preview_error.is_empty());
        settings.catalog.update(
            &settings.apps,
            &settings.ranking,
            "rare research workspace",
            0,
            0,
            &HashSet::new(),
        );
        assert_eq!(settings.catalog.indices.len(), 1);
        assert_eq!(settings.apps.len(), 101);
        assert!(settings.state.owned.is_empty());
    });
}

#[test]
fn unchanged_settings_refreshes_do_not_reparse_a_large_saved_catalog() {
    with_saved_catalog(1_000, |settings, rare| {
        let original = settings
            .preview
            .as_ref()
            .unwrap()
            .selected
            .contains(&rare.id);
        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        for _ in 0..12 {
            settings.reload().unwrap();
        }
        crate::store::CATALOG_READS.with(|reads| {
            assert_eq!(
                reads.get(),
                0,
                "An unchanged saved catalog must not be parsed every UI refresh"
            );
        });
        assert_eq!(settings.apps.len(), 1_001);
        assert_eq!(
            settings
                .preview
                .as_ref()
                .unwrap()
                .selected
                .contains(&rare.id),
            original
        );
    });
}

#[test]
fn settings_refresh_detects_both_external_and_same_connection_changes() {
    with_saved_catalog(100, |settings, rare| {
        let writer = Store::open(&settings.store.home).unwrap();
        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        for _ in 0..3 {
            settings.reload().unwrap();
        }
        let mut changed = writer.app(&rare.id).unwrap();
        changed.policy = Policy::Never;
        writer.put_app(&changed).unwrap();
        settings.reload().unwrap();
        assert_eq!(
            settings
                .apps
                .iter()
                .find(|app| app.id == rare.id)
                .unwrap()
                .policy,
            Policy::Never
        );
        assert!(
            !settings
                .preview
                .as_ref()
                .unwrap()
                .selected
                .contains(&rare.id)
        );
        crate::store::CATALOG_READS.with(|reads| assert_eq!(reads.get(), 1));

        // PRAGMA data_version does not change for writes on the same connection.
        // Protect immediate local rule edits as well as worker/tray changes.
        settings.store.policy(&rare.id, Policy::Always).unwrap();
        settings.reload().unwrap();
        assert_eq!(
            settings
                .apps
                .iter()
                .find(|app| app.id == rare.id)
                .unwrap()
                .policy,
            Policy::Always
        );
        assert!(
            settings
                .preview
                .as_ref()
                .unwrap()
                .selected
                .contains(&rare.id)
        );
        crate::store::CATALOG_READS.with(|reads| assert_eq!(reads.get(), 2));
    });
}

#[test]
fn settings_refresh_keeps_worker_status_and_completed_command_feedback_current() {
    with_saved_catalog(100, |settings, _| {
        let worker = Store::open(&settings.store.home).unwrap();
        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        for _ in 0..3 {
            settings.reload().unwrap();
        }
        worker
            .set("status", &"Changes paused: source unavailable")
            .unwrap();
        settings.reload().unwrap();
        assert_eq!(settings.status, "Changes paused: source unavailable");

        worker.command("apply").unwrap();
        let id: i64 = worker
            .db
            .query_row("SELECT MAX(id) FROM commands", [], |row| row.get(0))
            .unwrap();
        settings.pending_commands.push(PendingCommand {
            id,
            action: "apply".into(),
            submitted: Instant::now(),
        });
        settings.reload().unwrap();
        assert_eq!(settings.active_commands, 1);
        assert_eq!(settings.pending_commands.len(), 1);

        worker
            .db
            .execute(
                "UPDATE commands SET status='done',message='Applied safely' WHERE id=?1",
                [id],
            )
            .unwrap();
        settings.reload().unwrap();
        assert_eq!(settings.active_commands, 0);
        assert!(settings.pending_commands.is_empty());
        assert_eq!(settings.notice.as_ref().unwrap().text, "Applied safely");
    });
}

#[test]
fn elapsed_snapshot_checkpoint_rechecks_time_dependent_recommendations() {
    with_saved_catalog(100, |settings, rare| {
        let (version, changes, _) = settings.catalog_checkpoint.unwrap();
        settings.catalog_checkpoint =
            Some((version, changes, Instant::now() - Duration::from_secs(61)));
        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        settings.reload().unwrap();
        crate::store::CATALOG_READS.with(|reads| assert_eq!(reads.get(), 1));
        assert!(
            settings
                .preview
                .as_ref()
                .unwrap()
                .selected
                .contains(&rare.id)
        );
    });
}

/// A cold Settings window must display the last committed manual choices
/// immediately, even while another process holds the WAL writer for an edit.
/// It must then pick up the committed choice rather than retaining a stale
/// preview or pretending that the uncommitted edit was already applied.
#[test]
fn cold_start_with_busy_writer_keeps_saved_catalog_and_refreshes_after_commit() {
    with_saved_catalog(5_000, |settings, rare| {
        let home = settings.store.home.clone();
        let original_bytes = std::fs::read(&rare.source).unwrap();
        let writer = Store::open(&home).unwrap();
        let mut changed = writer.app(&rare.id).unwrap();
        changed.policy = Policy::Never;

        writer.db.execute_batch("BEGIN IMMEDIATE").unwrap();
        writer.put_app(&changed).unwrap();
        // The writer has not committed: the real Settings startup must remain
        // readable and must not disclose the incomplete rule transition.
        crate::store::CATALOG_READS.with(|reads| reads.set(0));
        let mut fresh = Settings::new(home)
            .expect("A complete saved catalog must open while another connection writes");
        crate::store::CATALOG_READS.with(|reads| {
            assert_eq!(reads.get(), 1, "Cold start must parse the catalog once");
        });
        assert_eq!(fresh.apps.len(), 5_001);
        let saved = fresh.apps.iter().find(|app| app.id == rare.id).unwrap();
        assert_eq!(saved.policy, Policy::Always);
        assert!(fresh.preview.as_ref().unwrap().selected.contains(&rare.id));
        fresh.catalog.update(
            &fresh.apps,
            &fresh.ranking,
            "unique saved research profile",
            0,
            0,
            &HashSet::new(),
        );
        assert_eq!(fresh.catalog.indices.len(), 1);
        assert_eq!(
            fresh.apps[fresh.catalog.indices[0]].note,
            "Unique saved research profile"
        );
        assert_eq!(std::fs::read(&rare.source).unwrap(), original_bytes);

        writer.db.execute_batch("COMMIT").unwrap();
        fresh.reload().unwrap();
        assert_eq!(
            fresh
                .apps
                .iter()
                .find(|app| app.id == rare.id)
                .unwrap()
                .policy,
            Policy::Never,
            "The newly committed manual prohibition must appear on refresh"
        );
        assert!(!fresh.preview.as_ref().unwrap().selected.contains(&rare.id));
        assert!(fresh.state.owned.is_empty());
        assert_eq!(std::fs::read(&rare.source).unwrap(), original_bytes);
    });
}
