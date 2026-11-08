//! Native screenshot preparation must preserve real local choices and files.
use super::*;
use std::fs;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    app: Settings,
    desktop: PathBuf,
    important: App,
    _com: native::Com,
    _cleanup: Cleanup,
}
impl Fixture {
    fn new(language: &str, count: usize) -> Self {
        let home = std::env::temp_dir().join(format!(
            "test-kept-review-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let desktop = home.join("Desktop %USERNAME%");
        fs::create_dir_all(&desktop).unwrap();
        let store = Store::open(&home).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("background_disabled", &true).unwrap();
        let config = Config {
            language: language.into(),
            collect: false,
            automatic: false,
            autostart: false,
            learning_enabled: false,
            ..Config::default()
        };
        store.set("config", &config).unwrap();
        let com = native::Com::new().unwrap();
        for index in 0..count {
            let path = desktop.join(format!("Project {index}.url"));
            fs::write(
                &path,
                format!("[InternetShortcut]\r\nURL=https://example.invalid/project/{index}\r\n"),
            )
            .unwrap();
            store.put_app(&native::read_link(&path).unwrap()).unwrap();
        }
        let path = desktop.join("Important.url");
        fs::write(
            &path,
            "[InternetShortcut]\r\nURL=https://example.invalid/important\r\n",
        )
        .unwrap();
        let mut important = native::read_link(&path).unwrap();
        important.policy = Policy::Always;
        important.note = "Keep the manual choice".into();
        store.put_app(&important).unwrap();
        store.observe(&important.id, 321, true, false).unwrap();
        drop(store);
        let mut app = Settings::new(home.clone()).unwrap();
        app.layout_receiver = None;
        app.find_after_layout = false;
        app.capture = Some(home.join("images"));
        Self {
            app,
            desktop,
            important,
            _com: com,
            _cleanup: Cleanup(home),
        }
    }

    fn files(&self) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
        fs::read_dir(&self.desktop)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let bytes = fs::read(&path).unwrap();
                (path, bytes)
            })
            .collect()
    }

    fn changes(&self) -> i64 {
        self.app
            .store
            .db
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap()
    }
}

#[test]
fn singleton_capture_uses_the_real_plan_without_saving_exclusions_or_moving_files() {
    for language in ["ru", "en"] {
        let mut f = Fixture::new(language, 3);
        let files = f.files();
        let days = serde_json::to_value(f.app.store.days().unwrap()).unwrap();
        organization(&mut f.app, "organization").unwrap();
        let ordinary = f.app.organization_plan.as_ref().unwrap();
        assert_eq!(ordinary.summary(), (3, 1, 2));
        f.app.store.set("test_singleton_review", &true).unwrap();
        let changes = f.changes();
        organization(&mut f.app, "organization").unwrap();
        let plan = f.app.organization_plan.as_ref().unwrap();
        assert_eq!(plan.summary(), (0, 0, 0));
        assert_eq!(plan.items.len(), 3);
        assert_eq!(plan.items.iter().filter(|item| item.selected).count(), 1);
        let folder = organize::Group::Sites.label(language == "ru");
        for item in &plan.items {
            assert!(!plan.will_move(item));
            assert_eq!(plan.destination(item), folder);
        }
        assert_eq!(f.changes(), changes, "capture preparation must be read-only");
        assert_eq!(f.files(), files);
        assert_eq!(f.app.store.app(&f.important.id).unwrap(), f.important);
        let after_days = serde_json::to_value(f.app.store.days().unwrap()).unwrap();
        assert_eq!(after_days, days);
        assert!(f.app.job.is_none());
        let status = organize::status(&f.app.store).unwrap();
        assert!(!status.pending);
        assert!(status.undo.is_none());
        assert!(f.app.store.get::<bool>("background_disabled").unwrap());
    }
}

#[test]
fn singleton_capture_rejects_a_deficient_fixture_without_changing_files_or_rules() {
    let mut f = Fixture::new("en", 1);
    f.app.store.set("test_singleton_review", &true).unwrap();
    let files = f.files();
    let changes = f.changes();
    let error = organization(&mut f.app, "organization").unwrap_err();
    assert!(error.to_string().contains("at least two items in one group"));
    assert_eq!(f.changes(), changes);
    assert_eq!(f.files(), files);
    assert_eq!(f.app.store.app(&f.important.id).unwrap(), f.important);
    assert!(f.app.job.is_none());
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());
}
