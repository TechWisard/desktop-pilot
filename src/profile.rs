use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

fn adjacent_profile(executable: &Path) -> Option<PathBuf> {
    let parent = executable.parent()?;
    let database = parent.join("usage.sqlite");
    if !database.is_file() {
        return None;
    }
    // Preserve the original default installation even if its database needs
    // repair; custom folders require positive, read-only schema identification.
    if parent
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("DesktopPilot"))
    {
        return Some(parent.to_path_buf());
    }
    let database = rusqlite::Connection::open_with_flags(
        database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    let expected_tables: i64 = database.query_row(
        "SELECT COUNT(*) FROM sqlite_schema WHERE type='table' AND name IN ('kv','apps','days','events','commands','transactions')",
        [], |row| row.get(0),
    ).ok()?;
    (expected_tables == 6).then(|| parent.to_path_buf())
}

fn select(explicit: Option<&Path>, local: &Path, executable: &Path) -> PathBuf {
    // An explicit profile must never silently select another database, including
    // when the requested path happens to equal LOCALAPPDATA\DesktopPilot.
    if let Some(explicit) = explicit {
        return explicit.to_path_buf();
    }
    if let Some(installed) = adjacent_profile(executable) {
        return installed;
    }
    local.join("DesktopPilot")
}

fn explicit_home(args: &[String]) -> Result<Option<&Path>> {
    let mut matches = args.iter().enumerate().filter(|(_, arg)| *arg == "--home");
    let Some((index, _)) = matches.next() else {
        return Ok(None);
    };
    ensure!(
        matches.next().is_none(),
        "--home may only be specified once"
    );
    let value = args.get(index + 1).context("Missing --home path")?;
    ensure!(
        !value.trim().is_empty() && !value.starts_with("--") && !value.contains('\0'),
        "Missing or invalid --home path"
    );
    Ok(Some(Path::new(value)))
}

pub fn home(args: &[String]) -> Result<PathBuf> {
    let selected = if let Some(explicit) = explicit_home(args)? {
        explicit.to_path_buf()
    } else {
        let executable = std::env::current_exe()?;
        if let Some(installed) = adjacent_profile(&executable) {
            installed
        } else {
            let local = PathBuf::from(
                std::env::var_os("LOCALAPPDATA")
                    .context("LOCALAPPDATA is missing; use --home to choose a profile")?,
            );
            select(None, &local, &executable)
        }
    };
    let selected = std::path::absolute(selected)?;
    // Canonicalize the nearest existing ancestor too: the first launch of a
    // profile through a junction and subsequent launches must share one lock.
    let mut existing = selected.as_path();
    let mut suffix = Vec::new();
    while !existing.try_exists()? {
        suffix.push(
            existing
                .file_name()
                .context("Invalid profile path")?
                .to_owned(),
        );
        existing = existing.parent().context("Invalid profile parent")?;
    }
    ensure!(existing.is_dir(), "The profile path is not a directory");
    let mut canonical = ordinary_path(std::fs::canonicalize(existing)?);
    canonical.extend(suffix.into_iter().rev());
    Ok(canonical)
}

pub(crate) fn ordinary_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = value.strip_prefix(r"\\?\")
        && local.as_bytes().get(1) == Some(&b':')
    {
        PathBuf::from(local)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_drive_and_unc_paths_remain_absolute_win32_paths() {
        assert_eq!(
            ordinary_path(PathBuf::from(r"\\?\X:\Example\DesktopPilot")),
            PathBuf::from(r"X:\Example\DesktopPilot")
        );
        assert_eq!(
            ordinary_path(PathBuf::from(r"\\?\UNC\server\share\DesktopPilot")),
            PathBuf::from(r"\\server\share\DesktopPilot")
        );
    }

    #[test]
    fn installed_profile_survives_different_launch_environment() {
        let root = std::env::temp_dir().join(format!("test-profile-{}", std::process::id()));
        let installed = root.join("redirected").join("DesktopPilot");
        std::fs::create_dir_all(&installed).unwrap();
        std::fs::write(installed.join("usage.sqlite"), b"fixture").unwrap();
        let local = root.join("ordinary-local");
        let executable = installed.join("desktop-pilot.exe");
        assert_eq!(select(None, &local, &executable), installed);
        assert_eq!(
            select(Some(&local.join("DesktopPilot")), &local, &executable),
            local.join("DesktopPilot")
        );
        let isolated = root.join("test-explicit");
        assert_eq!(select(Some(&isolated), &local, &executable), isolated);
        std::fs::remove_file(installed.join("usage.sqlite")).unwrap();
        assert_eq!(
            select(None, &local, &executable),
            local.join("DesktopPilot")
        );
        std::fs::remove_dir(&installed).unwrap();
        std::fs::remove_dir(root.join("redirected")).unwrap();
        std::fs::remove_dir(&root).unwrap();
    }

    #[test]
    fn explicit_profile_requires_one_real_value() {
        for args in [
            vec!["--home"],
            vec!["--home", ""],
            vec!["--home", "--settings"],
            vec!["--home", "a", "--home", "b"],
        ] {
            let args: Vec<_> = args.into_iter().map(String::from).collect();
            assert!(explicit_home(&args).is_err());
        }
        let args = vec![
            "--settings".into(),
            "--home".into(),
            r"X:\My profile\".into(),
        ];
        assert_eq!(
            explicit_home(&args).unwrap(),
            Some(Path::new(r"X:\My profile\"))
        );
        assert_eq!(explicit_home(&[]).unwrap(), None);
    }

    #[test]
    fn volume_device_paths_are_not_rewritten_as_relative_paths() {
        let path = PathBuf::from(r"\\?\Volume{1234}\DesktopPilot");
        assert_eq!(ordinary_path(path.clone()), path);
    }

    #[test]
    fn custom_adjacent_profiles_require_the_application_schema() {
        let root =
            std::env::temp_dir().join(format!("test-adjacent-profile-{}", std::process::id()));
        let unrelated = root.join("unrelated");
        std::fs::create_dir_all(&unrelated).unwrap();
        let database_path = unrelated.join("usage.sqlite");
        let database = rusqlite::Connection::open(&database_path).unwrap();
        database.execute_batch("CREATE TABLE unrelated_data(value TEXT); INSERT INTO unrelated_data VALUES('keep');").unwrap();
        drop(database);
        let local = root.join("local");
        assert_eq!(
            select(None, &local, &unrelated.join("desktop-pilot.exe")),
            local.join("DesktopPilot")
        );
        let database = rusqlite::Connection::open_with_flags(
            &database_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let table_count: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 1);
        drop(database);
        let custom = root.join("my-portable-profile");
        let store = crate::store::Store::open(&custom).unwrap();
        drop(store);
        assert_eq!(
            select(None, &local, &custom.join("desktop-pilot.exe")),
            custom
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
