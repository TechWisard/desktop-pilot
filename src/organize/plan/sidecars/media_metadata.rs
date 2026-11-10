//! Preserve a video's exact-name local NFO metadata and recognized artwork.
//! Both are kept beside a video in opt-in desktop cleanup; names are only
//! possible-dependency evidence. Never read media/metadata contents.
use super::norm_literal_path;
use std::{collections::{HashMap, HashSet}, path::PathBuf};

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    let mut videos = HashMap::<String, Vec<String>>::new();
    for path in paths {
        let (Some(stem), Some(extension)) = (
            path.file_stem().and_then(|part| part.to_str()),
            path.extension().and_then(|part| part.to_str()),
        ) else {
            continue;
        };
        if stem.is_empty()
            || !matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4" | "mkv" | "avi" | "mov" | "webm"
            )
        {
            continue;
        }
        videos
            .entry(stem.to_lowercase())
            .or_default()
            .push(norm_literal_path(&path.to_string_lossy()));
    }
    let mut kept = HashSet::new();
    for path in paths {
        let (Some(stem), Some(extension)) = (
            path.file_stem().and_then(|part| part.to_str()),
            path.extension().and_then(|part| part.to_str()),
        ) else {
            continue;
        };
        let extension = extension.to_ascii_lowercase();
        let original = if extension == "nfo" {
            // Kodi and Jellyfin use VideoFilename.nfo for local metadata.
            Some(stem)
        } else if matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp") {
            // Emby recognizes VideoFilename-poster/cover/thumb/fanart.
            // Match complete stems only: Movie-poster-old is unrelated.
            stem.rsplit_once('-').and_then(|(base, kind)| {
                matches!(
                    kind.to_ascii_lowercase().as_str(),
                    "poster" | "cover" | "thumb" | "fanart"
                )
                .then_some(base)
            })
        } else {
            None
        };
        if let Some(videos) = original.and_then(|base| videos.get(&base.to_lowercase())) {
            kept.extend(videos.iter().cloned());
            // Artwork is itself movable into Images. Keep it alongside the
            // video; NFO is already excluded as an unknown plain-file type.
            if extension != "nfo" {
                kept.insert(norm_literal_path(&path.to_string_lossy()));
            }
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_video_metadata_and_artwork_do_not_protect_unrelated_photos() {
        let paths = [
            "Film.mkv",
            "Film.NFO",
            "Film-poster.JPG",
            "Film-fanart.PNG",
            "Film-thumb.WEBP",
            "Film-cover.jpeg",
            "Другой фильм.mp4",
            "Другой фильм-poster.png",
            "Different.mov",
            "Different-poster-old.jpg",
            "Picture.jpg",
            "Picture.nfo",
            "Film2.mp4",
            "Film22-poster.jpg",
        ]
        .map(PathBuf::from);
        assert_eq!(
            protected(&paths),
            [
                "film.mkv",
                "film-poster.jpg",
                "film-fanart.png",
                "film-thumb.webp",
                "film-cover.jpeg",
                "другой фильм.mp4",
                "другой фильм-poster.png",
            ]
            .into_iter()
            .map(String::from)
            .collect()
        );
    }

    #[test]
    fn four_thousand_names_are_checked_with_exact_indexed_matches() {
        let mut paths = Vec::new();
        for index in 0..1000 {
            paths.push(PathBuf::from(format!("Clip {index}.mp4")));
            paths.push(PathBuf::from(format!("Clip {index}.nfo")));
            paths.push(PathBuf::from(format!("Clip {index}-poster.jpg")));
            paths.push(PathBuf::from(format!("Other {index}-poster.jpg")));
        }
        let kept = protected(&paths);
        assert_eq!(kept.len(), 2000);
        assert!(kept.contains("clip 0.mp4"));
        assert!(kept.contains("clip 999-poster.jpg"));
        assert!(!kept.contains("other 999-poster.jpg"));
    }
}
