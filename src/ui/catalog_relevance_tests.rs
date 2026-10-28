use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

fn with_saved_catalog(size: usize, test: impl FnOnce(&[App], &[Ranked])) {
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-catalog-relevance-{}-{}-{}",
        std::process::id(),
        now(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let store = Store::open(&root).unwrap();
    store.transaction(|| {
        for index in 0..size {
            // Common profile-folder terms match every result by metadata, not name.
            let app = App {
                id: format!("app-{index:04}"),
                name: format!("A{index:04} Utility"),
                source: format!(r"C:\Users\Editor\Work\Shortcuts\A{index:04}.lnk"),
                target: format!(r"C:\Tools\Utility{index:04}.exe"),
                valid: true,
                ..Default::default()
            };
            store.put_app(&app)?;
        }
        for (id, name, policy) in [
            ("exact", "Editor", Policy::Never),
            ("prefix", "Editor Work", Policy::Auto),
            ("phrase", "Work Editor", Policy::Auto),
            ("words", "My Editor Notes", Policy::Auto),
        ] {
            store.put_app(&App {
                id: id.into(),
                name: name.into(),
                source: format!(r"C:\Users\Editor\Work\Shortcuts\{id}.lnk"),
                target: format!(r"C:\Tools\{id}.exe"),
                policy,
                valid: true,
                ..Default::default()
            })?;
        }
        store.observe("app-0000", 1200, true, false)?;
        Ok(())
    }).unwrap();
    let mut apps = store.apps().unwrap();
    apps.sort_by_key(|app| app.name.to_lowercase());
    let ranking = rank(&apps, &store.days().unwrap(), &Config::default(), now());
    test(&apps, &ranking);
    drop(store);
    std::fs::remove_dir_all(root).unwrap();
}

fn names(catalog: &CatalogCache, apps: &[App]) -> Vec<String> {
    catalog.indices.iter().map(|&index| apps[index].name.clone()).collect()
}

#[test]
fn exact_name_beats_thousands_of_metadata_matches_without_losing_results() {
    for size in [100, 1000, 5000] {
        with_saved_catalog(size, |apps, ranking| {
            let mut cache = CatalogCache::default();
            cache.rebuild(apps, ranking);
            cache.update(apps, ranking, " EDITOR ", 0, 0, &HashSet::new());
            let matched = names(&cache, apps);
            assert_eq!(matched.len(), size + 4);
            assert_eq!(matched[0], "Editor", "exact name should be immediately visible");
            assert_eq!(matched[1], "Editor Work", "prefix name should follow exact");
            assert_eq!(matched[2], "Work Editor", "name phrase before path matches");
            assert_eq!(matched[3], "My Editor Notes");
            assert!(matched[4].starts_with('A'), "metadata-only results must follow names");
            let mut unique = HashSet::new();
            assert!(cache.indices.iter().all(|index| unique.insert(*index)));
            assert_eq!(unique.len(), size + 4);
        });
    }
}

#[test]
fn multiword_query_prefers_exact_title_but_keeps_unordered_word_and_path_matches() {
    with_saved_catalog(100, |apps, ranking| {
        let mut cache = CatalogCache::default();
        cache.rebuild(apps, ranking);
        cache.update(apps, ranking, "  WORK   Editor ", 0, 0, &HashSet::new());
        let matched = names(&cache, apps);
        assert_eq!(matched.len(), 104);
        assert_eq!(matched[0], "Work Editor");
        assert_eq!(matched[1], "Editor Work");
        assert_eq!(matched[2], "My Editor Notes");
        cache.update(apps, ranking, "", 0, 0, &HashSet::new());
        assert_eq!(names(&cache, apps)[0], "A0000 Utility", "empty query keeps alphabetic order");
    });
}

#[test]
fn explicit_sort_and_manual_rule_filters_are_not_overridden_by_relevance() {
    with_saved_catalog(100, |apps, ranking| {
        let mut cache = CatalogCache::default();
        cache.rebuild(apps, ranking);
        cache.update(apps, ranking, "editor", 0, 2, &HashSet::new());
        assert_eq!(names(&cache, apps)[0], "A0000 Utility", "explicit usage sort must win");
        cache.update(apps, ranking, "editor", 3, 0, &HashSet::new());
        assert_eq!(names(&cache, apps), ["Editor"], "Never filter must remain exact");
        cache.update(apps, ranking, "editor", 2, 0, &HashSet::new());
        assert!(cache.indices.is_empty());
    });
}
