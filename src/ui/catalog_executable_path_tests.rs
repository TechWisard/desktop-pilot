use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn copied_executable_path_keeps_literal_percent_directories_and_rules_distinct() {
    // These are two different, existing Windows paths. A pasted absolute
    // executable path is not an environment-variable template.
    let username = std::env::var("USERNAME").expect("Windows USERNAME");
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-executable-search-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let literal = root.join("%USERNAME%").join("Rare Tool.exe");
    let expanded = root.join(&username).join("Rare Tool.exe");
    assert_ne!(
        norm_literal_path(&literal.to_string_lossy()),
        norm_literal_path(&expanded.to_string_lossy())
    );
    std::fs::create_dir_all(literal.parent().unwrap()).unwrap();
    std::fs::create_dir_all(expanded.parent().unwrap()).unwrap();
    std::fs::write(&literal, b"literal executable fixture").unwrap();
    std::fs::write(&expanded, b"expanded executable fixture").unwrap();

    let system_root = std::env::var("SystemRoot").expect("Windows SystemRoot");
    let store = Store::open(&root.join("profile")).unwrap();
    for (id, target, policy, note) in [
        (
            "literal",
            literal.to_string_lossy().into_owned(),
            Policy::Never,
            String::new(),
        ),
        (
            "expanded",
            expanded.to_string_lossy().into_owned(),
            Policy::Always,
            String::new(),
        ),
        (
            "template",
            r"%SystemRoot%\System32\notepad.exe".into(),
            Policy::Auto,
            String::new(),
        ),
        (
            "note",
            r"C:\Another\tool.exe".into(),
            Policy::Auto,
            literal.to_string_lossy().into_owned(),
        ),
    ] {
        store
            .put_app(&App {
                id: id.into(),
                name: format!("Profile {id}"),
                source: format!(r"C:\Shortcuts\{id}.lnk"),
                target,
                policy,
                valid: true,
                note,
                ..Default::default()
            })
            .unwrap();
    }
    let before = store.apps().unwrap();
    let ranking = rank(&before, &store.days().unwrap(), &Config::default(), now());
    let mut cache = CatalogCache::default();
    cache.rebuild(&before, &ranking);
    let recent = HashSet::new();
    let mut matches = |query: &str, filter| {
        cache.update(&before, &ranking, query, filter, 0, &recent);
        cache
            .indices
            .iter()
            .map(|&i| before[i].id.as_str())
            .collect::<Vec<_>>()
    };

    for query in [
        format!(r#""{}""#, literal.display()),
        literal.to_string_lossy().replace('\\', "/"),
    ] {
        assert_eq!(matches(&query, 0), ["literal"], "literal path: {query}");
        assert_eq!(matches(&query, 3), ["literal"], "Never rule: {query}");
        assert!(
            matches(&query, 2).is_empty(),
            "Never is not Always: {query}"
        );
    }
    let expanded_query = expanded.to_string_lossy();
    assert_eq!(matches(&expanded_query, 0), ["expanded"]);
    assert_eq!(matches(&expanded_query, 2), ["expanded"]);
    assert!(matches(&expanded_query, 3).is_empty());

    // Only an explicitly relative template may expand. Keep existing search
    // for launch targets saved as %SystemRoot%\... shortcuts.
    let windows_query = format!(r"{}\System32\notepad.exe", system_root);
    assert_eq!(matches(&windows_query, 0), ["template"]);
    assert_eq!(store.apps().unwrap(), before);
    assert_eq!(
        std::fs::read(&literal).unwrap(),
        b"literal executable fixture"
    );
    assert_eq!(
        std::fs::read(&expanded).unwrap(),
        b"expanded executable fixture"
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn copied_batch_launch_paths_identify_only_the_saved_target_and_policy() {
    // Full Explorer paths should not match unrelated notes or longer names.
    // Use real, distinct local files and the actual SQLite catalog/search.
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-batch-search-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cmd = root.join("Team Alpha").join("Launch.cmd");
    let longer = root.join("Team Alpha").join("Launch.cmd.backup");
    let bat = root.join("Maintenance").join("Deploy.bat");
    std::fs::create_dir_all(cmd.parent().unwrap()).unwrap();
    std::fs::create_dir_all(bat.parent().unwrap()).unwrap();
    std::fs::write(&cmd, b"@echo off\r\nrem first command\r\n").unwrap();
    std::fs::write(&longer, b"unrelated backup\r\n").unwrap();
    std::fs::write(&bat, b"@echo off\r\nrem other batch\r\n").unwrap();
    let store = Store::open(&root.join("profile")).unwrap();
    for (id, target, policy, note) in [
        (
            "cmd",
            cmd.to_string_lossy().into_owned(),
            Policy::Never,
            String::new(),
        ),
        (
            "longer",
            longer.to_string_lossy().into_owned(),
            Policy::Auto,
            String::new(),
        ),
        (
            "cmd-note",
            r"C:\Different\Unrelated.exe".into(),
            Policy::Always,
            cmd.to_string_lossy().into_owned(),
        ),
        (
            "bat",
            bat.to_string_lossy().into_owned(),
            Policy::Always,
            String::new(),
        ),
        (
            "bat-note",
            r"C:\Different\Other.exe".into(),
            Policy::Never,
            bat.to_string_lossy().into_owned(),
        ),
    ] {
        store
            .put_app(&App {
                id: id.into(),
                name: format!("Launcher {id}"),
                source: format!(r"C:\Shortcuts\{id}.lnk"),
                target,
                policy,
                note,
                valid: true,
                ..Default::default()
            })
            .unwrap();
    }
    let apps = store.apps().unwrap();
    let ranking = rank(&apps, &store.days().unwrap(), &Config::default(), now());
    let mut cache = CatalogCache::default();
    cache.rebuild(&apps, &ranking);
    let recent = HashSet::new();
    let mut matches = |query: &str, filter| {
        cache.update(&apps, &ranking, query, filter, 0, &recent);
        cache
            .indices
            .iter()
            .map(|&i| apps[i].id.as_str())
            .collect::<Vec<_>>()
    };

    let cmd_query = format!(r#""{}""#, cmd.display());
    assert_eq!(matches(&cmd_query, 0), ["cmd"]);
    assert_eq!(matches(&cmd_query, 3), ["cmd"]);
    assert!(matches(&cmd_query, 2).is_empty());
    assert_eq!(
        matches(&cmd.to_string_lossy().replace('\\', "/"), 0),
        ["cmd"]
    );

    let bat_query = format!("'{}'", bat.display());
    assert_eq!(matches(&bat_query, 0), ["bat"]);
    assert_eq!(matches(&bat_query, 2), ["bat"]);
    assert!(matches(&bat_query, 3).is_empty());

    // Ordinary word searches remain intentionally broad.
    assert!(matches("launcher", 0).len() >= 5);
    assert_eq!(store.apps().unwrap(), apps);
    assert_eq!(
        std::fs::read(&cmd).unwrap(),
        b"@echo off\r\nrem first command\r\n"
    );
    assert_eq!(
        std::fs::read(&bat).unwrap(),
        b"@echo off\r\nrem other batch\r\n"
    );
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn copied_windows_command_paths_keep_saved_profiles_and_manual_rules_distinct() {
    // Windows' default PATHEXT includes more than .exe, .cmd and .bat.
    // Exercise the real saved SQLite catalog; none of these fixtures is run.
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-command-target-search-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let folder = root.join("Rare Admin Tools");
    std::fs::create_dir_all(&folder).unwrap();
    let store = Store::open(&root.join("profile")).unwrap();
    let extensions = ["com", "vbs", "vbe", "js", "jse", "wsf", "wsh", "msc"];
    for &extension in &extensions {
        let target = folder.join(format!("Inspect.{extension}"));
        let unrelated = folder.join(format!("Inspect.{extension}.backup"));
        std::fs::write(&target, b"synthetic command fixture").unwrap();
        std::fs::write(&unrelated, b"unrelated backup").unwrap();
        store
            .put_app(&App {
                id: format!("direct-{extension}"),
                name: format!("Selected {extension} profile"),
                source: format!(r"C:\Shortcuts\direct-{extension}.lnk"),
                target: target.to_string_lossy().into_owned(),
                policy: Policy::Always,
                valid: true,
                ..Default::default()
            })
            .unwrap();
        store
            .put_app(&App {
                id: format!("note-{extension}"),
                name: format!("Other {extension} profile"),
                source: format!(r"C:\Shortcuts\note-{extension}.lnk"),
                target: r"C:\Other\Unrelated.exe".into(),
                note: target.to_string_lossy().into_owned(),
                policy: Policy::Never,
                valid: true,
                ..Default::default()
            })
            .unwrap();
        store
            .put_app(&App {
                id: format!("backup-{extension}"),
                name: format!("Backup {extension} profile"),
                source: format!(r"C:\Shortcuts\backup-{extension}.lnk"),
                target: unrelated.to_string_lossy().into_owned(),
                policy: Policy::Never,
                valid: true,
                ..Default::default()
            })
            .unwrap();
    }

    let apps = store.apps().unwrap();
    let ranking = rank(&apps, &store.days().unwrap(), &Config::default(), now());
    let mut catalog = CatalogCache::default();
    catalog.rebuild(&apps, &ranking);
    let recent = HashSet::new();
    let mut matches = |query: &str, filter| {
        catalog.update(&apps, &ranking, query, filter, 0, &recent);
        catalog
            .indices
            .iter()
            .map(|&index| apps[index].id.clone())
            .collect::<Vec<_>>()
    };
    for &extension in &extensions {
        let target = folder.join(format!("Inspect.{extension}"));
        let quoted = format!(r#""{}""#, target.display());
        let expected = vec![format!("direct-{extension}")];
        assert_eq!(matches(&quoted, 0), expected, "quoted {extension}");
        assert_eq!(matches(&quoted, 2), expected, "Always {extension}");
        assert!(matches(&quoted, 3).is_empty(), "Never {extension}");
        let slashes = target.to_string_lossy().replace('\\', "/");
        assert_eq!(matches(&slashes, 0), expected, "slash {extension}");
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"synthetic command fixture",
            "source bytes {extension}"
        );
    }
    // Search by a word still finds both actual targets and documented notes.
    assert!(matches("profile", 0).len() >= extensions.len() * 3);
    assert_eq!(store.apps().unwrap(), apps);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}
