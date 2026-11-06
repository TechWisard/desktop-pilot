//! Regression sequences use the actual organizer, Shell links and SQLite.
use super::*;
use std::fs as disk;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = disk::remove_dir_all(&self.0);
    }
}

struct Fixture {
    store: Store,
    desktop: PathBuf,
    _com: native::Com,
    _cleanup: Cleanup,
}
impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!(
            "test-organization-consent-{}",
            fs::operation_id().unwrap()
        ));
        let store = Store::open(&home).unwrap();
        let desktop = home.join("Desktop");
        disk::create_dir(&desktop).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("config", &Config::default()).unwrap();
        Self {
            store,
            desktop,
            _com: native::Com::new().unwrap(),
            _cleanup: Cleanup(home),
        }
    }

    fn plan(&self, include_files: bool) -> Plan {
        analyze(
            &self.store,
            Options { include_files },
            &Config::default(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }

    fn site(&self, name: &str) {
        let path = self.desktop.join(format!("{name}.url"));
        disk::write(
            &path,
            format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
        )
        .unwrap();
        self.store
            .put_app(&native::read_link(&path).unwrap())
            .unwrap();
    }

    fn argument_link(&self, argument: &str) {
        let tools = self.store.home.join("Tools");
        disk::create_dir_all(&tools).unwrap();
        let executable = tools.join("Reader.exe");
        disk::write(&executable, b"inert fixture; never execute").unwrap();
        let path = self.desktop.join("Open document.lnk");
        native::make_link(&path, &executable, argument, "", 0, "fixture").unwrap();
        self.store
            .put_app(&native::read_link(&path).unwrap())
            .unwrap();
    }
}

#[test]
fn old_preview_cannot_override_a_new_keep_choice_after_undo() {
    let f = Fixture::new();
    for name in ["Alpha", "Beta", "Gamma"] {
        f.site(name);
    }
    let old_preview = f.plan(false);
    let mut latest = old_preview.clone();
    latest
        .items
        .iter_mut()
        .find(|i| i.name == "Alpha.url")
        .unwrap()
        .selected = false;
    apply(&f.store, &latest, &AtomicBool::new(false)).unwrap();
    let id = status(&f.store).unwrap().undo.unwrap();
    undo(&f.store, &id).unwrap();
    let before = f.store.apps().unwrap();
    let history = f.store.get::<Vec<String>>(HISTORY).unwrap();
    let error = apply(&f.store, &old_preview, &AtomicBool::new(false))
        .expect_err("a stale preview must respect the newer keep-on-desktop choice");
    assert!(format!("{error:#}").contains("choice"), "{error:#}");
    assert_eq!(f.store.apps().unwrap(), before);
    assert_eq!(f.store.get::<Vec<String>>(HISTORY).unwrap(), history);
    assert!(!status(&f.store).unwrap().pending);
    assert_eq!(disk::read_dir(&f.desktop).unwrap().count(), 3);
    let refreshed = f.plan(false);
    assert!(!refreshed.items.iter().any(|i| i.name == "Alpha.url"));
    assert_eq!(refreshed.summary(), (2, 1, 1));
    clear_exclusions(&f.store).unwrap();
    assert_eq!(f.plan(false).summary(), (3, 1, 2));
}

#[test]
fn explicit_shortcut_argument_keeps_its_document_out_of_the_plan() {
    let f = Fixture::new();
    for name in ["Important report.txt", "Other.txt"] {
        disk::write(f.desktop.join(name), b"original document").unwrap();
    }
    let document = f.desktop.join("Important report.txt");
    f.argument_link(&format!("--open=\"{}\"", document.display()));
    let plan = f.plan(true);
    assert!(
        !plan.items.iter().any(|i| i.name == "Important report.txt"),
        "moving a document named in a shortcut argument would break the saved launch"
    );
    assert!(plan.items.iter().any(|i| i.name == "Other.txt"));
    assert!(
        plan.skipped
            .iter()
            .any(|i| i.name == "Important report.txt")
    );
    assert_eq!(disk::read(document).unwrap(), b"original document");
}

#[test]
fn aliased_desktop_working_directory_protects_opt_in_files_at_preview_and_apply() {
    let f = Fixture::new();
    for name in ["Important.txt", "Other.pdf"] {
        disk::write(f.desktop.join(name), b"user document").unwrap();
    }
    let old_preview = f.plan(true);
    assert_eq!(old_preview.summary(), (2, 1, 1));

    // A stored application can name this very desktop through an alternate
    // absolute spelling (8.3 / extended paths have the same problem). The
    // string comparison previously missed it and offered to move its files.
    let tools = f.store.home.join("Tools");
    disk::create_dir(&tools).unwrap();
    let executable = tools.join("Reader.exe");
    disk::write(&executable, b"fixture; never execute").unwrap();
    let link = tools.join("Reader.lnk");
    native::make_link(&link, &executable, "", "", 0, "fixture").unwrap();
    let mut app = native::read_link(&link).unwrap();
    let root_alias = f.desktop.join(".");
    for alias in [
        root_alias.to_string_lossy().into_owned(),
        format!(r"\\?\{}", root_alias.display()),
    ] {
        app.cwd = alias;
        assert_ne!(
            norm(&app.cwd),
            norm(&f.desktop.to_string_lossy()),
            "the regression requires two distinct textual spellings"
        );
        let ordinary = crate::profile::ordinary_path(PathBuf::from(&app.cwd));
        assert_eq!(
            fs::identity(&fs::directory(&ordinary).unwrap()).unwrap(),
            fs::identity(&fs::directory(&f.desktop).unwrap()).unwrap()
        );
        f.store.put_app(&app).unwrap();

        let fresh = f.plan(true);
        assert!(fresh.items.is_empty());
        for name in ["Important.txt", "Other.pdf"] {
            assert!(
                fresh
                    .skipped
                    .iter()
                    .any(|item| { item.name == name && item.reason.contains("working directory") })
            );
        }

        // Another Settings window can have an older preview. Apply rechecks
        // the same production dependency rules before moving even one file.
        let before_apps = f.store.apps().unwrap();
        let error = apply(&f.store, &old_preview, &AtomicBool::new(false))
            .expect_err("an old preview cannot override a new desktop dependency");
        assert!(
            format!("{error:#}").contains("working directory"),
            "{error:#}"
        );
        assert_eq!(f.store.apps().unwrap(), before_apps);
        assert_eq!(
            disk::read(f.desktop.join("Important.txt")).unwrap(),
            b"user document"
        );
        assert_eq!(
            disk::read(f.desktop.join("Other.pdf")).unwrap(),
            b"user document"
        );
        assert!(!f.desktop.join("Documents").exists());
        assert!(!status(&f.store).unwrap().pending);
        assert!(status(&f.store).unwrap().undo.is_none());
    }

    // After the user changes the app's working folder, a newly reviewed
    // plan can still be applied and completely undone.
    app.cwd = tools.to_string_lossy().into_owned();
    f.store.put_app(&app).unwrap();
    let reviewed = f.plan(true);
    assert_eq!(reviewed.summary(), (2, 1, 1));
    assert_eq!(
        apply(&f.store, &reviewed, &AtomicBool::new(false))
            .unwrap()
            .moved,
        2
    );
    let operation = status(&f.store).unwrap().undo.unwrap();
    assert_eq!(undo(&f.store, &operation).unwrap().restored, 2);
    for name in ["Important.txt", "Other.pdf"] {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), b"user document");
    }
}

#[test]
fn aliased_local_launch_or_icon_path_cannot_move_a_desktop_dependency() {
    let f = Fixture::new();
    for name in ["Important image.png", "Other.png"] {
        disk::write(f.desktop.join(name), b"unchanged user image").unwrap();
    }
    let old_preview = f.plan(true);
    assert_eq!(old_preview.summary(), (2, 1, 1));

    let image = f.desktop.join("Important image.png");
    let tools = f.store.home.join("Tools");
    disk::create_dir(&tools).unwrap();
    // A hard link elsewhere shares the same file ID but not the desktop
    // directory entry: moving the desktop link does not break this target.
    let external = tools.join("External image.png");
    disk::hard_link(&image, &external).unwrap();
    assert_eq!(
        fs::inspect(&image, false).unwrap().id,
        fs::inspect(&external, false).unwrap().id
    );
    let executable = tools.join("Viewer.exe");
    disk::write(&executable, b"inert; not executed").unwrap();
    let link = tools.join("Viewer.lnk");
    native::make_link(&link, &executable, "", "", 0, "fixture").unwrap();
    let mut app = native::read_link(&link).unwrap();
    app.target = external.to_string_lossy().into_owned();
    f.store.put_app(&app).unwrap();
    assert_eq!(f.plan(true).summary(), (2, 1, 1));

    // An alias in the actual personal Desktop does break when moved.
    // Do not require 8.3 short names: those can be disabled on NTFS.
    let dotted = f.desktop.join(".").join("Important image.png");
    let aliases = [
        dotted.to_string_lossy().into_owned(),
        format!(r"\\?\{}", dotted.display()),
    ];
    for alias in aliases {
        assert_ne!(norm(&alias), norm(&image.to_string_lossy()));
        assert_eq!(
            fs::inspect(&crate::profile::ordinary_path(PathBuf::from(&alias)), false)
                .unwrap()
                .id,
            fs::inspect(&image, false).unwrap().id
        );
        for as_icon in [false, true] {
            app.target = if as_icon {
                executable.to_string_lossy().into_owned()
            } else {
                alias.clone()
            };
            app.icon = if as_icon {
                alias.clone()
            } else {
                String::new()
            };
            f.store.put_app(&app).unwrap();

            let fresh = f.plan(true);
            assert!(fresh.items.iter().any(|item| item.name == "Other.png"));
            assert!(
                !fresh
                    .items
                    .iter()
                    .any(|item| item.name == "Important image.png")
            );
            assert!(fresh.skipped.iter().any(|item| {
                item.name == "Important image.png"
                    && item.reason.contains("Another known shortcut depends")
            }));

            // A second Settings window may still be holding the old plan.
            let before = f.store.apps().unwrap();
            let error = apply(&f.store, &old_preview, &AtomicBool::new(false))
                .expect_err("Apply must recheck aliases before moving anything");
            assert!(
                format!("{error:#}").contains("Another known shortcut depends"),
                "{error:#}"
            );
            assert_eq!(f.store.apps().unwrap(), before);
            assert_eq!(disk::read(&image).unwrap(), b"unchanged user image");
            assert_eq!(
                disk::read(f.desktop.join("Other.png")).unwrap(),
                b"unchanged user image"
            );
            assert!(!f.desktop.join("Images").exists());
            assert!(!status(&f.store).unwrap().pending);
            assert!(status(&f.store).unwrap().undo.is_none());
        }
    }

    // The unrelated hard link is no reason to block explicit opt-in moves.
    app.target = external.to_string_lossy().into_owned();
    app.icon.clear();
    f.store.put_app(&app).unwrap();
    let reviewed = f.plan(true);
    assert_eq!(reviewed.summary(), (2, 1, 1));
    assert_eq!(
        apply(&f.store, &reviewed, &AtomicBool::new(false))
            .unwrap()
            .moved,
        2
    );
    let operation = status(&f.store).unwrap().undo.unwrap();
    assert_eq!(undo(&f.store, &operation).unwrap().restored, 2);
    assert_eq!(disk::read(&image).unwrap(), b"unchanged user image");
    assert_eq!(disk::read(&external).unwrap(), b"unchanged user image");
}

#[test]
fn dependency_created_after_preview_prevents_every_move() {
    let f = Fixture::new();
    for name in ["Important.txt", "Other.txt"] {
        disk::write(f.desktop.join(name), b"original document").unwrap();
    }
    let preview = f.plan(true);
    assert_eq!(preview.summary(), (2, 1, 1));
    let document = f.desktop.join("Important.txt");
    f.argument_link(&format!("\"{}\"", document.display()));
    let before = f.store.apps().unwrap();
    apply(&f.store, &preview, &AtomicBool::new(false))
        .expect_err("new shortcut dependencies must be checked again at Apply");
    assert_eq!(f.store.apps().unwrap(), before);
    assert_eq!(disk::read(document).unwrap(), b"original document");
    assert!(f.desktop.join("Other.txt").is_file());
    assert!(!f.desktop.join("Documents").exists());
    assert!(!status(&f.store).unwrap().pending);
    assert!(status(&f.store).unwrap().undo.is_none());
}

#[test]
fn unknown_shortcut_dependencies_keep_files_but_not_independent_site_shortcuts() {
    let f = Fixture::new();
    f.site("Alpha");
    f.site("Beta");
    for name in ["Important.txt", "Other.txt"] {
        disk::write(f.desktop.join(name), b"original document").unwrap();
    }
    let broken = f.desktop.join("Unreadable.lnk");
    disk::write(&broken, b"not a valid Windows Shell link").unwrap();
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (2, 1, 1));
    assert!(plan.items.iter().all(|item| item.group == Group::Sites));
    assert!(plan.skipped.iter().any(|item| item.name == "Important.txt"));
    apply(&f.store, &plan, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        disk::read(f.desktop.join("Important.txt")).unwrap(),
        b"original document"
    );
    assert_eq!(
        disk::read(&broken).unwrap(),
        b"not a valid Windows Shell link"
    );
    let id = status(&f.store).unwrap().undo.unwrap();
    undo(&f.store, &id).unwrap();
    disk::remove_file(broken).unwrap();
    assert_eq!(f.plan(true).summary(), (4, 2, 2));
}

#[test]
fn relative_argument_spelling_is_conservatively_preserved_without_launching() {
    let f = Fixture::new();
    disk::write(f.desktop.join("Report.TXT"), b"keep").unwrap();
    f.argument_link("--file=\"..\\Desktop\\REPORT.txt\"");
    let plan = f.plan(true);
    assert!(!plan.items.iter().any(|item| item.name == "Report.TXT"));
    assert!(plan.skipped.iter().any(|item| item.name == "Report.TXT"));
    assert_eq!(disk::read(f.desktop.join("Report.TXT")).unwrap(), b"keep");
}

#[test]
fn rejecting_all_suggestions_persists_a_choice_without_a_fake_organization() {
    let f = Fixture::new();
    f.site("Alpha");
    f.site("Beta");
    let before = f.store.apps().unwrap();
    let alpha = disk::read(f.desktop.join("Alpha.url")).unwrap();
    let beta = disk::read(f.desktop.join("Beta.url")).unwrap();
    let mut preview = f.plan(false);
    assert_eq!(preview.summary(), (2, 1, 1));
    for item in &mut preview.items {
        item.selected = false;
    }
    assert_eq!(preview.summary(), (0, 0, 0));

    let outcome = apply(&f.store, &preview, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        (
            outcome.moved,
            outcome.restored,
            outcome.folders_created,
            outcome.kept
        ),
        (0, 0, 0, 2)
    );
    assert_eq!(f.store.apps().unwrap(), before);
    assert_eq!(disk::read(f.desktop.join("Alpha.url")).unwrap(), alpha);
    assert_eq!(disk::read(f.desktop.join("Beta.url")).unwrap(), beta);
    assert!(!f.desktop.join("Sites").exists());
    assert_eq!(
        f.store.get::<Vec<String>>(HISTORY).unwrap(),
        Vec::<String>::new()
    );
    assert!(status(&f.store).unwrap().undo.is_none());
    assert!(!status(&f.store).unwrap().pending);

    // The same user decision survives a fresh SQLite connection and scan.
    let reopened = Store::open(&f.store.home).unwrap();
    let next = analyze(
        &reopened,
        Options {
            include_files: false,
        },
        &Config::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(next.items.is_empty());
    assert_eq!(reopened.get::<Vec<String>>(KEEP).unwrap().len(), 2);

    clear_exclusions(&reopened).unwrap();
    assert_eq!(f.plan(false).summary(), (2, 1, 1));
}

#[test]
fn stale_rejected_file_cannot_create_a_persistent_exclusion() {
    let f = Fixture::new();
    f.site("Alpha");
    f.site("Beta");
    let mut preview = f.plan(false);
    for item in &mut preview.items {
        item.selected = false;
    }
    disk::write(f.desktop.join("Beta.url"), b"changed after preview").unwrap();
    let before = f.store.apps().unwrap();
    let error = apply(&f.store, &preview, &AtomicBool::new(false))
        .expect_err("a stale identity must not be saved as a user exclusion");
    assert!(format!("{error:#}").contains("changed after preview"));
    assert!(f.store.get::<Vec<String>>(KEEP).unwrap().is_empty());
    assert_eq!(f.store.apps().unwrap(), before);
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
    assert!(status(&f.store).unwrap().undo.is_none());
    assert!(!status(&f.store).unwrap().pending);
}

#[test]
fn mixed_apply_rechecks_every_rejected_object_before_moving() {
    // A chosen Keep is meaningful even when other objects are moving.
    // A stale selection must not be persisted under the previous file ID,
    // nor allow unrelated files to move on the strength of an old preview.
    for change in ["missing", "replaced", "edited"] {
        let f = Fixture::new();
        for name in ["Alpha", "Beta", "Gamma"] {
            f.site(name);
        }
        let mut preview = f.plan(false);
        assert_eq!(preview.summary(), (3, 1, 2));
        preview
            .items
            .iter_mut()
            .find(|item| item.name == "Gamma.url")
            .unwrap()
            .selected = false;
        assert_eq!(preview.summary(), (2, 1, 1));

        let alpha = disk::read(f.desktop.join("Alpha.url")).unwrap();
        let beta = disk::read(f.desktop.join("Beta.url")).unwrap();
        let original = f.desktop.join("Gamma.url");
        let backup = f.store.home.join("Original Gamma.url");
        let gamma = disk::read(&original).unwrap();
        let before_apps = f.store.apps().unwrap();

        match change {
            "missing" | "replaced" => {
                disk::rename(&original, &backup).unwrap();
                if change == "replaced" {
                    disk::write(
                        &original,
                        b"[InternetShortcut]\r\nURL=https://example.invalid/replacement\r\n",
                    )
                    .unwrap();
                }
            }
            "edited" => {
                disk::write(
                    &original,
                    b"[InternetShortcut]\r\nURL=https://example.invalid/edited\r\n",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }

        let error = apply(&f.store, &preview, &AtomicBool::new(false))
            .expect_err("mixed Apply must recheck every rejected object");
        assert!(
            format!("{error:#}").contains("An excluded object changed after preview"),
            "unexpected error for {change}: {error:#}"
        );
        assert_eq!(disk::read(f.desktop.join("Alpha.url")).unwrap(), alpha);
        assert_eq!(disk::read(f.desktop.join("Beta.url")).unwrap(), beta);
        assert!(!f.desktop.join("Sites").exists());
        assert_eq!(f.store.apps().unwrap(), before_apps);
        assert!(f.store.get::<Vec<String>>(KEEP).unwrap().is_empty());
        assert!(f.store.get::<Vec<String>>(HISTORY).unwrap().is_empty());
        assert!(status(&f.store).unwrap().undo.is_none());
        assert!(!status(&f.store).unwrap().pending);

        // A fresh preview may be explicitly accepted. It then remembers
        // the actual current file, and Undo retains that choice.
        if change == "missing" {
            disk::rename(&backup, &original).unwrap();
        }
        let current_gamma = disk::read(&original).unwrap();
        let mut fresh = f.plan(false);
        fresh
            .items
            .iter_mut()
            .find(|item| item.name == "Gamma.url")
            .unwrap()
            .selected = false;
        let outcome = apply(&f.store, &fresh, &AtomicBool::new(false)).unwrap();
        assert_eq!((outcome.moved, outcome.kept), (2, 1));
        assert_eq!(f.store.get::<Vec<String>>(KEEP).unwrap().len(), 1);
        let operation = status(&f.store).unwrap().undo.unwrap();
        assert_eq!(undo(&f.store, &operation).unwrap().restored, 2);
        assert_eq!(disk::read(f.desktop.join("Alpha.url")).unwrap(), alpha);
        assert_eq!(disk::read(f.desktop.join("Beta.url")).unwrap(), beta);
        assert_eq!(disk::read(&original).unwrap(), current_gamma);
        if change == "replaced" {
            assert_eq!(disk::read(&backup).unwrap(), gamma);
        }
        assert!(
            !f.plan(false)
                .items
                .iter()
                .any(|item| item.name == "Gamma.url")
        );
    }
}

#[test]
fn saved_catalog_source_alias_and_manual_choice_survive_preview_apply_and_undo() {
    let f = Fixture::new();
    for name in ["Alpha", "Beta", "Gamma"] {
        f.site(name);
    }
    let original = f.desktop.join("Alpha.url");
    let alias = f.desktop.join(".").join("Alpha.url");
    assert_ne!(
        norm(&original.to_string_lossy()),
        norm(&alias.to_string_lossy())
    );
    let stale = f.plan(false);
    assert_eq!(stale.summary(), (3, 1, 2));

    // A policy change can arrive after the preview from another Settings
    // window, with an alternate but valid spelling of the same local source.
    let mut saved = native::read_link(&original).unwrap();
    saved.source = alias.to_string_lossy().into_owned();
    saved.policy = Policy::Always;
    f.store.put_app(&saved).unwrap();
    let previous_apps = f.store.apps().unwrap();
    let error = apply(&f.store, &stale, &AtomicBool::new(false))
        .expect_err("stale selection must not move a manually controlled alias");
    assert!(format!("{error:#}").contains("catalog source"), "{error:#}");
    assert_eq!(f.store.apps().unwrap(), previous_apps);
    assert!(f.desktop.join("Beta.url").exists());
    assert!(f.desktop.join("Gamma.url").exists());
    assert!(!f.desktop.join("Sites").exists());
    assert!(status(&f.store).unwrap().undo.is_none());

    // A refreshed plan can still help: the two unrelated sites are moved,
    // and Undo restores them without touching the user's manual source.
    let refreshed = f.plan(false);
    assert_eq!(refreshed.summary(), (2, 1, 1));
    assert!(refreshed.skipped.iter().any(|s| s.name == "Alpha.url"));
    assert_eq!(
        apply(&f.store, &refreshed, &AtomicBool::new(false))
            .unwrap()
            .moved,
        2
    );
    let undo_id = status(&f.store).unwrap().undo.unwrap();
    assert_eq!(undo(&f.store, &undo_id).unwrap().restored, 2);
    assert_eq!(f.store.app(&saved.id).unwrap(), saved);
    assert!(original.is_file());
    assert!(!status(&f.store).unwrap().pending);

    // Even an automatic catalog record must not retain a broken path after
    // its Desktop entry is moved. Explicitly releasing Always is not enough.
    saved.policy = Policy::Auto;
    f.store.put_app(&saved).unwrap();
    assert_eq!(f.plan(false).summary(), (2, 1, 1));

    // File ID alone must not reject a different directory entry: an external
    // hard link remains usable when its Desktop sibling is moved.
    let external_dir = f.store.home.join("Other");
    disk::create_dir(&external_dir).unwrap();
    let external = external_dir.join("Alpha.url");
    disk::hard_link(&original, &external).unwrap();
    saved.source = external.to_string_lossy().into_owned();
    f.store.put_app(&saved).unwrap();
    assert_eq!(f.plan(false).summary(), (3, 1, 2));
    assert!(external.is_file());

    // The fixture's Temp spelling may be an 8.3 path. Use the same
    // canonical root as production Analyze, not the test input spelling.
    let canonical = root(&f.store).unwrap().join("Alpha.url");
    saved.source = canonical.to_string_lossy().into_owned();
    f.store.put_app(&saved).unwrap();
    let current = f.plan(false);
    assert_eq!(current.summary(), (3, 1, 2), "{:?}", current.skipped);
    assert_eq!(
        apply(&f.store, &current, &AtomicBool::new(false))
            .unwrap()
            .moved,
        3
    );
    let undo_id = status(&f.store).unwrap().undo.unwrap();
    assert_eq!(undo(&f.store, &undo_id).unwrap().restored, 3);
    assert_eq!(f.store.app(&saved.id).unwrap(), saved);
    assert!(original.exists());
    assert!(external.exists());
}
