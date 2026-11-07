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
