use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

fn select(explicit: Option<&Path>, local: &Path, executable: &Path) -> PathBuf {
    let default = local.join("DesktopPilot");
    let installed = executable.parent().filter(|parent| {
        parent
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("DesktopPilot"))
            && parent.join("usage.sqlite").is_file()
    });
    let default_requested = explicit.is_none_or(|path| {
        crate::model::norm(&path.to_string_lossy())
            == crate::model::norm(&default.to_string_lossy())
    });
    if default_requested && let Some(installed) = installed {
        return installed.to_path_buf();
    }
    explicit.unwrap_or(&default).to_path_buf()
}

pub fn home(args: &[String]) -> Result<PathBuf> {
    let explicit = args
        .iter()
        .position(|s| s == "--home")
        .map(|i| {
            std::path::absolute(args.get(i + 1).context("Missing --home path")?)
                .map_err(anyhow::Error::from)
        })
        .transpose()?;
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is missing")?);
    let selected = select(explicit.as_deref(), &local, &std::env::current_exe()?);
    if selected.is_dir() {
        return Ok(ordinary_path(std::fs::canonicalize(selected)?));
    }
    Ok(selected)
}

fn ordinary_path(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = value.strip_prefix(r"\\?\") {
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
            installed
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
}
