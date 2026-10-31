//! Same-volume, identity-checked renames. No copy/delete fallback or overwrite.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::Read,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path},
    time::UNIX_EPOCH,
};
use windows::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FileRenameInfo, GetFileInformationByHandle,
        SetFileInformationByHandle,
    },
};

const UNSAFE_ATTRIBUTES: u32 = 0x2 | 0x4 | 0x400 | 0x1000 | 0x40000 | 0x400000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct Stamp {
    pub id: String,
    pub size: u64,
    pub modified: u64,
    pub digest: Option<String>,
}

pub(super) fn leaf(name: &str) -> Result<()> {
    let mut components = Path::new(name).components();
    ensure!(
        matches!(components.next(), Some(Component::Normal(_)))
            && components.next().is_none()
            && !name.contains(['/', '\\', ':', '\0'])
            && !name.ends_with(['.', ' ']),
        "Invalid organization filename"
    );
    Ok(())
}

pub(super) fn identity(file: &File) -> Result<String> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    unsafe {
        GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info)?;
    }
    ensure!(
        info.nFileIndexHigh != 0 || info.nFileIndexLow != 0,
        "The filesystem cannot identify this object safely"
    );
    Ok(format!(
        "{:08x}-{:08x}{:08x}",
        info.dwVolumeSerialNumber, info.nFileIndexHigh, info.nFileIndexLow
    ))
}

pub(super) fn directory(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .access_mode(0x80)
        .share_mode(3)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_dir() && metadata.file_attributes() & 0x400 == 0,
        "A filesystem link cannot be used as an organization folder"
    );
    Ok(file)
}

pub(super) fn stamp(file: &mut File, hash_content: bool) -> Result<Stamp> {
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.file_attributes() & UNSAFE_ATTRIBUTES == 0,
        "Protected, cloud-placeholder and filesystem-link objects are left in place"
    );
    let digest = if hash_content {
        ensure!(
            before.len() <= 1024 * 1024,
            "Shortcut is too large to inspect safely"
        );
        let mut bytes = Vec::new();
        (&mut *file).take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= 1024 * 1024,
            "Shortcut changed while being inspected"
        );
        Some(crate::model::hash(&bytes))
    } else {
        None
    };
    let after = file.metadata()?;
    ensure!(
        before.len() == after.len() && before.last_write_time() == after.last_write_time(),
        "Object changed while being inspected"
    );
    Ok(Stamp {
        id: identity(file)?,
        size: after.len(),
        modified: after.last_write_time(),
        digest,
    })
}

pub(super) fn inspect(path: &Path, shortcut: bool) -> Result<Stamp> {
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file() && metadata.file_attributes() & UNSAFE_ATTRIBUTES == 0,
        "Protected, cloud-placeholder and filesystem-link objects are left in place"
    );
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .custom_flags(0x0020_0000)
        .open(path)?;
    stamp(&mut file, shortcut)
}

pub(super) fn matches_id(path: &Path, expected: &str) -> Result<bool> {
    match OpenOptions::new()
        .access_mode(0x80)
        .share_mode(7)
        .custom_flags(0x0020_0000)
        .open(path)
    {
        Ok(file) => {
            let metadata = file.metadata()?;
            Ok(metadata.is_file()
                && metadata.file_attributes() & UNSAFE_ATTRIBUTES == 0
                && identity(&file)? == expected)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn absent(path: &Path) -> Result<bool> {
    match path.symlink_metadata() {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn rename_file(
    source: &Path,
    target_directory: &File,
    target_name: &str,
    expected: &Stamp,
    apply: bool,
) -> Result<()> {
    leaf(target_name)?;
    let mut file = OpenOptions::new()
        .read(true)
        .access_mode(0x8000_0000 | 0x0001_0000)
        .share_mode(1)
        .custom_flags(0x0020_0000)
        .open(source)?;
    let actual = stamp(&mut file, apply && expected.digest.is_some())?;
    ensure!(
        actual.id == expected.id,
        "The object was replaced; it was preserved"
    );
    if apply {
        ensure!(
            actual == *expected,
            "The object changed after preview; refresh the plan"
        );
    }
    // A directory handle pins the exact destination against junction replacement.
    // A relative leaf and ReplaceIfExists=false prevent path traversal/overwrite.
    #[repr(C)]
    struct RenameInfo {
        replace: u32,
        directory: *mut std::ffi::c_void,
        length: u32,
        name: [u16; 1],
    }
    let name: Vec<u16> = target_name.encode_utf16().collect();
    let offset = std::mem::offset_of!(RenameInfo, name);
    let bytes = offset
        .checked_add(name.len() * 2)
        .context("Rename path is too long")?
        .max(std::mem::size_of::<RenameInfo>());
    let mut buffer = vec![0usize; bytes.div_ceil(std::mem::size_of::<usize>())];
    let info = buffer.as_mut_ptr().cast::<RenameInfo>();
    unsafe {
        (*info).replace = 0;
        (*info).directory = target_directory.as_raw_handle();
        (*info).length = u32::try_from(name.len() * 2)?;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            info.cast::<u8>().add(offset).cast(),
            name.len(),
        );
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileRenameInfo,
            info.cast(),
            u32::try_from(bytes)?,
        )?;
    }
    ensure!(
        identity(&file)? == expected.id,
        "Filesystem identity changed during rename"
    );
    Ok(())
}

pub(super) fn operation_id() -> Result<String> {
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    Ok(format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_nanos(),
        SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

pub(super) fn remove_empty_directory(path: &Path, expected: &str) -> Result<()> {
    use windows::Win32::Storage::FileSystem::{FILE_DISPOSITION_INFO, FileDispositionInfo};
    let file = OpenOptions::new()
        .access_mode(0x80 | 0x0001_0000)
        .share_mode(3)
        .custom_flags(0x0200_0000 | 0x0020_0000)
        .open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_dir()
            && metadata.file_attributes() & 0x400 == 0
            && identity(&file)? == expected,
        "The folder was replaced; it was preserved"
    );
    // Windows rejects disposition for a nonempty directory. Never recurse.
    let disposition = FILE_DISPOSITION_INFO {
        DeleteFile: true.into(),
    };
    unsafe {
        SetFileInformationByHandle(
            HANDLE(file.as_raw_handle()),
            FileDispositionInfo,
            &disposition as *const _ as *const _,
            std::mem::size_of_val(&disposition) as u32,
        )?;
    }
    Ok(())
}
