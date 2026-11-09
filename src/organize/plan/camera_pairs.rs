//! Protect plausible camera RAW+JPEG and Live Photo still+movie pairs.
//! File names are conservative evidence of association, not a content check.
//! The caller supplies one bounded Desktop snapshot; never open image files.
use super::norm_literal_path;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

#[derive(Default)]
struct Pair {
    has_raw: bool,
    jpegs: Vec<String>,
    stills: Vec<String>,
    movies: Vec<String>,
}

fn raw_extension(extension: &str) -> bool {
    matches!(
        extension,
        "3fr"
            | "arw"
            | "cr2"
            | "cr3"
            | "dng"
            | "erf"
            | "iiq"
            | "k25"
            | "kdc"
            | "mef"
            | "mos"
            | "mrw"
            | "nef"
            | "nrw"
            | "orf"
            | "pef"
            | "raf"
            | "raw"
            | "rw2"
            | "rwl"
            | "sr2"
            | "srf"
            | "srw"
            | "x3f"
    )
}

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    let mut pairs = HashMap::<String, Pair>::new();
    for path in paths {
        // Use only lossless basenames. The normal analyzer independently
        // rejects names that Windows cannot represent safely.
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if stem.is_empty() {
            continue;
        }
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let raw = raw_extension(&extension);
        let jpeg = matches!(extension.as_str(), "jpg" | "jpeg");
        let still = jpeg || matches!(extension.as_str(), "heic" | "heif");
        let movie = extension == "mov";
        if !(raw || still || movie) {
            continue;
        }

        let pair = pairs.entry(stem.to_lowercase()).or_default();
        if raw {
            pair.has_raw = true;
        }
        if still || movie {
            let filename = norm_literal_path(&path.to_string_lossy());
            if jpeg {
                pair.jpegs.push(filename.clone());
            }
            if still {
                pair.stills.push(filename.clone());
            }
            if movie {
                pair.movies.push(filename);
            }
        }
    }

    let mut protected = HashSet::new();
    for pair in pairs.into_values() {
        // RAW is not itself an eligible ordinary-file group. Keep its JPEG
        // sibling instead of leaving RAW behind after moving only the JPEG.
        if pair.has_raw {
            protected.extend(pair.jpegs);
        }
        // Apple Photos exports a Live Photo as a still and a movie. When
        // original basenames match, leave both eligible pieces together.
        // Do not assume an arbitrary MP4 is a Live Photo movie.
        if !pair.stills.is_empty() && !pair.movies.is_empty() {
            protected.extend(pair.stills);
            protected.extend(pair.movies);
        }
    }
    protected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_camera_pairs_are_protected_without_capturing_other_files() {
        let paths = [
            "DSC_001.CR3",
            "DSC_001.JPG",
            "DSC_002.NEF",
            "DSC_002.jpeg",
            "Live One.HEIC",
            "Live One.MOV",
            "Live Two.jpg",
            "Live Two.mov",
            "ImageOnly.heic",
            "MovieOnly.mov",
            "Unrelated.jpg",
            "Unrelated.mp4",
            "Snapshot.dng",
            "Snapshot-other.jpeg",
            "RawOnly.raf",
            "ImageOnlyTwo.jpeg",
            "SimilarShot.mov",
            "SimilarShotFinal.jpeg",
            "Фото%USERNAME%.CR2",
            "Фото%USERNAME%.JPG",
        ]
        .map(PathBuf::from);
        let expected: HashSet<_> = [
            "dsc_001.jpg",
            "dsc_002.jpeg",
            "live one.heic",
            "live one.mov",
            "live two.jpg",
            "live two.mov",
            "фото%username%.jpg",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn six_thousand_names_are_scanned_without_reading_media_contents() {
        let mut paths = Vec::new();
        for index in 0..1500 {
            paths.push(PathBuf::from(format!("Camera {index}.CR3")));
            paths.push(PathBuf::from(format!("Camera {index}.JPG")));
            paths.push(PathBuf::from(format!("Live {index}.HEIC")));
            paths.push(PathBuf::from(format!("Live {index}.MOV")));
        }
        assert_eq!(paths.len(), 6000);
        let protected = protected(&paths);
        assert_eq!(protected.len(), 4500);
        assert!(protected.contains("camera 0.jpg"));
        assert!(protected.contains("live 1499.heic"));
        assert!(protected.contains("live 1499.mov"));
    }
}
