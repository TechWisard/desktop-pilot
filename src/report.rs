use anyhow::{Context, Result, ensure};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
use windows::{
    Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    core::PCWSTR,
};

fn validate_name(path: &Path) -> Result<()> {
    let name = path
        .file_name()
        .context("Choose a file for the JSON report")?
        .to_string_lossy()
        .to_ascii_lowercase();
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase());
    let stem = name.split('.').next().unwrap_or_default().trim_end();
    let device_number = stem
        .strip_prefix("com")
        .or_else(|| stem.strip_prefix("lpt"));
    let reserved = matches!(stem, "con" | "prn" | "aux" | "nul" | "conin$" | "conout$")
        || device_number.is_some_and(|number| {
            matches!(
                number,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        });
    ensure!(
        !reserved
            && !name.chars().any(|character| {
                character.is_ascii_control()
                    || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            })
            && name.trim_end_matches(['.', ' ']) == name
            && !name.starts_with("usage.sqlite")
            && !matches!(
                extension.as_deref(),
                Some(
                    "exe"
                        | "dll"
                        | "com"
                        | "scr"
                        | "msi"
                        | "msp"
                        | "cpl"
                        | "bat"
                        | "cmd"
                        | "ps1"
                        | "psm1"
                        | "vbs"
                        | "vbe"
                        | "wsf"
                        | "wsh"
                        | "js"
                        | "jse"
                        | "hta"
                        | "reg"
                        | "lnk"
                        | "url"
                        | "sqlite"
                        | "sqlite3"
                        | "db"
                )
            ),
        "Choose a separate JSON report file; application files and databases cannot be overwritten"
    );
    Ok(())
}

/// Reports must never replace a database, executable or shortcut by mistake.
pub fn validate(path: &Path) -> Result<()> {
    validate_name(path)?;
    match path.symlink_metadata() {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
                "The report destination must be an ordinary file, not a folder or filesystem link"
            );
            ensure!(
                !metadata.permissions().readonly(),
                "The report file is read-only; choose a writable destination"
            );
            // A legacy 8.3 alias can conceal the database's real extension.
            validate_name(&std::fs::canonicalize(path)?)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn reserve(path: &Path) -> Result<(Temporary, File)> {
    let destination = std::path::absolute(path)?;
    let directory = destination.parent().context("Invalid report directory")?;
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    Ok((0..100)
        .find_map(|_| {
            let path = directory.join(format!(
                ".desktop-pilot-report-{}-{}.tmp",
                std::process::id(),
                SERIAL.fetch_add(1, Ordering::Relaxed)
            ));
            match OpenOptions::new().create_new(true).write(true).open(&path) {
                Ok(file) => Some(Ok((Temporary(path), file))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(error)),
            }
        })
        .context("Cannot reserve a temporary report file")??)
}

/// Catch ordinary output errors before an apply/undo operation can take place.
pub fn preflight(path: &Path) -> Result<()> {
    validate(path)?;
    if path.try_exists()? {
        OpenOptions::new().write(true).open(path)?;
    }
    let (temporary, file) = reserve(path)?;
    drop(file);
    drop(temporary);
    Ok(())
}

/// Publish a fully written report in one rename, preserving an older report if
/// serialization, disk space, flushing or replacement fails.
pub fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    validate(path)?;
    let destination = std::path::absolute(path)?;
    let (temporary, mut file) = reserve(&destination)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    validate(&destination)?;
    let from = crate::native::wide(&temporary.0.to_string_lossy());
    let to = crate::native::wide(&destination.to_string_lossy());
    unsafe {
        MoveFileExW(
            PCWSTR(from.as_ptr()),
            PCWSTR(to.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_cannot_replace_profile_files_or_shortcuts() {
        for name in [
            "usage.sqlite",
            "USAGE.SQLITE-WAL",
            "usage.sqlite.",
            "usage.sqlite ",
            "usage.sqlite-shm",
            "desktop-pilot.exe",
            "desktop-pilot.exe. ",
            "Original.LNK",
            "Original.url",
            "automation.bat",
            "automation.cmd",
            "automation.COM",
            "helper.scr",
            "installer.msi",
            "installer.msp",
            "helper.cpl",
            "setup.ps1",
            "module.psm1",
            "automation.vbs",
            "automation.vbe",
            "task.wsf",
            "task.wsh",
            "helper.js",
            "helper.jse",
            "htmlapp.hta",
            "registry.reg",
            "history.db",
            "document.txt:report.json",
            "CON.json",
            "nul",
            "COM1.txt",
            "LPT².json",
            "bad?.json",
            "bad*.json",
        ] {
            assert!(validate(Path::new(name)).is_err(), "{name}");
        }
        assert!(validate(Path::new("diagnostic-report.json")).is_ok());
    }

    #[test]
    fn existing_script_files_survive_report_preflight_and_write() {
        // --out publishes JSON, but a mistyped script path must not replace
        // a user's automation, installer or registry file with that JSON.
        let directory = std::env::temp_dir().join(format!(
            "test-report-script-safety-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let original = b"keep existing user script\r\n";
        for name in [
            "automation.cmd",
            "automation.bat",
            "automation.ps1",
            "automation.vbs",
            "automation.js",
            "automation.hta",
            "installer.msi",
            "registry.reg",
        ] {
            let path = directory.join(name);
            std::fs::write(&path, original).unwrap();
            assert!(preflight(&path).is_err(), "{name}");
            assert!(write(&path, br#"{"report":true}"#).is_err(), "{name}");
            assert_eq!(std::fs::read(&path).unwrap(), original, "{name}");
        }
        let allowed = directory.join("audit.json");
        preflight(&allowed).unwrap();
        write(&allowed, br#"{"report":true}"#).unwrap();
        assert_eq!(std::fs::read(&allowed).unwrap(), br#"{"report":true}"#);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 9);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn short_name_alias_cannot_overwrite_the_profile_database() {
        use windows::Win32::Storage::FileSystem::GetShortPathNameW;

        let directory =
            std::env::temp_dir().join(format!("test-report-short-name-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let database = directory.join("usage.sqlite");
        let original = b"Original database bytes";
        std::fs::write(&database, original).unwrap();
        let source = crate::native::wide(&database.to_string_lossy());
        let mut buffer = vec![0u16; 32768];
        let length = unsafe { GetShortPathNameW(PCWSTR(source.as_ptr()), Some(&mut buffer)) };
        assert!(length > 0 && (length as usize) < buffer.len());
        // NTFS may expose a name such as USAGE~1.SQL. When short names are
        // disabled, Windows returns the long name, which must also be rejected.
        let alias = PathBuf::from(String::from_utf16(&buffer[..length as usize]).unwrap());
        assert!(validate(&alias).is_err());
        assert!(write(&alias, br#"{"report":true}"#).is_err());
        assert_eq!(std::fs::read(&database).unwrap(), original);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_publication_preserves_the_previous_report_and_removes_staging() {
        let directory =
            std::env::temp_dir().join(format!("test-report-publish-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("report.json");
        std::fs::write(&path, b"previous report").unwrap();
        let original_permissions = std::fs::metadata(&path).unwrap().permissions();
        let mut permissions = original_permissions.clone();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();
        assert!(preflight(&path).is_err());
        std::fs::set_permissions(&path, original_permissions).unwrap();
        let blocked = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
                .open(&path)
                .unwrap()
        };
        assert!(write(&path, br#"{"new":true}"#).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"previous report");
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        drop(blocked);
        write(&path, br#"{"new":true}"#).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"new":true}"#);
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn missing_report_directory_is_rejected_before_the_operation() {
        let directory =
            std::env::temp_dir().join(format!("test-report-absent-{}", std::process::id()));
        assert!(!directory.exists());
        assert!(preflight(&directory.join("report.json")).is_err());
        assert!(!directory.exists());
    }
}
