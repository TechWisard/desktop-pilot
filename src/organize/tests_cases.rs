use super::*;
use crate::model::{App, Owned, hash};
use anyhow::bail;
use std::{cell::Cell, fs as disk};

thread_local! {
    static FAIL_AFTER: Cell<Option<usize>> = const { Cell::new(None) };
}

pub(super) fn checkpoint(index: usize) -> Result<()> {
    if FAIL_AFTER.with(|value| value.get() == Some(index)) {
        FAIL_AFTER.with(|value| value.set(None));
        bail!("Injected interruption after a completed rename");
    }
    Ok(())
}

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = disk::remove_dir_all(&self.0);
    }
}

struct Fixture {
    home: PathBuf,
    desktop: PathBuf,
    store: Store,
    _com: native::Com,
    // Fields are dropped in declaration order: SQLite closes before cleanup.
    _cleanup: Cleanup,
}
impl Fixture {
    fn new() -> Self {
        let home =
            std::env::temp_dir().join(format!("test-organization-{}", fs::operation_id().unwrap()));
        let store = Store::open(&home).unwrap();
        let desktop = home.join("Desktop");
        disk::create_dir(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("config", &Config::default()).unwrap();
        Self {
            home: home.clone(),
            desktop,
            store,
            _com: native::Com::new().unwrap(),
            _cleanup: Cleanup(home),
        }
    }
    fn url(&self, name: &str, url: &str) -> App {
        let path = self.desktop.join(name);
        disk::write(&path, format!("[InternetShortcut]\r\nURL={url}\r\n")).unwrap();
        let app = native::read_link(&path).unwrap();
        self.store.put_app(&app).unwrap();
        app
    }
    fn two_sites(&self) -> (App, App) {
        (
            self.url("Alpha.url", "https://example.invalid/Alpha"),
            self.url("Beta.url", "https://example.invalid/Beta"),
        )
    }
    fn plan(&self, files: bool) -> Plan {
        analyze(
            &self.store,
            Options {
                include_files: files,
            },
            &Config::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }
    fn apply(&self, plan: &Plan) -> Outcome {
        apply(&self.store, plan, &AtomicBool::new(false)).unwrap()
    }
    fn last(&self) -> String {
        status(&self.store).unwrap().undo.unwrap()
    }
}

#[test]
fn plan_is_read_only_and_apply_undo_preserve_identity_history_and_rules() {
    let f = Fixture::new();
    let (a, b) = f.two_sites();
    f.store.observe(&a.id, 321, true, false).unwrap();
    let days = serde_json::to_value(f.store.days().unwrap()).unwrap();
    let mut saved = a.clone();
    saved.note = "Keep this note".into();
    saved.seed = 10.0;
    f.store.put_app(&saved).unwrap();
    let a_stamp = fs::inspect(Path::new(&a.source), true).unwrap();
    let before: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    let plan = f.plan(false);
    assert_eq!(plan.summary(), (2, 1, 1));
    assert!(plan.will_move(&plan.items[0]));
    assert_eq!(disk::read_dir(&f.desktop).unwrap().count(), 2);
    let after: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after);
    let outcome = f.apply(&plan);
    assert_eq!(outcome.moved, 2);
    let relocated = f.store.app(&a.id).unwrap();
    assert_eq!(relocated.note, saved.note);
    assert_eq!(relocated.seed, saved.seed);
    assert_eq!(relocated.identity(), saved.identity());
    assert_eq!(
        fs::inspect(Path::new(&relocated.source), true).unwrap(),
        a_stamp
    );
    assert_eq!(serde_json::to_value(f.store.days().unwrap()).unwrap(), days);
    assert!(f.plan(false).items.is_empty());
    undo(&f.store, &f.last()).unwrap();
    assert_eq!(f.store.app(&a.id).unwrap(), saved);
    assert_eq!(f.store.app(&b.id).unwrap(), b);
    assert_eq!(fs::inspect(Path::new(&a.source), true).unwrap(), a_stamp);
    assert_eq!(disk::read_dir(&f.desktop).unwrap().count(), 2);
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn defaults_leave_documents_projects_code_and_manual_choices_untouched() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    f.store.policy(&a.id, Policy::Always).unwrap();
    disk::write(f.desktop.join("notes.txt"), b"notes").unwrap();
    disk::write(f.desktop.join("script.ps1"), b"script").unwrap();
    disk::write(f.desktop.join("book.xlsx"), b"linked spreadsheet").unwrap();
    disk::create_dir(f.desktop.join("Project")).unwrap();
    disk::write(f.desktop.join("Project/asset.png"), b"project asset").unwrap();
    let plan = f.plan(false);
    assert_eq!(plan.summary(), (0, 0, 0));
    assert_eq!(plan.items.len(), 1);
    assert_eq!(plan.skipped.len(), 5);
    assert!(!plan.will_move(&plan.items[0]));
    let plan = f.plan(true);
    assert!(plan.items.iter().any(|i| i.name == "notes.txt"));
    assert!(!plan.items.iter().any(|i| matches!(
        i.name.as_str(),
        "script.ps1" | "book.xlsx" | "Project" | "Alpha.url"
    )));
}

#[test]
fn literal_percent_names_keep_photo_and_its_xmp_sidecar_together() {
    let f = Fixture::new();
    // Both the parent and filename are real, already materialized filesystem
    // names. Neither is an environment-variable template.
    let literal_desktop = f.desktop.join("%USERNAME%");
    disk::create_dir(&literal_desktop).unwrap();
    f.store
        .set("test_desktop", &literal_desktop.to_string_lossy())
        .unwrap();
    for (name, content) in [
        ("Photo%USERNAME%.jpg", "original image"),
        ("Photo%USERNAME%.jpg.xmp", "local editing metadata"),
        ("Standalone 1.jpg", "first independent image"),
        ("Standalone 2.jpg", "second independent image"),
    ] {
        disk::write(literal_desktop.join(name), content).unwrap();
    }

    let plan = f.plan(true);
    assert!(
        plan.skipped.iter().any(|item| {
            item.name == "Photo%USERNAME%.jpg" && item.reason.contains("Associated file set")
        }),
        "the real analyzer must protect a photo with an adjacent sidecar"
    );
    assert!(
        !plan
            .items
            .iter()
            .any(|item| item.name == "Photo%USERNAME%.jpg")
    );
    assert_eq!(plan.summary(), (2, 1, 1));
    let outcome = f.apply(&plan);
    assert_eq!(outcome.moved, 2);
    assert_eq!(
        disk::read(literal_desktop.join("Photo%USERNAME%.jpg")).unwrap(),
        b"original image"
    );
    assert_eq!(
        disk::read(literal_desktop.join("Photo%USERNAME%.jpg.xmp")).unwrap(),
        b"local editing metadata"
    );
    assert!(literal_desktop.join("Images/Standalone 1.jpg").is_file());
    assert!(literal_desktop.join("Images/Standalone 2.jpg").is_file());

    undo(&f.store, &f.last()).unwrap();
    assert!(literal_desktop.join("Standalone 1.jpg").is_file());
    assert!(literal_desktop.join("Standalone 2.jpg").is_file());
    assert!(!literal_desktop.join("Images").exists());
    assert_eq!(
        disk::read(literal_desktop.join("Photo%USERNAME%.jpg")).unwrap(),
        b"original image"
    );
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn optional_media_organization_keeps_editor_and_camera_companions_in_place() {
    let f = Fixture::new();
    let entries = [
        ("Edited.jpg", "photograph"),
        ("Edited.acr", "Lightroom editing adjustments"),
        ("GX012345.mp4", "camera master"),
        ("GX012345.thm", "camera thumbnail"),
        ("GL012345.lrv", "camera preview"),
        ("Standalone 1.jpg", "independent image one"),
        ("Standalone 2.jpg", "independent image two"),
        ("Independent 1.mp4", "independent video one"),
        ("Independent 2.mp4", "independent video two"),
    ];
    for (name, contents) in entries {
        disk::write(f.desktop.join(name), contents).unwrap();
    }

    // The ordinary-file pass is deliberately opt-in. The production analyzer
    // must never split a master and adjacent edits/previews.
    let default_plan = f.plan(false);
    assert!(default_plan.items.is_empty());
    let plan = f.plan(true);
    for protected in ["Edited.jpg", "GX012345.mp4"] {
        assert!(
            plan.skipped.iter().any(|item| {
                item.name == protected && item.reason.contains("Associated file set")
            }),
            "{protected} should stay beside its companion"
        );
        assert!(!plan.items.iter().any(|item| item.name == protected));
    }
    assert_eq!(plan.summary(), (4, 2, 2));
    let outcome = f.apply(&plan);
    assert_eq!(outcome.moved, 4);
    for (name, contents) in entries {
        let path = f.desktop.join(name);
        if name.starts_with("Standalone") || name.starts_with("Independent") {
            assert!(!path.exists());
        } else {
            assert_eq!(disk::read(path).unwrap(), contents.as_bytes());
        }
    }
    for name in ["Standalone 1.jpg", "Standalone 2.jpg"] {
        assert!(f.desktop.join("Images").join(name).is_file());
    }
    for name in ["Independent 1.mp4", "Independent 2.mp4"] {
        assert!(f.desktop.join("Media").join(name).is_file());
    }

    undo(&f.store, &f.last()).unwrap();
    for (name, contents) in entries {
        assert_eq!(
            disk::read(f.desktop.join(name)).unwrap(),
            contents.as_bytes()
        );
    }
    assert!(!f.desktop.join("Images").exists());
    assert!(!f.desktop.join("Media").exists());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn opt_in_files_are_renamed_not_copied_and_same_file_edits_survive_undo() {
    let f = Fixture::new();
    for name in ["one.txt", "two.pdf"] {
        disk::write(f.desktop.join(name), b"original").unwrap();
    }
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (2, 1, 1));
    f.apply(&plan);
    let destination = f.desktop.join("Documents/one.txt");
    let mut file = disk::OpenOptions::new()
        .append(true)
        .open(&destination)
        .unwrap();
    std::io::Write::write_all(&mut file, b" edited").unwrap();
    drop(file);
    undo(&f.store, &f.last()).unwrap();
    assert_eq!(
        disk::read(f.desktop.join("one.txt")).unwrap(),
        b"original edited"
    );
    assert_eq!(disk::read(f.desktop.join("two.pdf")).unwrap(), b"original");
}

#[test]
fn stale_source_or_folder_collision_rejects_plan_before_any_move() {
    for collision in [false, true] {
        let f = Fixture::new();
        f.two_sites();
        let plan = f.plan(false);
        if collision {
            disk::create_dir(f.desktop.join("Sites")).unwrap();
        } else {
            disk::write(f.desktop.join("Beta.url"), b"changed").unwrap();
        }
        assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
        assert!(f.desktop.join("Alpha.url").is_file());
        assert!(f.desktop.join("Beta.url").is_file());
        assert!(!status(&f.store).unwrap().pending);
        assert!(status(&f.store).unwrap().undo.is_none());
    }
}

#[test]
fn foreign_category_is_never_merged_or_overwritten() {
    let f = Fixture::new();
    f.two_sites();
    disk::create_dir(f.desktop.join("Sites")).unwrap();
    disk::write(f.desktop.join("Sites/sentinel.txt"), b"foreign").unwrap();
    let plan = f.plan(false);
    assert_eq!(plan.destination(&plan.items[0]), "Sites (Desktop Pilot)");
    f.apply(&plan);
    undo(&f.store, &f.last()).unwrap();
    assert_eq!(
        disk::read(f.desktop.join("Sites/sentinel.txt")).unwrap(),
        b"foreign"
    );
}

#[test]
fn partial_apply_failure_restores_files_and_catalog() {
    let f = Fixture::new();
    f.two_sites();
    let before = f.store.apps().unwrap();
    let plan = f.plan(false);
    FAIL_AFTER.with(|v| v.set(Some(0)));
    assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    assert_eq!(f.store.apps().unwrap(), before);
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
    assert!(!status(&f.store).unwrap().pending);
}

fn simulate_interruption(f: &Fixture, plan: &Plan) -> String {
    let id = fs::operation_id().unwrap();
    let mut journal = Journal {
        id: id.clone(),
        root: plan.root.clone(),
        root_id: plan.root_id.clone(),
        phase: Phase::Prepared,
        movements: plan.movements(),
        folders: plan.folders.clone(),
        layout: None,
    };
    f.store
        .transaction(|| {
            install_catalog_barrier(&f.store)?;
            save(&f.store, &journal)?;
            f.store.set(ACTIVE, &Some(&id))
        })
        .unwrap();
    disk::create_dir(f.desktop.join("Sites")).unwrap();
    let directory = fs::directory(&f.desktop.join("Sites")).unwrap();
    journal.folders[0].id = Some(fs::identity(&directory).unwrap());
    save(&f.store, &journal).unwrap();
    let m = &journal.movements[0];
    fs::rename_file(
        &f.desktop.join(&m.item.name),
        &directory,
        &m.item.name,
        &m.item.stamp,
        true,
    )
    .unwrap();
    id
}

#[test]
fn interrupted_apply_can_be_explicitly_accepted_without_moving_any_other_files() {
    let f = Fixture::new();
    let (alpha, beta) = f.two_sites();
    let original = f.store.apps().unwrap();
    let operation = simulate_interruption(&f, &f.plan(false));
    assert!(status(&f.store).unwrap().pending);
    assert!(f.desktop.join("Sites/Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());

    // Explicitly accept the interrupted mixed state; only verified catalog
    // paths change. No other desktop entry, preference or file is overwritten.
    accept_current(&f.store, &operation).unwrap();
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Accepted);
    assert_eq!(f.store.app(&beta.id).unwrap().source, beta.source);
    let accepted_source = f.store.app(&alpha.id).unwrap().source;
    let moved_id = fs::inspect(&f.desktop.join("Sites/Alpha.url"), true)
        .unwrap()
        .id;
    assert!(
        fs::matches_id(Path::new(&accepted_source), &moved_id).unwrap(),
        "Catalog path must refer to the moved file even across Windows 8.3 aliases"
    );
    assert!(f.desktop.join("Sites/Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Alpha.url").exists());
    assert_eq!(f.store.apps().unwrap().len(), original.len());
    assert!(ensure_ready(&f.store).is_ok());
    f.store.policy(&beta.id, Policy::Never).unwrap();
    assert_eq!(f.store.app(&beta.id).unwrap().policy, Policy::Never);
}

#[test]
fn accept_current_keeps_recovery_pending_for_missing_or_replaced_originals() {
    // Recovery must not silently release the catalog barrier when a file
    // disappeared or a foreign object took its place after an interruption.
    for replaced_moved_file in [false, true] {
        let f = Fixture::new();
        let (alpha, beta) = f.two_sites();
        let operation = simulate_interruption(&f, &f.plan(false));
        let affected = if replaced_moved_file {
            f.desktop.join("Sites/Alpha.url")
        } else {
            PathBuf::from(&beta.source)
        };
        let saved = f.home.join("Saved original.url");
        let original_id = fs::inspect(&affected, true).unwrap().id;
        disk::rename(&affected, &saved).unwrap();
        if replaced_moved_file {
            disk::write(&affected, b"unrelated replacement").unwrap();
        }
        let catalog = f.store.apps().unwrap();
        let error = accept_current(&f.store, &operation).unwrap_err();
        assert!(
            format!("{error:#}").contains("missing from both recorded locations"),
            "{error:#}"
        );
        assert!(status(&f.store).unwrap().pending);
        assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Prepared);
        assert!(ensure_ready(&f.store).is_err());
        assert_eq!(f.store.apps().unwrap(), catalog);
        assert_eq!(fs::inspect(&saved, true).unwrap().id, original_id);
        if replaced_moved_file {
            assert_eq!(disk::read(&affected).unwrap(), b"unrelated replacement");
            disk::rename(&affected, f.home.join("Preserved replacement.url")).unwrap();
        }
        disk::rename(&saved, &affected).unwrap();
        accept_current(&f.store, &operation).unwrap();
        assert!(!status(&f.store).unwrap().pending);
        assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Accepted);
        assert!(ensure_ready(&f.store).is_ok());
        assert_eq!(fs::inspect(&affected, true).unwrap().id, original_id);
        assert_eq!(f.store.app(&beta.id).unwrap().source, beta.source);
        let actual_alpha = f.store.app(&alpha.id).unwrap().source;
        assert!(
            fs::matches_id(
                Path::new(&actual_alpha),
                &fs::inspect(&f.desktop.join("Sites/Alpha.url"), true)
                    .unwrap()
                    .id
            )
            .unwrap()
        );
    }
}

#[test]
fn reopened_profile_recovers_a_rename_and_blocks_stale_catalog_writers() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    let id = simulate_interruption(&f, &f.plan(false));
    let reopened = Store::open(&f.home).unwrap();
    assert!(status(&reopened).unwrap().pending);
    assert!(ensure_ready(&reopened).is_err());
    assert!(reopened.policy(&a.id, Policy::Never).is_err());
    assert!(
        reopened
            .db
            .execute(
                "INSERT INTO transactions VALUES('blocked',1,'prepared','{}')",
                []
            )
            .is_err()
    );
    undo(&reopened, &id).unwrap();
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!status(&reopened).unwrap().pending);
    assert_eq!(reopened.app(&a.id).unwrap(), a);
    reopened.policy(&a.id, Policy::Never).unwrap();
}

#[test]
fn undo_conflict_preserves_both_objects_and_can_be_retried() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    f.apply(&f.plan(false));
    let id = f.last();
    let destination = f.store.app(&a.id).unwrap().source;
    disk::write(&a.source, b"new user file").unwrap();
    assert!(undo(&f.store, &id).is_err());
    assert_eq!(disk::read(&a.source).unwrap(), b"new user file");
    assert!(Path::new(&destination).is_file());
    // A known foreign collision is detected before a recovery barrier or any
    // partial move. The original operation stays available for a safe retry.
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(load(&f.store, &id).unwrap().phase, Phase::Committed);
    assert_eq!(status(&f.store).unwrap().undo.as_deref(), Some(id.as_str()));
    assert!(!f.desktop.join("Beta.url").exists());
    disk::rename(&a.source, f.desktop.join("new-user-file.txt")).unwrap();
    undo(&f.store, &id).unwrap();
    assert_eq!(
        disk::read(f.desktop.join("new-user-file.txt")).unwrap(),
        b"new user file"
    );
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn replacement_with_identical_bytes_is_not_moved_by_undo() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    f.apply(&f.plan(false));
    let id = f.last();
    let destination = PathBuf::from(f.store.app(&a.id).unwrap().source);
    let bytes = disk::read(&destination).unwrap();
    disk::rename(&destination, f.desktop.join("retained-original.url")).unwrap();
    disk::write(&destination, &bytes).unwrap();
    let error = undo(&f.store, &id).unwrap_err();
    assert!(
        format!("{error:#}").contains("organized object is missing or has been replaced"),
        "{error:#}"
    );
    assert_eq!(disk::read(&destination).unwrap(), bytes);
    assert!(f.desktop.join("retained-original.url").is_file());
    // Identical bytes do not confer ownership. The new preflight leaves the
    // original journal Committed, so ordinary Undo can be retried safely.
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(load(&f.store, &id).unwrap().phase, Phase::Committed);
    disk::rename(&destination, f.desktop.join("user-replacement.url")).unwrap();
    disk::rename(f.desktop.join("retained-original.url"), &destination).unwrap();
    assert_eq!(undo(&f.store, &id).unwrap().restored, 2);
    assert!(Path::new(&a.source).is_file());
    assert_eq!(
        disk::read(f.desktop.join("user-replacement.url")).unwrap(),
        bytes
    );
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn undo_preserves_new_objects_inside_created_folders() {
    let f = Fixture::new();
    f.two_sites();
    f.apply(&f.plan(false));
    disk::write(f.desktop.join("Sites/new.txt"), b"new content").unwrap();
    let result = undo(&f.store, &f.last()).unwrap();
    assert!(!result.warnings.is_empty());
    assert_eq!(
        disk::read(f.desktop.join("Sites/new.txt")).unwrap(),
        b"new content"
    );
}

#[test]
fn replaced_category_directory_is_not_trusted() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    f.apply(&f.plan(false));
    let id = f.last();
    disk::rename(f.desktop.join("Sites"), f.desktop.join("SavedSites")).unwrap();
    disk::create_dir(f.desktop.join("Sites")).unwrap();
    disk::write(f.desktop.join("Sites/Alpha.url"), b"replacement").unwrap();
    assert!(undo(&f.store, &id).is_err());
    assert_eq!(
        disk::read(f.desktop.join("Sites/Alpha.url")).unwrap(),
        b"replacement"
    );
    assert!(f.desktop.join("SavedSites/Alpha.url").is_file());
    assert!(!Path::new(&a.source).exists());
}

#[test]
fn cancellation_before_apply_changes_nothing() {
    let f = Fixture::new();
    f.two_sites();
    let plan = f.plan(false);
    assert!(apply(&f.store, &plan, &AtomicBool::new(true)).is_err());
    assert_eq!(disk::read_dir(&f.desktop).unwrap().count(), 2);
    assert!(status(&f.store).unwrap().undo.is_none());
}

#[test]
fn exclusions_are_remembered_and_can_be_explicitly_reset() {
    let f = Fixture::new();
    f.two_sites();
    let keep = f.url("Keep.url", "https://example.invalid/Keep");
    let mut plan = f.plan(false);
    plan.items
        .iter_mut()
        .find(|i| i.name == "Keep.url")
        .unwrap()
        .selected = false;
    f.apply(&plan);
    assert!(Path::new(&keep.source).is_file());
    let repeated = f.plan(false);
    assert!(repeated.items.is_empty());
    assert!(repeated.skipped.iter().any(|s| s.name == "Keep.url"));
    clear_exclusions(&f.store).unwrap();
    let repeated = f.plan(false);
    assert!(repeated.items.iter().any(|i| i.name == "Keep.url"));
}

#[test]
fn profiles_keep_distinct_arguments_and_launch_metadata() {
    let f = Fixture::new();
    let executable = std::env::current_exe().unwrap();
    let mut originals = Vec::new();
    for profile in ["work", "personal"] {
        let path = f.desktop.join(format!("Browser-{profile}.lnk"));
        native::make_link(
            &path,
            &executable,
            &format!("--profile {profile}"),
            "",
            0,
            "inert test",
        )
        .unwrap();
        let app = native::read_link(&path).unwrap();
        assert_eq!(metadata::read(&path).unwrap().identity(), app.identity());
        f.store.put_app(&app).unwrap();
        originals.push(app);
    }
    assert_ne!(originals[0].id, originals[1].id);
    f.apply(&f.plan(false));
    for original in &originals {
        let relocated = f.store.app(&original.id).unwrap();
        assert_eq!(relocated.identity(), original.identity());
        assert_eq!(
            native::read_link(Path::new(&relocated.source))
                .unwrap()
                .identity(),
            original.identity()
        );
    }
    undo(&f.store, &f.last()).unwrap();
    for original in originals {
        assert_eq!(f.store.app(&original.id).unwrap(), original);
    }
}

#[test]
fn a_managed_copy_is_excluded_after_manual_rename() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    let stamp = fs::inspect(Path::new(&a.source), true).unwrap();
    f.store
        .set(
            "state",
            &State {
                owned: vec![Owned {
                    app: a.id.clone(),
                    path: a.source.clone(),
                    sha: hash(&disk::read(&a.source).unwrap()),
                    position: Point::default(),
                    manual: true,
                    since: 1,
                    file_id: Some(stamp.id),
                }],
                ..Default::default()
            },
        )
        .unwrap();
    disk::rename(&a.source, f.desktop.join("Renamed.url")).unwrap();
    assert!(!f.plan(false).items.iter().any(|i| i.name == "Renamed.url"));
}

#[test]
fn unfinished_placement_and_a_running_worker_block_file_changes() {
    let f = Fixture::new();
    f.two_sites();
    let plan = f.plan(false);
    {
        let _worker = native::exclusive("background", &f.home).unwrap();
        assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    }
    f.store
        .db
        .execute(
            "INSERT INTO transactions VALUES('existing',1,'prepared','{}')",
            [],
        )
        .unwrap();
    assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    assert_eq!(disk::read_dir(&f.desktop).unwrap().count(), 2);
}

#[test]
fn database_rejection_after_moves_rolls_back_the_whole_batch() {
    let f = Fixture::new();
    f.two_sites();
    let plan = f.plan(false);
    f.store.db.execute_batch("CREATE TRIGGER reject_relocation BEFORE UPDATE ON apps BEGIN SELECT RAISE(ABORT, 'simulated write failure'); END;").unwrap();
    assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!status(&f.store).unwrap().pending);
    f.store
        .db
        .execute_batch("DROP TRIGGER reject_relocation")
        .unwrap();
    assert_eq!(f.apply(&f.plan(false)).moved, 2);
}

#[test]
fn an_owned_folder_is_reused_and_undo_remains_ordered() {
    let f = Fixture::new();
    f.two_sites();
    f.apply(&f.plan(false));
    let older = f.last();
    f.url("New.url", "https://example.invalid/New");
    let plan = f.plan(false);
    assert_eq!(plan.summary(), (1, 0, 1));
    f.apply(&plan);
    assert!(undo(&f.store, &older).is_err());
    undo(&f.store, &f.last()).unwrap();
    assert!(f.desktop.join("Sites/Alpha.url").is_file());
    assert!(f.desktop.join("New.url").is_file());
}

#[test]
fn new_manual_rules_invalidate_an_old_plan() {
    let f = Fixture::new();
    let (a, _) = f.two_sites();
    let plan = f.plan(false);
    f.store.policy(&a.id, Policy::Always).unwrap();
    assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    assert!(Path::new(&a.source).is_file());
    assert!(status(&f.store).unwrap().undo.is_none());
}

#[test]
fn known_file_dependencies_are_kept_even_when_files_are_enabled() {
    let f = Fixture::new();
    let target = f.desktop.join("linked.pdf");
    disk::write(&target, b"document").unwrap();
    disk::write(f.desktop.join("independent.pdf"), b"other").unwrap();
    let link = f.desktop.join("Document.lnk");
    native::make_link(&link, &target, "", "", 0, "document link").unwrap();
    let plan = f.plan(true);
    assert!(!plan.items.iter().any(|i| i.name == "linked.pdf"));
    assert!(plan.skipped.iter().any(|i| i.name == "linked.pdf"));
}

#[test]
fn projected_space_counts_folders_and_respects_deselection() {
    let f = Fixture::new();
    f.two_sites();
    f.url("Gamma.url", "https://example.invalid/Gamma");
    let mut plan = f.plan(false);
    plan.items[0].selected = false;
    for (index, item) in plan.items.iter_mut().enumerate() {
        item.position = Some(Point {
            x: 16 + 100 * index as i32,
            y: 2,
        });
    }
    plan.layout = Some(native::Layout {
        items: plan
            .items
            .iter()
            .map(|item| native::Item {
                path: f.desktop.join(&item.name).to_string_lossy().into_owned(),
                name: item.name.clone(),
                position: item.position.clone().unwrap(),
            })
            .collect(),
        spacing: Point { x: 100, y: 100 },
        origin: Point::default(),
        work: [0, 0, 300, 100],
        flags: 0,
        topology: "synthetic-grid".into(),
        icon_size: 32,
    });
    let config = Config {
        slots: 1,
        zone_col: 0,
        zone_row: 0,
        zone_columns: 1,
        ..Default::default()
    };
    assert!(find_free_zone(plan.layout.as_ref().unwrap(), &config).is_none());
    assert_eq!(plan.summary(), (2, 1, 1));
    // The real Windows runner exposes the same desktop through the short
    // RUNNER~1 and long Runneradmin paths; occupancy is resolved by file ID.
    let proposed = plan.projected_zone(&config).unwrap();
    assert_eq!(proposed.zone_col, 2);
    assert_eq!(proposed.zone_row, 0);

    // Another desktop may expose a different item under the same leaf name.
    // Never assume it was moved merely because its basename matches ours.
    let other = f.home.join("Public");
    disk::create_dir(&other).unwrap();
    let unrelated = other.join("Beta.url");
    disk::write(
        &unrelated,
        b"[InternetShortcut]\\r\\nURL=https://example.invalid/foreign\\r\\n",
    )
    .unwrap();
    let mut blocked = plan.clone();
    blocked.layout.as_mut().unwrap().items.push(native::Item {
        path: unrelated.to_string_lossy().into_owned(),
        name: "Beta.url".into(),
        position: Point { x: 216, y: 2 },
    });
    assert!(blocked.projected_zone(&config).is_none());
}

#[test]
fn projected_space_retains_public_desktop_hardlinks_and_recognizes_local_aliases() {
    let f = Fixture::new();
    f.two_sites();
    f.url("Gamma.url", "https://example.invalid/Gamma");
    let mut plan = f.plan(false);
    plan.items[0].selected = false;
    for (index, item) in plan.items.iter_mut().enumerate() {
        item.position = Some(Point {
            x: 16 + 100 * index as i32,
            y: 2,
        });
    }
    plan.layout = Some(native::Layout {
        items: plan
            .items
            .iter()
            .map(|item| native::Item {
                path: f.desktop.join(&item.name).to_string_lossy().into_owned(),
                name: item.name.clone(),
                position: item.position.clone().unwrap(),
            })
            .collect(),
        spacing: Point { x: 100, y: 100 },
        origin: Point::default(),
        work: [0, 0, 300, 100],
        flags: 0,
        topology: "synthetic-hardlink-grid".into(),
        icon_size: 32,
    });
    let config = Config {
        slots: 1,
        zone_col: 0,
        zone_row: 0,
        zone_columns: 1,
        ..Default::default()
    };
    // A different lexical spelling of the same desktop directory is still
    // removable from the projected layout, subject to verified identity.
    let alias = f.desktop.join(".").join("Gamma.url");
    let current_gamma = f.desktop.join("Gamma.url");
    assert_ne!(
        norm(&alias.to_string_lossy()),
        norm(&current_gamma.to_string_lossy())
    );
    let layout = plan.layout.as_mut().unwrap();
    layout
        .items
        .iter_mut()
        .find(|item| item.name == "Gamma.url")
        .unwrap()
        .path = alias.to_string_lossy().into_owned();
    assert_eq!(plan.projected_zone(&config).unwrap().zone_col, 2);

    // The user can expose a second Explorer icon in Public Desktop through an
    // NTFS hard link. It has the same file ID but a different parent directory.
    // The proposed free cell is therefore occupied and must remain blocked.
    let public = f.home.join("Public");
    disk::create_dir(&public).unwrap();
    let public_gamma = public.join("Gamma.url");
    disk::hard_link(&current_gamma, &public_gamma).unwrap();
    assert_eq!(
        fs::inspect(&current_gamma, false).unwrap().id,
        fs::inspect(&public_gamma, false).unwrap().id
    );
    plan.layout.as_mut().unwrap().items.push(native::Item {
        path: public_gamma.to_string_lossy().into_owned(),
        name: "Gamma.url".into(),
        position: Point { x: 216, y: 2 },
    });
    assert!(plan.projected_zone(&config).is_none());
}

#[test]
fn free_zone_search_matches_placement_for_dense_offset_and_negative_layouts() {
    let f = Fixture::new();
    for left in [-500, 0, 100] {
        for count in [0, 1, 5, 17, 32, 39, 40] {
            let layout = native::Layout {
                items: (0..count)
                    .map(|index| native::Item {
                        path: f
                            .desktop
                            .join(format!("icon-{index}"))
                            .display()
                            .to_string(),
                        name: format!("icon-{index}"),
                        position: Point {
                            x: left + 16 + (index / 5) * 100,
                            y: 2 + (index % 5) * 100,
                        },
                    })
                    .collect(),
                spacing: Point { x: 100, y: 100 },
                origin: Point::default(),
                work: [left, 0, left + 800, 500],
                flags: 0,
                topology: "offset-grid".into(),
                icon_size: 32,
            };
            for slots in [1, 4, 8] {
                let cfg = Config {
                    slots,
                    ..Default::default()
                };
                let result = find_free_zone(&layout, &cfg);
                let mut possible = false;
                for columns in [cfg.zone_columns, 1, 2, slots as i32] {
                    for col in 0..8 {
                        for row in 0..5 {
                            let candidate = Config {
                                zone_col: col,
                                zone_row: row,
                                zone_columns: columns,
                                ..cfg.clone()
                            };
                            possible |= crate::engine::reserve_positions(
                                &layout,
                                &candidate,
                                &State::default(),
                                &[],
                                slots,
                            )
                            .is_ok();
                        }
                    }
                }
                assert_eq!(result.is_some(), possible);
                if let Some(result) = result {
                    assert!(
                        crate::engine::reserve_positions(
                            &layout,
                            &result,
                            &State::default(),
                            &[],
                            slots,
                        )
                        .is_ok()
                    );
                }
            }
        }
    }
}

#[test]
fn double_location_hardlink_blocks_undo_before_moving_other_originals() {
    let f = Fixture::new();
    let (alpha, beta) = f.two_sites();
    let alpha_before = fs::inspect(Path::new(&alpha.source), true).unwrap();
    let beta_before = fs::inspect(Path::new(&beta.source), true).unwrap();
    f.apply(&f.plan(false));
    let operation = f.last();
    let catalog_before = f.store.apps().unwrap();

    // A hard link is a second directory entry for the very same original.
    // It must not be mistaken for a manual move out of the category folder.
    let organized_alpha = f.desktop.join("Sites/Alpha.url");
    disk::hard_link(&organized_alpha, &alpha.source).unwrap();
    assert_eq!(
        fs::inspect(Path::new(&alpha.source), true).unwrap().id,
        alpha_before.id
    );
    assert_eq!(
        fs::inspect(&organized_alpha, true).unwrap().id,
        alpha_before.id
    );
    let error = undo(&f.store, &operation).unwrap_err();
    assert!(format!("{error:#}").contains("both desktop and organized locations"));
    assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Committed);
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(
        status(&f.store).unwrap().undo.as_deref(),
        Some(operation.as_str())
    );
    assert_eq!(f.store.apps().unwrap(), catalog_before);
    assert!(f.desktop.join("Sites/Beta.url").is_file());
    assert!(!Path::new(&beta.source).exists());

    // Removing only our test link restores the unambiguous original location.
    disk::remove_file(&alpha.source).unwrap();
    assert_eq!(undo(&f.store, &operation).unwrap().restored, 2);
    assert_eq!(
        fs::inspect(Path::new(&alpha.source), true).unwrap(),
        alpha_before
    );
    assert_eq!(
        fs::inspect(Path::new(&beta.source), true).unwrap(),
        beta_before
    );
    assert!(!organized_alpha.exists());
    assert!(!f.desktop.join("Sites/Beta.url").exists());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn double_location_hardlink_keeps_interrupted_recovery_open_until_resolved() {
    let f = Fixture::new();
    let (alpha, beta) = f.two_sites();
    let alpha_before = fs::inspect(Path::new(&alpha.source), true).unwrap();
    let plan = f.plan(false);
    let operation = simulate_interruption(&f, &plan);
    let organized_alpha = f.desktop.join("Sites/Alpha.url");
    let apps_before = f.store.apps().unwrap();

    disk::hard_link(&organized_alpha, &alpha.source).unwrap();
    let error = accept_current(&f.store, &operation).unwrap_err();
    assert!(format!("{error:#}").contains("both desktop and organized locations"));
    assert!(status(&f.store).unwrap().pending);
    assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Prepared);
    assert_eq!(f.store.apps().unwrap(), apps_before);
    assert_eq!(fs::inspect(&organized_alpha, true).unwrap(), alpha_before);
    assert!(Path::new(&beta.source).is_file());

    disk::remove_file(&alpha.source).unwrap();
    accept_current(&f.store, &operation).unwrap();
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(load(&f.store, &operation).unwrap().phase, Phase::Accepted);
    // NTFS/Explorer may spell the same parent as a short 8.3 alias.
    // Verify the actual moved original rather than comparing path spelling.
    assert!(
        fs::matches_id(
            Path::new(&f.store.app(&alpha.id).unwrap().source),
            &alpha_before.id
        )
        .unwrap()
    );
    assert_eq!(f.store.app(&beta.id).unwrap().source, beta.source);
    assert_eq!(fs::inspect(&organized_alpha, true).unwrap(), alpha_before);
}

#[test]
fn analyzed_position_alias_must_belong_to_personal_desktop_not_public_hardlink() {
    let f = Fixture::new();
    let (original, _) = f.two_sites();
    let source = Path::new(&original.source);
    let stamp = fs::inspect(source, true).unwrap();
    let root_id = fs::identity(&fs::directory(&f.desktop).unwrap()).unwrap();

    // A Public Desktop icon may refer to the same NTFS file: file ID alone
    // does not establish which Explorer grid position belongs to this source.
    let public = f.home.join("Public");
    disk::create_dir(&public).unwrap();
    let foreign = public.join("Alpha.url");
    disk::hard_link(source, &foreign).unwrap();
    assert_eq!(fs::inspect(&foreign, true).unwrap().id, stamp.id);

    let exact = std::collections::HashMap::new();
    let mut aliases = std::collections::HashMap::from([(
        "alpha.url".to_owned(),
        vec![(foreign, Point { x: 111, y: 2 })],
    )]);
    assert_eq!(
        plan::source_position(&exact, &aliases, source, &root_id, &stamp.id),
        None,
        "The Public Desktop hard link must never supply a personal icon position"
    );

    // Explorer can also spell the same local parent directory differently.
    // Unlike the Public hard link, this verified path is a valid alias.
    let alias = f.desktop.join(".").join("Alpha.url");
    assert_ne!(
        norm(&alias.to_string_lossy()),
        norm(&source.to_string_lossy())
    );
    assert_eq!(fs::inspect(&alias, true).unwrap().id, stamp.id);
    let correct = Point { x: 211, y: 2 };
    aliases
        .get_mut("alpha.url")
        .unwrap()
        .push((alias, correct.clone()));
    assert_eq!(
        plan::source_position(&exact, &aliases, source, &root_id, &stamp.id),
        Some(correct.clone())
    );

    // Two matching view entries have no unambiguous owner. Retain the file
    // without claiming a saved layout position that might be the wrong icon.
    let second_alias = f.desktop.join("..").join("Desktop").join("Alpha.url");
    assert_eq!(fs::inspect(&second_alias, true).unwrap().id, stamp.id);
    aliases
        .get_mut("alpha.url")
        .unwrap()
        .push((second_alias, Point { x: 311, y: 2 }));
    assert_eq!(
        plan::source_position(&exact, &aliases, source, &root_id, &stamp.id),
        None
    );

    // An exact Explorer path remains authoritative even if alias entries are
    // ambiguous; no file, policy, history or Explorer position is modified.
    let exact = std::collections::HashMap::from([(
        norm(&source.to_string_lossy()),
        Point { x: 411, y: 2 },
    )]);
    assert_eq!(
        plan::source_position(&exact, &aliases, source, &root_id, &stamp.id),
        Some(Point { x: 411, y: 2 })
    );
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(public.join("Alpha.url").is_file());
    assert!(status(&f.store).unwrap().undo.is_none());
}

#[test]
fn organizing_literal_percent_named_shortcut_preserves_manual_sibling_and_undo() {
    let f = Fixture::new();
    let username = std::env::var("USERNAME").expect("Windows username is required");
    let resolved_name = format!("{username}.url");
    let literal = f.url("%USERNAME%.url", "https://example.invalid/literal");
    let manual = f.url(&resolved_name, "https://example.invalid/resolved");
    let other = f.url("Independent.url", "https://example.invalid/independent");
    assert_ne!(
        crate::model::norm_literal_path(&literal.source),
        crate::model::norm_literal_path(&manual.source)
    );
    f.store.policy(&manual.id, Policy::Never).unwrap();
    let literal_before = fs::inspect(Path::new(&literal.source), true).unwrap();
    let manual_before = fs::inspect(Path::new(&manual.source), true).unwrap();
    let manual_bytes = disk::read(&manual.source).unwrap();
    let history = serde_json::to_value(f.store.days().unwrap()).unwrap();

    // This is production analysis over real .url files and persisted rules.
    // Expanding a *literal filename* here used to hide both siblings.
    let plan = f.plan(false);
    assert!(plan.items.iter().any(|item| item.name == "%USERNAME%.url"));
    assert!(plan.items.iter().any(|item| item.name == "Independent.url"));
    assert!(!plan.items.iter().any(|item| item.name == resolved_name));
    assert_eq!(plan.summary(), (2, 1, 1));

    let outcome = f.apply(&plan);
    assert_eq!(outcome.moved, 2);
    assert_eq!(f.store.app(&manual.id).unwrap().source, manual.source);
    assert_eq!(f.store.app(&manual.id).unwrap().policy, Policy::Never);
    assert_eq!(disk::read(&manual.source).unwrap(), manual_bytes);
    assert_eq!(
        fs::inspect(&f.desktop.join("Sites/%USERNAME%.url"), true).unwrap(),
        literal_before
    );
    assert!(f.desktop.join("Sites/Independent.url").is_file());
    assert!(!Path::new(&other.source).exists());
    let operation = f.last();
    undo(&f.store, &operation).unwrap();
    assert_eq!(f.store.app(&literal.id).unwrap().source, literal.source);
    assert_eq!(f.store.app(&manual.id).unwrap().source, manual.source);
    assert_eq!(f.store.app(&manual.id).unwrap().policy, Policy::Never);
    assert_eq!(
        fs::inspect(Path::new(&literal.source), true).unwrap(),
        literal_before
    );
    assert_eq!(
        fs::inspect(Path::new(&manual.source), true).unwrap(),
        manual_before
    );
    assert_eq!(disk::read(&manual.source).unwrap(), manual_bytes);
    assert_eq!(
        serde_json::to_value(f.store.days().unwrap()).unwrap(),
        history
    );
    assert!(!status(&f.store).unwrap().pending);
}
#[test]
fn optional_camera_pairs_stay_together_while_independent_media_moves_and_undoes() {
    let f = Fixture::new();
    let entries = [
        ("DSC_1001.CR3", "raw original"),
        ("DSC_1001.JPG", "raw companion JPEG"),
        ("Live A.HEIC", "live still"),
        ("Live A.MOV", "live motion"),
        ("Live B.jpeg", "compatible still"),
        ("Live B.mov", "compatible motion"),
        ("Solo A.jpg", "independent photo one"),
        ("Solo B.jpeg", "independent photo two"),
        ("Solo C.mp4", "independent video one"),
        ("Solo D.mov", "independent video two"),
    ];
    for (name, bytes) in entries {
        disk::write(f.desktop.join(name), bytes).unwrap();
    }
    // Ordinary files are opt-in; this test exercises the real Windows
    // filesystem, analyzer, journal and Undo rather than a toy classifier.
    assert!(f.plan(false).items.is_empty());
    let plan = f.plan(true);
    for name in [
        "DSC_1001.JPG",
        "Live A.HEIC",
        "Live A.MOV",
        "Live B.jpeg",
        "Live B.mov",
    ] {
        assert!(
            plan.skipped
                .iter()
                .any(|item| { item.name == name && item.reason.contains("Associated file set") }),
            "{name} must not be offered without its companion: {:?}",
            plan.skipped
                .iter()
                .map(|item| &item.name)
                .collect::<Vec<_>>()
        );
        assert!(!plan.items.iter().any(|item| item.name == name));
    }
    assert_eq!(plan.summary(), (4, 2, 2));
    assert_eq!(f.apply(&plan).moved, 4);
    for (name, bytes) in entries {
        let original = f.desktop.join(name);
        if name.starts_with("Solo ") {
            assert!(!original.exists());
        } else {
            assert_eq!(disk::read(original).unwrap(), bytes.as_bytes());
        }
    }
    for name in ["Solo A.jpg", "Solo B.jpeg"] {
        assert!(f.desktop.join("Images").join(name).is_file());
    }
    for name in ["Solo C.mp4", "Solo D.mov"] {
        assert!(f.desktop.join("Media").join(name).is_file());
    }
    undo(&f.store, &f.last()).unwrap();
    for (name, bytes) in entries {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), bytes.as_bytes());
    }
    assert!(!f.desktop.join("Images").exists());
    assert!(!f.desktop.join("Media").exists());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn camera_companion_arriving_after_preview_rejects_all_moves_without_a_journal() {
    for (candidate, companion) in [
        ("DSC_2001.jpg", "DSC_2001.NEF"),
        ("IMG_2002.heic", "IMG_2002.mov"),
        ("IMG_2003.jpeg", "IMG_2003.MOV"),
    ] {
        let f = Fixture::new();
        let (alpha, beta) = f.two_sites();
        disk::write(f.desktop.join(candidate), b"still image").unwrap();
        disk::write(f.desktop.join("Independent.jpg"), b"unrelated image").unwrap();
        let plan = f.plan(true);
        assert_eq!(plan.summary(), (4, 2, 2));
        assert!(plan.items.iter().any(|item| item.name == candidate));
        disk::write(f.desktop.join(companion), b"late-arriving companion").unwrap();

        let error = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap_err();
        assert!(
            format!("{error:#}").contains("Associated file set"),
            "a late {companion} must invalidate the entire preview: {error:#}"
        );
        assert_eq!(
            disk::read(f.desktop.join(candidate)).unwrap(),
            b"still image"
        );
        assert_eq!(
            disk::read(f.desktop.join(companion)).unwrap(),
            b"late-arriving companion"
        );
        assert_eq!(
            disk::read(f.desktop.join("Independent.jpg")).unwrap(),
            b"unrelated image"
        );
        for name in ["Alpha.url", "Beta.url"] {
            assert!(f.desktop.join(name).is_file());
        }
        assert_eq!(f.store.app(&alpha.id).unwrap(), alpha);
        assert_eq!(f.store.app(&beta.id).unwrap(), beta);
        assert!(!f.desktop.join("Sites").exists());
        assert!(!f.desktop.join("Images").exists());
        assert!(!f.desktop.join("Media").exists());
        assert!(status(&f.store).unwrap().undo.is_none());
        assert!(!status(&f.store).unwrap().pending);
    }
}

#[test]
fn mixed_document_and_camera_dependencies_survive_apply_undo_and_late_arrival() {
    let f = Fixture::new();
    f.store.set("background_disabled", &true).unwrap();
    let (alpha, beta) = f.two_sites();
    f.store.observe(&alpha.id, 180, true, false).unwrap();
    let days = serde_json::to_value(f.store.days().unwrap()).unwrap();

    // These are ordinary, non-hidden files in a disposable Windows Desktop.
    // Each pair would be separated by the old optional-files organizer.
    let files = [
        ("Report.docx", "open Word document"),
        ("~$port.docx", "Word owner marker"),
        ("Letter.odt", "open LibreOffice document"),
        (".~lock.Letter.odt#", "LibreOffice owner marker"),
        ("DSC_3100.CR3", "camera RAW original"),
        ("DSC_3100.JPG", "camera JPEG companion"),
        ("Live Trip.HEIC", "Live Photo still"),
        ("Live Trip.MOV", "Live Photo movie"),
        ("Notes.docx", "independent document one"),
        ("Draft.docx", "independent document two"),
        ("Solo A.jpg", "independent still one"),
        ("Solo B.jpeg", "independent still two"),
    ];
    for &(name, value) in &files {
        disk::write(f.desktop.join(name), value).unwrap();
    }
    assert_eq!(f.plan(false).summary(), (2, 1, 1));
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (6, 3, 3));
    for name in [
        "Report.docx",
        "~$port.docx",
        "Letter.odt",
        ".~lock.Letter.odt#",
        "DSC_3100.JPG",
        "Live Trip.HEIC",
        "Live Trip.MOV",
    ] {
        assert!(
            !plan.items.iter().any(|item| item.name == name)
                && plan.skipped.iter().any(|item| item.name == name),
            "linked item was proposed for movement: {name}"
        );
    }

    let result = f.apply(&plan);
    assert_eq!(result.moved, 6);
    assert_eq!(result.folders_created, 3);
    for &(name, value) in &files[..8] {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), value.as_bytes());
    }
    for name in ["Notes.docx", "Draft.docx"] {
        assert!(f.desktop.join("Documents").join(name).is_file());
    }
    for name in ["Solo A.jpg", "Solo B.jpeg"] {
        assert!(f.desktop.join("Images").join(name).is_file());
    }
    assert!(f.desktop.join("Sites/Alpha.url").is_file());
    assert!(f.desktop.join("Sites/Beta.url").is_file());

    undo(&f.store, &f.last()).unwrap();
    for &(name, value) in &files {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), value.as_bytes());
    }
    for name in ["Sites", "Documents", "Images"] {
        assert!(!f.desktop.join(name).exists());
    }
    assert_eq!(f.store.app(&alpha.id).unwrap(), alpha);
    assert_eq!(f.store.app(&beta.id).unwrap(), beta);
    assert_eq!(serde_json::to_value(f.store.days().unwrap()).unwrap(), days);
    assert!(f.store.get::<bool>("background_disabled").unwrap());
    assert!(status(&f.store).unwrap().undo.is_none());

    // A second review contains a genuine independent photo. An RAW original
    // arriving after it was reviewed must abort the *whole* mixed batch.
    disk::write(f.desktop.join("Late Camera.jpg"), b"fresh still").unwrap();
    let stale = f.plan(true);
    assert!(
        stale
            .items
            .iter()
            .any(|item| item.name == "Late Camera.jpg")
    );
    disk::write(f.desktop.join("Late Camera.NEF"), b"late original").unwrap();
    let error = apply(&f.store, &stale, &AtomicBool::new(false)).unwrap_err();
    assert!(
        format!("{error:#}").contains("Associated file set"),
        "late camera companion must invalidate every proposed move: {error:#}"
    );
    for &(name, value) in &files {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), value.as_bytes());
    }
    assert_eq!(
        disk::read(f.desktop.join("Late Camera.jpg")).unwrap(),
        b"fresh still"
    );
    assert_eq!(
        disk::read(f.desktop.join("Late Camera.NEF")).unwrap(),
        b"late original"
    );
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    for name in ["Sites", "Documents", "Images"] {
        assert!(!f.desktop.join(name).exists());
    }
    assert!(status(&f.store).unwrap().undo.is_none());
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(serde_json::to_value(f.store.days().unwrap()).unwrap(), days);
    assert!(f.store.get::<bool>("background_disabled").unwrap());
}

#[test]
fn local_lyrics_and_audio_stay_together_through_apply_and_undo() {
    let f = Fixture::new();
    let paired: [(&str, &[u8]); 4] = [
        ("Песня%USERNAME%.mp3", b"audio master"),
        ("Песня%USERNAME%.lrc", b"[00:01.00]timed lyrics"),
        ("Album.flac", b"lossless audio"),
        ("Album.txt", b"plain lyrics"),
    ];
    for &(name, contents) in &paired {
        disk::write(f.desktop.join(name), contents).unwrap();
    }
    for name in [
        "Independent 1.mp3",
        "Independent 2.flac",
        "Notes 1.txt",
        "Notes 2.txt",
    ] {
        disk::write(f.desktop.join(name), format!("test fixture: {name}")).unwrap();
    }
    let important = f.url("Important.url", "https://example.invalid/important");
    f.store.policy(&important.id, Policy::Always).unwrap();
    f.store.observe(&important.id, 321, true, false).unwrap();
    let saved = f.store.app(&important.id).unwrap();
    let days = serde_json::to_value(f.store.days().unwrap()).unwrap();
    let before: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();

    assert!(f.plan(false).items.is_empty());
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (4, 2, 2));
    for &(name, contents) in &paired {
        let skipped = plan.skipped.iter().find(|item| item.name == name).unwrap();
        assert!(
            skipped.reason.contains("Associated file set"),
            "{name}: {}",
            skipped.reason
        );
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), contents);
    }
    let after: i64 = f
        .store
        .db
        .query_row("SELECT total_changes()", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after, "analysis must not write history");

    let media = plan.destination(
        plan.items
            .iter()
            .find(|item| item.name == "Independent 1.mp3")
            .unwrap(),
    );
    let docs = plan.destination(
        plan.items
            .iter()
            .find(|item| item.name == "Notes 1.txt")
            .unwrap(),
    );
    assert_eq!(f.apply(&plan).moved, 4);
    for &(name, contents) in &paired {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), contents);
    }
    for name in ["Independent 1.mp3", "Independent 2.flac"] {
        assert!(f.desktop.join(&media).join(name).is_file());
    }
    for name in ["Notes 1.txt", "Notes 2.txt"] {
        assert!(f.desktop.join(&docs).join(name).is_file());
    }
    undo(&f.store, &f.last()).unwrap();
    for name in [
        "Independent 1.mp3",
        "Independent 2.flac",
        "Notes 1.txt",
        "Notes 2.txt",
    ] {
        assert!(f.desktop.join(name).is_file());
    }
    for &(name, contents) in &paired {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), contents);
    }
    assert!(!f.desktop.join(media).exists());
    assert!(!f.desktop.join(docs).exists());
    assert_eq!(f.store.app(&important.id).unwrap(), saved);
    assert_eq!(serde_json::to_value(f.store.days().unwrap()).unwrap(), days);
    assert!(status(&f.store).unwrap().undo.is_none());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn late_lyrics_invalidate_a_preview_before_any_moves() {
    for companion in ["Late.lrc", "Late.txt"] {
        let f = Fixture::new();
        disk::write(f.desktop.join("Late.mp3"), b"original audio").unwrap();
        disk::write(f.desktop.join("Other.mp3"), b"other audio").unwrap();
        let plan = f.plan(true);
        assert_eq!(plan.summary(), (2, 1, 1));
        let folder = plan.destination(&plan.items[0]);
        disk::write(f.desktop.join(companion), b"new external lyrics").unwrap();

        let error = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap_err();
        assert!(
            format!("{error:#}").contains("Associated file set"),
            "a late {companion} must block the stale audio move: {error:#}"
        );
        assert_eq!(
            disk::read(f.desktop.join("Late.mp3")).unwrap(),
            b"original audio"
        );
        assert_eq!(
            disk::read(f.desktop.join("Other.mp3")).unwrap(),
            b"other audio"
        );
        assert_eq!(
            disk::read(f.desktop.join(companion)).unwrap(),
            b"new external lyrics"
        );
        assert!(!f.desktop.join(folder).exists());
        assert!(status(&f.store).unwrap().undo.is_none());
        assert!(!status(&f.store).unwrap().pending);
        let fresh = f.plan(true);
        assert!(fresh.skipped.iter().any(|item| {
            item.name == "Late.mp3" && item.reason.contains("Associated file set")
        }));
    }
}

#[test]
fn sami_subtitle_keeps_video_in_place_while_unrelated_media_moves_and_undoes() {
    let f = Fixture::new();
    let video = "Эпизод%USERNAME%.mkv";
    let subtitle = "Эпизод%USERNAME%.eng.forced.smi";
    disk::write(f.desktop.join(video), b"original video").unwrap();
    disk::write(f.desktop.join(subtitle), b"<SAMI>synthetic captions</SAMI>").unwrap();
    for name in [
        "Independent A.mp4",
        "Independent B.avi",
        "Notes A.txt",
        "Notes B.txt",
    ] {
        disk::write(f.desktop.join(name), format!("fixture: {name}")).unwrap();
    }
    let important = f.url("Important.url", "https://example.invalid/essential");
    f.store.policy(&important.id, Policy::Always).unwrap();
    f.store.observe(&important.id, 321, true, false).unwrap();
    let saved = f.store.app(&important.id).unwrap();
    let days = serde_json::to_value(f.store.days().unwrap()).unwrap();
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (4, 2, 2));
    let skipped = plan.skipped.iter().find(|item| item.name == video).unwrap();
    assert!(skipped.reason.contains("Associated file set"), "{skipped:?}");
    assert_eq!(disk::read(f.desktop.join(video)).unwrap(), b"original video");
    let media_folder = plan.destination(
        plan.items
            .iter()
            .find(|item| item.name == "Independent A.mp4")
            .unwrap(),
    );
    assert_eq!(f.apply(&plan).moved, 4);
    assert_eq!(disk::read(f.desktop.join(video)).unwrap(), b"original video");
    assert_eq!(
        disk::read(f.desktop.join(subtitle)).unwrap(),
        b"<SAMI>synthetic captions</SAMI>"
    );
    for name in ["Independent A.mp4", "Independent B.avi"] {
        assert!(f.desktop.join(&media_folder).join(name).is_file());
    }
    undo(&f.store, &f.last()).unwrap();
    for name in [
        "Independent A.mp4",
        "Independent B.avi",
        "Notes A.txt",
        "Notes B.txt",
    ] {
        assert!(f.desktop.join(name).is_file());
    }
    assert!(!f.desktop.join(media_folder).exists());
    assert_eq!(f.store.app(&important.id).unwrap(), saved);
    assert_eq!(serde_json::to_value(f.store.days().unwrap()).unwrap(), days);
    assert!(status(&f.store).unwrap().undo.is_none());
}

#[test]
fn late_sami_subtitle_blocks_stale_desktop_preview_before_any_move() {
    let f = Fixture::new();
    disk::write(f.desktop.join("Episode.mp4"), b"film").unwrap();
    disk::write(f.desktop.join("Other.mp4"), b"other").unwrap();
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (2, 1, 1));
    let folder = plan.destination(&plan.items[0]);
    disk::write(f.desktop.join("Episode.en.smi"), b"late subtitles").unwrap();
    let error = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap_err();
    assert!(
        format!("{error:#}").contains("Associated file set"),
        "{error:#}"
    );
    assert_eq!(disk::read(f.desktop.join("Episode.mp4")).unwrap(), b"film");
    assert_eq!(disk::read(f.desktop.join("Other.mp4")).unwrap(), b"other");
    assert_eq!(
        disk::read(f.desktop.join("Episode.en.smi")).unwrap(),
        b"late subtitles"
    );
    assert!(!f.desktop.join(folder).exists());
    assert!(status(&f.store).unwrap().undo.is_none());
    assert!(!status(&f.store).unwrap().pending);
    let refreshed = f.plan(true);
    assert!(refreshed.skipped.iter().any(|item| {
        item.name == "Episode.mp4" && item.reason.contains("Associated file set")
    }));
}
