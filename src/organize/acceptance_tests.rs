use super::*;
use crate::{engine, model::App};
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
            "test-organizer-acceptance-{}",
            fs::operation_id().unwrap()
        ));
        disk::create_dir_all(home.join("Desktop")).unwrap();
        let home = crate::profile::ordinary_path(disk::canonicalize(home).unwrap());
        let desktop = home.join("Desktop");
        let store = Store::open(&home).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("background_disabled", &true).unwrap();
        store
            .set(
                "config",
                &Config {
                    learning_enabled: false,
                    respect_taskbar_pins: false,
                    slots: 1,
                    ..Default::default()
                },
            )
            .unwrap();
        Self {
            store,
            desktop,
            _com: native::Com::new().unwrap(),
            _cleanup: Cleanup(home),
        }
    }

    fn url(&self, name: &str) -> App {
        let path = self.desktop.join(format!("{name}.url"));
        disk::write(
            &path,
            format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
        )
        .unwrap();
        let app = native::read_link(&path).unwrap();
        self.store.put_app(&app).unwrap();
        app
    }

    fn plan(&self, include_files: bool) -> Plan {
        analyze(
            &self.store,
            Options { include_files },
            &self.store.get::<Config>("config").unwrap(),
            &AtomicBool::new(false),
        )
        .unwrap()
    }
}

#[test]
fn organization_and_quick_placement_undo_work_in_both_orders() {
    for organization_first in [true, false] {
        let f = Fixture::new();
        let alpha = f.url("Alpha");
        let beta = f.url("Beta");
        let original = fs::inspect(Path::new(&alpha.source), true).unwrap();
        let history = serde_json::to_value(f.store.days().unwrap()).unwrap();
        let result = apply(&f.store, &f.plan(false), &AtomicBool::new(false)).unwrap();
        assert_eq!(result.moved, 2);
        let operation = status(&f.store).unwrap().undo.unwrap();
        f.store.policy(&alpha.id, Policy::Always).unwrap();
        engine::apply(&f.store, true).unwrap();
        let placed: State = f.store.get("state").unwrap();
        assert_eq!(placed.owned.len(), 1);
        let quick = &placed.owned[0];
        assert_eq!(quick.app, alpha.id);
        assert_ne!(norm(&quick.path), norm(&alpha.source));
        let quick_stamp = fs::inspect(Path::new(&quick.path), true).unwrap();
        assert_ne!(quick_stamp.id, original.id);
        if organization_first {
            undo(&f.store, &operation).unwrap();
            assert_eq!(
                fs::inspect(Path::new(&quick.path), true).unwrap(),
                quick_stamp
            );
            assert_eq!(f.store.get::<State>("state").unwrap().owned, placed.owned);
            engine::rollback(&f.store).unwrap();
        } else {
            engine::rollback(&f.store).unwrap();
            assert!(f.desktop.join("Sites/Alpha.url").is_file());
            assert_eq!(
                fs::inspect(&f.desktop.join("Sites/Alpha.url"), true).unwrap(),
                original
            );
            undo(&f.store, &operation).unwrap();
        }
        assert!(!Path::new(&quick.path).exists());
        assert!(f.store.get::<State>("state").unwrap().owned.is_empty());
        assert_eq!(
            fs::inspect(Path::new(&alpha.source), true).unwrap(),
            original
        );
        assert!(Path::new(&beta.source).is_file());
        assert_eq!(f.store.app(&alpha.id).unwrap().source, alpha.source);
        assert_eq!(f.store.app(&alpha.id).unwrap().policy, Policy::Always);
        assert_eq!(
            serde_json::to_value(f.store.days().unwrap()).unwrap(),
            history
        );
        assert!(!status(&f.store).unwrap().pending);
        assert!(f.store.get::<bool>("background_disabled").unwrap());
    }
}

#[test]
fn optional_file_organization_keeps_media_and_editing_sidecars_together() {
    for (media, companion, independent) in [
        ("Photo.jpg", "Photo.jpg.xmp", "Separate.jpg"),
        ("Photo.jpg", "Photo.xmp", "Separate.jpg"),
        ("Film.mp4", "Film.srt", "Separate.mp4"),
        ("Film.mp4", "Film.en.srt", "Separate.mp4"),
        ("Album.flac", "Album.cue", "Separate.flac"),
    ] {
        let f = Fixture::new();
        for name in [media, companion, independent] {
            disk::write(f.desktop.join(name), b"user content").unwrap();
        }
        f.url("Alpha");
        f.url("Beta");
        let plan = f.plan(true);
        assert!(
            !plan.items.iter().any(|item| item.name == media),
            "Moving {media} would separate it from {companion}"
        );
        assert!(plan.skipped.iter().any(|item| item.name == media));
        assert!(plan.items.iter().any(|item| item.name == independent));
        let outcome = apply(&f.store, &plan, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            outcome.moved, 2,
            "Only the independent site group should move"
        );
        for name in [media, companion, independent] {
            assert_eq!(disk::read(f.desktop.join(name)).unwrap(), b"user content");
        }
        let operation = status(&f.store).unwrap().undo.unwrap();
        undo(&f.store, &operation).unwrap();
        assert!(f.desktop.join("Alpha.url").is_file());
        assert!(f.desktop.join("Beta.url").is_file());
    }
}

#[test]
fn a_sidecar_created_after_preview_prevents_every_move_without_partial_folders() {
    let f = Fixture::new();
    for name in ["Film.mp4", "Separate.mp4"] {
        disk::write(f.desktop.join(name), b"media content").unwrap();
    }
    let plan = f.plan(true);
    assert_eq!(plan.summary(), (2, 1, 1));
    disk::write(f.desktop.join("Film.srt"), b"subtitles saved after preview").unwrap();
    assert!(apply(&f.store, &plan, &AtomicBool::new(false)).is_err());
    for name in ["Film.mp4", "Separate.mp4"] {
        assert_eq!(disk::read(f.desktop.join(name)).unwrap(), b"media content");
    }
    assert!(!f.desktop.join("Media").exists());
    assert!(!status(&f.store).unwrap().pending);
    assert!(status(&f.store).unwrap().undo.is_none());
}
