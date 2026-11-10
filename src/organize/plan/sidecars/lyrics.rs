//! Keep same-name external lyrics beside movable audio without reading payloads.
use super::norm_literal_path;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

pub(super) fn protected(paths: &[PathBuf]) -> HashSet<String> {
    let mut audio = HashMap::<String, Vec<String>>::new();
    let mut lyrics = HashMap::<String, Vec<String>>::new();
    for path in paths {
        let (Some(stem), Some(extension)) = (
            path.file_stem().and_then(|part| part.to_str()),
            path.extension().and_then(|part| part.to_str()),
        ) else {
            continue;
        };
        if stem.is_empty() {
            continue;
        }
        let entries = match extension.to_ascii_lowercase().as_str() {
            // Only audio formats the optional organizer actually moves.
            "mp3" | "wav" | "flac" | "ogg" | "m4a" => &mut audio,
            // TXT can itself move to Documents; keep both sides of the pair.
            "lrc" | "txt" => &mut lyrics,
            _ => continue,
        };
        entries
            .entry(stem.to_lowercase())
            .or_default()
            .push(norm_literal_path(&path.to_string_lossy()));
    }
    let mut protected = HashSet::new();
    for (stem, companions) in lyrics {
        if let Some(tracks) = audio.get(&stem) {
            protected.extend(tracks.iter().cloned());
            protected.extend(companions);
        }
    }
    protected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_audio_lyrics_are_protected_without_claiming_unrelated_media() {
        let paths = [
            "Песня%USERNAME%.MP3",
            "Песня%USERNAME%.LRC",
            "Album.FLAC",
            "Album.txt",
            "Mix.M4A",
            "Mix.Lrc",
            "Tape.OGG",
            "Tape.TXT",
            "Voice.WAV",
            "Voice.lrc",
            "Movie.mp4",
            "Movie.txt",
            "Photo.jpg",
            "Photo.txt",
            "Unrelated.mp3",
            "UnrelatedNotes.lrc",
            "Orphan.lrc",
            "Letter.txt",
        ]
        .map(PathBuf::from);
        let expected: HashSet<_> = [
            "песня%username%.mp3",
            "песня%username%.lrc",
            "album.flac",
            "album.txt",
            "mix.m4a",
            "mix.lrc",
            "tape.ogg",
            "tape.txt",
            "voice.wav",
            "voice.lrc",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        assert_eq!(protected(&paths), expected);
    }

    #[test]
    fn six_thousand_names_use_bounded_exact_stem_matching() {
        let mut paths = Vec::new();
        for index in 0..2000 {
            paths.push(PathBuf::from(format!("Track {index}.mp3")));
            paths.push(PathBuf::from(format!("Track {index}.lrc")));
            paths.push(PathBuf::from(format!("Track {index}.txt")));
        }
        let result = protected(&paths);
        assert_eq!(result.len(), 6000);
        assert!(result.contains("track 0.mp3"));
        assert!(result.contains("track 1999.txt"));
    }
}
