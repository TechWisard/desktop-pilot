//! Preserve videos whose external subtitles are in a sibling Subs/Subtitles folder.
//! Read a bounded list of names through a verified directory handle. Never open
//! subtitle contents, follow a junction, or recurse into a media library.
use super::{Group, norm_literal_path, plain_group};
use crate::organize::fs;
use anyhow::{Result, bail, ensure};
use std::{
    collections::{HashMap, HashSet},
    fs::{File, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        FILE_ID_BOTH_DIR_INFO, FileIdBothDirectoryInfo, GetFileInformationByHandleEx,
    },
};

const FOLDER_ENTRY_LIMIT: usize = 6000;

// A directory handle stays bound to the original non-reparse directory even
// if someone renames or replaces its path while this read is in progress.
// FileIdBothDirectoryInfo enumerates names and attributes, not file payloads.
fn names_from_handle(folder: &File, cancel: &AtomicBool) -> Result<Vec<String>> {
    let mut names = Vec::new();
    let mut buffer = [0u64; 4096]; // 32 KiB, naturally eight-byte aligned.
    let capacity = std::mem::size_of_val(&buffer);
    let header = std::mem::offset_of!(FILE_ID_BOTH_DIR_INFO, FileName);
    loop {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Organization analysis cancelled"
        );
        match unsafe {
            GetFileInformationByHandleEx(
                HANDLE(folder.as_raw_handle()),
                FileIdBothDirectoryInfo,
                buffer.as_mut_ptr().cast(),
                capacity as u32,
            )
        } {
            Ok(()) => {}
            // ERROR_NO_MORE_FILES is returned after the final batch.
            Err(error) if error.code().0 as u32 == 0x8007_0012 => break,
            Err(error) => return Err(error.into()),
        }
        let bytes = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), capacity) };
        let mut offset = 0usize;
        loop {
            ensure!(
                offset + std::mem::size_of::<FILE_ID_BOTH_DIR_INFO>() <= capacity
                    && offset % 8 == 0,
                "Invalid subtitle directory enumeration"
            );
            let entry = unsafe { &*bytes.as_ptr().add(offset).cast::<FILE_ID_BOTH_DIR_INFO>() };
            let length = entry.FileNameLength as usize;
            ensure!(
                length > 0 && length % 2 == 0 && offset + header + length <= capacity,
                "Invalid subtitle filename in directory enumeration"
            );
            let start = offset + header;
            let units = unsafe {
                std::slice::from_raw_parts(bytes.as_ptr().add(start).cast::<u16>(), length / 2)
            };
            let name = String::from_utf16(units)?;
            if name != "." && name != ".." {
                names.push(name);
                ensure!(
                    names.len() <= FOLDER_ENTRY_LIMIT,
                    "Subtitle directory contains too many entries"
                );
            }
            let next = entry.NextEntryOffset as usize;
            if next == 0 {
                break;
            }
            ensure!(
                next % 8 == 0 && next >= header + length && offset + next < capacity,
                "Invalid subtitle directory entry offset"
            );
            offset += next;
        }
    }
    Ok(names)
}

fn subtitle_name_stem(name: &str) -> Option<&str> {
    let (stem, extension) = name.rsplit_once('.')?;
    (!stem.is_empty()
        && matches!(
            extension.to_ascii_lowercase().as_str(),
            "srt" | "smi" | "ssa" | "ass" | "vtt" | "sub" | "idx"
        ))
    .then_some(stem)
}

fn matching_video_keys(
    filename: &str,
    by_stem: &HashMap<String, Vec<String>>,
    protected: &mut HashSet<String>,
) {
    let Some(stem) = subtitle_name_stem(filename) else {
        return;
    };
    let name = stem.to_lowercase();
    let mut candidate = name.as_str();
    loop {
        // Language and presentation qualifiers, e.g. Film.en.forced.srt,
        // may be dotted. Never match merely a shared filename prefix.
        if let Some(keys) = by_stem.get(candidate) {
            protected.extend(keys.iter().cloned());
        }
        let Some((parent, _)) = candidate.rsplit_once('.') else {
            break;
        };
        candidate = parent;
    }
}

pub(in crate::organize::plan) fn protected(
    paths: &[PathBuf],
    cancel: &AtomicBool,
) -> Result<HashSet<String>> {
    let mut videos = HashMap::<String, Vec<String>>::new();
    let mut all_video_paths = Vec::new();
    for path in paths {
        let (Some(stem), Some(extension)) = (
            path.file_stem().and_then(|name| name.to_str()),
            path.extension().and_then(|ext| ext.to_str()),
        ) else {
            continue;
        };
        if !matches!(
            extension.to_ascii_lowercase().as_str(),
            "mp4" | "mkv" | "avi" | "mov" | "webm"
        ) || !matches!(
            plain_group(&extension.to_ascii_lowercase()),
            Some(Group::Media)
        ) {
            continue;
        }
        let key = norm_literal_path(&path.to_string_lossy());
        videos
            .entry(stem.to_lowercase())
            .or_default()
            .push(key.clone());
        all_video_paths.push(key);
    }
    let mut marked = HashSet::new();
    if videos.is_empty() {
        return Ok(marked);
    }
    for folder in paths.iter().filter(|path| {
        path.file_name().is_some_and(|name| {
            name.to_str().is_some_and(|name| {
                name.eq_ignore_ascii_case("subs") || name.eq_ignore_ascii_case("subtitles")
            })
        })
    }) {
        if cancel.load(Ordering::Relaxed) {
            bail!("Organization analysis cancelled");
        }
        let read = (|| -> Result<Option<Vec<String>>> {
            let metadata = folder.symlink_metadata()?;
            // An ordinary file named Subs is not a directory dependency.
            // A link, cloud placeholder or uninspectable directory is unknown.
            if metadata.is_file() && metadata.file_attributes() & 0x400 == 0 {
                return Ok(None);
            }
            // The generic organizer directory handle intentionally needs only
            // FILE_READ_ATTRIBUTES (0x80), sufficient for identity checks but
            // not for enumerating directory entries on Windows. Open a
            // separate, non-reparse handle with FILE_LIST_DIRECTORY (0x1);
            // validate its attributes on the same handle before reading.
            let _cloud = fs::cloud_visibility::expose()?;
            let handle = OpenOptions::new()
                .access_mode(0x81)
                .share_mode(3)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(folder)?;
            let checked = handle.metadata()?;
            ensure!(
                checked.is_dir() && checked.file_attributes() & 0x400 == 0,
                "A filesystem link cannot be used as an organization folder"
            );
            Ok(Some(names_from_handle(&handle, cancel)?))
        })();
        match read {
            Ok(Some(names)) => {
                for name in names {
                    matching_video_keys(&name, &videos, &mut marked);
                }
            }
            Ok(None) => {}
            Err(_) if cancel.load(Ordering::Relaxed) => {
                bail!("Organization analysis cancelled");
            }
            Err(_) => {
                // Unknown means preserve all candidate videos, but do not
                // penalize independent shortcuts, pictures or documents.
                marked.extend(all_video_paths.iter().cloned());
            }
        }
    }
    Ok(marked)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_actual_list_directory_handle_reads_existing_subtitle_names() {
        let directory = std::env::temp_dir().join(format!(
            "pilot-subtitle-handle-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("Episode.en.srt"), b"example").unwrap();
        // This tests the actual Windows information class and permissions,
        // not only the pure filename matching function.
        let _cloud = fs::cloud_visibility::expose().unwrap();
        let handle = OpenOptions::new()
            .access_mode(0x81)
            .share_mode(3)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(&directory)
            .unwrap();
        let names = names_from_handle(&handle, &AtomicBool::new(false)).unwrap();
        assert!(names.iter().any(|name| name == "Episode.en.srt"), "{names:?}");
        drop(handle);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn dotted_language_and_forced_suffixes_match_only_exact_video_stems() {
        let videos = HashMap::from([
            ("episode".into(), vec!["episode.mp4".into()]),
            ("episode 2".into(), vec!["episode 2.mp4".into()]),
            ("песня".into(), vec!["песня.mkv".into()]),
        ]);
        let mut marked = HashSet::new();
        for name in [
            "Episode.en.forced.srt",
            "Песня.ru.smi",
            "Episode 2.eng.ssa",
            "Other.ass",
            "Episode2.srt",
            "Episode.en.txt",
        ] {
            matching_video_keys(name, &videos, &mut marked);
        }
        assert_eq!(
            marked,
            HashSet::from([
                "episode.mp4".into(),
                "episode 2.mp4".into(),
                "песня.mkv".into(),
            ])
        );
    }
}
