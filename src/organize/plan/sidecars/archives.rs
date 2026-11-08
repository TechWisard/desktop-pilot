//! Recognize named archive volumes without opening or extracting their contents.
//! Input is the same bounded, non-recursive directory snapshot used by the plan.
use super::norm_literal_path;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

fn digits(value: &str, minimum: usize) -> bool {
    value.len() >= minimum && value.bytes().all(|byte| byte.is_ascii_digit())
}

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    let by_name: HashMap<_, _> = paths
        .iter()
        .filter_map(|path| {
            Some((
                path.file_name()?.to_string_lossy().to_lowercase(),
                norm_literal_path(&path.to_string_lossy()),
            ))
        })
        .collect();
    let mut protected = HashSet::new();
    let mut numbered = HashMap::<(String, usize), Vec<String>>::new();
    for (name, key) in &by_name {
        let Some((stem, extension)) = name.rsplit_once('.') else {
            continue;
        };
        if stem.is_empty() {
            continue;
        }
        // ZIP volumes use name.z01, name.z02, ..., name.zip. The numbered
        // parts are already ineligible ordinary files; protect the .zip too.
        // Require the exact basename, not an arbitrary prefix match.
        let main_extension = if extension
            .strip_prefix('z')
            .is_some_and(|number| digits(number, 2) && number.bytes().any(|byte| byte != b'0'))
        {
            Some("zip")
        } else if extension
            .strip_prefix('r')
            .is_some_and(|number| number.len() == 2 && digits(number, 2))
        {
            // Older RAR sets start with name.rar, followed by name.r00, etc.
            Some("rar")
        } else {
            None
        };
        if let Some(extension) = main_extension
            && let Some(main) = by_name.get(&format!("{stem}.{extension}"))
        {
            protected.insert(main.clone());
        }
        if extension != "rar" {
            continue;
        }
        let Some((base, part)) = stem.rsplit_once('.') else {
            continue;
        };
        if base.is_empty() {
            continue;
        }
        if part
            .strip_prefix("part")
            .is_some_and(|number| digits(number, 1))
        {
            // An explicitly named volume stays put even if the next download
            // has not arrived. Selecting just one volume must not split a set.
            protected.insert(key.clone());
        } else if digits(part, 3) {
            // Numeric-only names are ambiguous (for example Report.2026.rar).
            // Require at least two distinct sibling names of the same width.
            numbered
                .entry((base.to_owned(), part.len()))
                .or_default()
                .push(key.clone());
        }
    }
    for parts in numbered.into_values().filter(|parts| parts.len() >= 2) {
        protected.extend(parts);
    }
    protected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_volume_names_preserve_exact_sets_not_similar_standalone_names() {
        let paths: Vec<_> = [
            "Backup %USERNAME%.ZIP",
            "Backup %USERNAME%.Z01",
            "Backup %USERNAME%.z100",
            "Legacy.RAR",
            "Legacy.r00",
            "Съёмка.part001.rar",
            "Съёмка.part002.rar",
            "Incomplete.part1.rar",
            "Numeric.000.rar",
            "Numeric.001.rar",
            "Other.zip",
            "Other.z-notes",
            "Notes.rar",
            "Notes.r0",
            "Notes.r100",
            "Report.2026.rar",
            "Report.001.rar",
            "Book.partdraft.rar",
            "Backup-more.zip",
            "Zero.zip",
            "Zero.z00",
            "Orphan.z01",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let expected: HashSet<_> = [
            "backup %username%.zip",
            "legacy.rar",
            "съёмка.part001.rar",
            "съёмка.part002.rar",
            "incomplete.part1.rar",
            "numeric.000.rar",
            "numeric.001.rar",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn a_duplicate_numeric_path_is_not_evidence_of_another_volume() {
        let paths = vec![PathBuf::from("Report.001.rar"); 2];
        assert!(protected(&paths).is_empty());
    }

    #[test]
    fn full_bounded_archive_snapshot_uses_only_indexed_names() {
        let mut paths = Vec::new();
        for index in 0..3000 {
            paths.push(PathBuf::from(format!("Архив {index}.zip")));
            paths.push(PathBuf::from(format!("Архив {index}.z01")));
        }
        let result = protected(&paths);
        assert_eq!(result.len(), 3000);
        assert!(result.contains("архив 0.zip"));
        assert!(result.contains("архив 2999.zip"));
    }
}
