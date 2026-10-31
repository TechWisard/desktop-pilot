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
        store.set("test_desktop", &desktop.to_string_lossy()).unwrap();
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
        self.store.put_app(&native::read_link(&path).unwrap()).unwrap();
    }

    fn argument_link(&self, argument: &str) {
        let tools = self.store.home.join("Tools");
        disk::create_dir_all(&tools).unwrap();
        let executable = tools.join("Reader.exe");
        disk::write(&executable, b"inert fixture; never execute").unwrap();
        let path = self.desktop.join("Open document.lnk");
        native::make_link(&path, &executable, argument, "", 0, "fixture").unwrap();
        self.store.put_app(&native::read_link(&path).unwrap()).unwrap();
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
    latest.items.iter_mut().find(|i| i.name == "Alpha.url").unwrap().selected = false;
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
    assert!(plan.skipped.iter().any(|i| i.name == "Important report.txt"));
    assert_eq!(disk::read(document).unwrap(), b"original document");
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
