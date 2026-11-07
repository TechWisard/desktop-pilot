use super::*;
use crate::{native, store::Store};
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_root(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "desktop-pilot-literal-launch-{suffix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn cleanup(root: &std::path::Path) {
    for attempt in 0..5 {
        match std::fs::remove_dir_all(root) {
            Ok(()) => return,
            Err(_) if attempt < 4 => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => panic!("cannot remove isolated fixture: {error}"),
        }
    }
}

#[test]
fn literal_absolute_launch_targets_keep_saved_manual_rules_independent() {
    // A real directory can be named %USERNAME%. It must not be confused
    // with the path obtained by expanding that variable in a launch template.
    let user = std::env::var("USERNAME").expect("Windows username");
    let root = fixture_root("targets");
    let literal = root.join("%USERNAME%").join("Inspect.exe");
    let expanded = root.join(&user).join("Inspect.exe");
    assert_ne!(literal, expanded);
    std::fs::create_dir_all(literal.parent().unwrap()).unwrap();
    std::fs::write(&literal, b"literal fixture").unwrap();

    let mut literal_app = App {
        name: "Literal target".into(),
        target: literal.to_string_lossy().into_owned(),
        policy: Policy::Never,
        source: root.join("literal.lnk").to_string_lossy().into_owned(),
        valid: true,
        ..Default::default()
    };
    let mut expanded_app = App {
        name: "Expanded target".into(),
        target: expanded.to_string_lossy().into_owned(),
        policy: Policy::Always,
        source: root.join("expanded.lnk").to_string_lossy().into_owned(),
        valid: true,
        ..Default::default()
    };
    assert!(native::valid_target(&literal_app));
    assert!(!native::valid_target(&expanded_app));
    std::fs::create_dir_all(expanded.parent().unwrap()).unwrap();
    std::fs::write(&expanded, b"expanded fixture").unwrap();
    assert!(native::valid_target(&expanded_app));

    literal_app.assign_id();
    expanded_app.assign_id();
    assert_ne!(literal_app.identity(), expanded_app.identity());
    assert_ne!(literal_app.id, expanded_app.id);

    let store = Store::open(&root.join("profile")).unwrap();
    store.put_app(&literal_app).unwrap();
    store.put_app(&expanded_app).unwrap();
    let saved = store.apps().unwrap();
    assert_eq!(saved.len(), 2);
    let cfg = Config {
        slots: 1,
        collect: true,
        ..Default::default()
    };
    let selected = select_at(
        &saved,
        &rank(&saved, &store.days().unwrap(), &cfg, now()),
        &State::default(),
        &cfg,
        true,
        now(),
    );
    assert_eq!(selected, [expanded_app.id.clone()]);

    // A later manual reversal changes only the intended saved profile.
    literal_app.policy = Policy::Always;
    expanded_app.policy = Policy::Never;
    store.put_app(&literal_app).unwrap();
    store.put_app(&expanded_app).unwrap();
    let after = store.apps().unwrap();
    assert_eq!(after.len(), 2);
    let selected = select_at(
        &after,
        &rank(&after, &store.days().unwrap(), &cfg, now()),
        &State::default(),
        &cfg,
        true,
        now(),
    );
    assert_eq!(selected, [literal_app.id.clone()]);
    assert_eq!(std::fs::read(&literal).unwrap(), b"literal fixture");
    assert_eq!(std::fs::read(&expanded).unwrap(), b"expanded fixture");
    drop(store);
    cleanup(&root);
}

#[test]
fn literal_working_directory_and_relative_templates_stay_distinct() {
    let user = std::env::var("USERNAME").expect("Windows username");
    let root = fixture_root("cwd");
    let executable = root.join("Viewer.exe");
    let literal = root.join("%USERNAME%");
    let expanded = root.join(&user);
    std::fs::create_dir_all(&literal).unwrap();
    std::fs::write(&executable, b"fixture").unwrap();
    let mut literal_app = App {
        target: executable.to_string_lossy().into_owned(),
        cwd: literal.to_string_lossy().into_owned(),
        ..Default::default()
    };
    let mut expanded_app = App {
        cwd: expanded.to_string_lossy().into_owned(),
        ..literal_app.clone()
    };
    assert!(native::valid_target(&literal_app));
    assert!(!native::valid_target(&expanded_app));
    std::fs::create_dir_all(&expanded).unwrap();
    assert!(native::valid_target(&expanded_app));
    literal_app.assign_id();
    expanded_app.assign_id();
    assert_ne!(literal_app.id, expanded_app.id);

    let system_root = std::env::var("SystemRoot").expect("Windows SystemRoot");
    let template = r"%SystemRoot%\System32\notepad.exe";
    let concrete = format!(r"{system_root}\System32\notepad.exe");
    assert_eq!(target_identity(template), target_identity(&concrete));
    assert_eq!(
        target_identity(r"C:\Apps\Editor.EXE"),
        target_identity(r"c:/apps/editor.exe")
    );
    cleanup(&root);
}
