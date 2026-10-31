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
    assert_eq!(disk::read(f.desktop.join("user-replacement.url")).unwrap(), bytes);
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
