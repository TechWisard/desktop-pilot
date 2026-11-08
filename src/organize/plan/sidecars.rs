//! Keep recognized associated file sets in place rather than splitting them.
//! Names are evidence of a possible dependency, not proof of file contents.
use super::{Group, norm_literal_path, plain_group};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

mod archives;
mod document_locks;

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    let mut by_name = HashMap::new();
    let mut by_stem = HashMap::<String, Vec<String>>::new();
    for path in paths {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(plain_group(&extension), Some(Group::Images | Group::Media)) {
            continue;
        }
        let (Some(name), Some(stem)) = (path.file_name(), path.file_stem()) else {
            continue;
        };
        let key = norm_literal_path(&path.to_string_lossy());
        by_name.insert(name.to_string_lossy().to_lowercase(), key.clone());
        by_stem
            .entry(stem.to_string_lossy().to_lowercase())
            .or_default()
            .push(key);
    }
    let mut protected = archives::protected(paths);
    protected.extend(document_locks::protected(paths));
    for path in paths {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !matches!(
            extension.as_str(),
            "xmp"
                | "acr"
                | "thm"
                | "lrv"
                | "dtdata"
                | "aae"
                | "pp3"
                | "dop"
                | "srt"
                | "sub"
                | "ass"
                | "ssa"
                | "vtt"
                | "idx"
                | "cdg"
                | "cue"
        ) {
            continue;
        }
        let Some(stem) = path.file_stem() else {
            continue;
        };
        let stem = stem.to_string_lossy().to_lowercase();
        // GoPro high-resolution clips are named GH/GX + six digits, whereas
        // the low-resolution companion can be GL + the same six digits.
        // Match an actual adjacent MP4/MOV, never a merely similar prefix.
        if extension == "lrv"
            && stem.len() == 8
            && stem.starts_with("gl")
            && stem[2..].bytes().all(|byte| byte.is_ascii_digit())
        {
            for prefix in ["gh", "gx"] {
                for video_ext in ["mp4", "mov"] {
                    if let Some(key) = by_name.get(&format!("{prefix}{}.{video_ext}", &stem[2..])) {
                        protected.insert(key.clone());
                    }
                }
            }
        }
        let mut candidate = stem.as_str();
        loop {
            // Both Photo.jpg.xmp and Photo.xmp are in common use. Caption
            // names may include language/variant suffixes (Film.en.forced.srt).
            if let Some(key) = by_name.get(candidate) {
                protected.insert(key.clone());
            }
            if let Some(keys) = by_stem.get(candidate) {
                protected.extend(keys.iter().cloned());
            }
            // Non-destructive editors can keep several edits of one original,
            // e.g. Photo_01.jpg.xmp. Preserve the original even when the main
            // Photo.jpg.xmp is missing. Do not strip arbitrary underscores.
            if let Some((base, version)) = candidate.rsplit_once('_')
                && !version.is_empty()
                && version.bytes().all(|byte| byte.is_ascii_digit())
                && let Some(keys) = by_stem.get(base)
            {
                protected.extend(keys.iter().cloned());
            }
            let Some((base, _)) = candidate.rsplit_once('.') else {
                break;
            };
            candidate = base;
        }
    }
    protected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_name_conventions_preserve_only_possible_media_dependencies() {
        let paths: Vec<_> = [
            "Photo.JPG",
            "Photo_02.jpg.xmp",
            "Film.mp4",
            "Film.en.forced.srt",
            "Album.flac",
            "album.cue",
            "Unrelated.jpg",
            "Photograph.xmp",
            "Document.txt",
            "Document.xmp",
            "Site.url",
            "Site.srt",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let expected: HashSet<_> = ["photo.jpg", "film.mp4", "album.flac"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn camera_and_editor_sidecars_keep_only_their_existing_companions() {
        let paths: Vec<_> = [
            "Edited.JPG",
            "Edited.acr",
            "Edited.jpg.XMP",
            "GX012345.MP4",
            "GX012345.THM",
            "GL012345.LRV",
            "GH018765.mp4",
            "GL018765.lrv",
            "Clip.mov",
            "Clip.lrv",
            "Standalone.jpg",
            "Unrelated.mp4",
            "Orphan.acr",
            "GL999999.lrv",
            "GL-unrelated.lrv",
        ]
        .into_iter()
        .map(PathBuf::from)
        .collect();
        let expected: HashSet<_> = ["edited.jpg", "gx012345.mp4", "gh018765.mp4", "clip.mov"]
            .into_iter()
            .map(String::from)
            .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn unicode_names_and_a_full_bounded_snapshot_do_not_require_file_io() {
        let mut paths = Vec::new();
        for index in 0..3000 {
            paths.push(PathBuf::from(format!("Фото {index}.jpg")));
            paths.push(PathBuf::from(format!("Фото {index}.jpg.xmp")));
        }
        let result = protected(&paths);
        assert_eq!(result.len(), 3000);
        assert!(result.contains("фото 0.jpg"));
        assert!(result.contains("фото 2999.jpg"));
    }
}
