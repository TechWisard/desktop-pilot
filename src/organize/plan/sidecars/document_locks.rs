//! Keep document owner markers and their editable documents in place.
//! A marker indicates a possible active editor or stale lock, not proof of
//! an open handle. Do not read, remove or interpret marker contents.
use super::norm_literal_path;
use std::{collections::HashSet, path::PathBuf};

fn editable_document(name: &str) -> bool {
    let Some((_, extension)) = name.rsplit_once('.') else {
        return false;
    };
    matches!(extension, "doc" | "docx" | "odt" | "ppt" | "pptx" | "rtf")
}

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    // One case-normalized snapshot. The analyzer and Apply both call this
    // with their own current bounded desktop listing, so a late owner marker
    // invalidates an earlier plan before any rename.
    let names: HashSet<String> = paths
        .iter()
        .filter_map(|path| path.file_name().map(|name| name.to_string_lossy().to_lowercase()))
        .collect();

    paths
        .iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().to_lowercase();
            if !editable_document(&name) {
                return None;
            }
            // Office owner files can have an ordinary document extension.
            // Even an orphan must never become a proposed "document".
            if name.starts_with("~$") {
                return Some(norm_literal_path(&path.to_string_lossy()));
            }
            // Word's documented owner-file spelling replaces the first two
            // original filename characters with "~$" (Document -> ~$cument).
            // Some versions also use "~$Document". Require an exact sibling
            // instead of a loose prefix match. In ambiguous cases keep both
            // candidates: a false negative could disrupt an open document.
            let after_two = name.chars().skip(2).collect::<String>();
            let office = names.contains(&format!("~${name}"))
                || (!after_two.is_empty() && names.contains(&format!("~${after_two}")));
            // LibreOffice uses .~lock.<full filename>#.
            let libre = names.contains(&format!(".~lock.{name}#"));
            (office || libre).then(|| norm_literal_path(&path.to_string_lossy()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_owner_markers_keep_editable_documents_not_similar_names() {
        let paths: Vec<_> = [
            "Document.DOCX",
            "~$cument.docx",
            "Summary.docx",
            "~$Summary.docx",
            "Budget.odt",
            ".~lock.Budget.odt#",
            "Budget copy.odt",
            "Unrelated.docx",
            "Archive.pdf",
            ".~lock.Archive.pdf#",
            "~$orphan.docx",
            "Letter.txt",
            ".~lock.Letter.txt#",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let expected: HashSet<_> = [
            "document.docx",
            "~$cument.docx",
            "summary.docx",
            "~$summary.docx",
            "budget.odt",
            "~$orphan.docx",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn ambiguous_owner_suffix_keeps_every_possible_source() {
        let paths = [
            "ABsame.docx",
            "CDsame.docx",
            "~$same.docx",
            "Other.docx",
        ]
        .map(PathBuf::from);
        let expected: HashSet<_> = ["absame.docx", "cdsame.docx", "~$same.docx"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn full_six_thousand_entry_snapshot_needs_no_content_reads() {
        let mut paths = Vec::new();
        for index in 0..3000 {
            paths.push(PathBuf::from(format!("Документ {index}.docx")));
            paths.push(PathBuf::from(format!(".~lock.Документ {index}.docx#")));
        }
        let result = protected(&paths);
        assert_eq!(result.len(), 3000);
        assert!(result.contains("документ 0.docx"));
        assert!(result.contains("документ 2999.docx"));
    }
}
