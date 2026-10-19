use crate::{engine, model::*, native, store::Store};
use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::windows::fs::MetadataExt,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    Win32::Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    core::PCWSTR,
};

pub fn installed_exe(home: &Path) -> PathBuf {
    home.join("desktop-pilot.exe")
}

fn test_profile(store: &Store) -> Result<bool> {
    Ok(!store.get::<String>("test_desktop")?.is_empty()
        || store.home.file_name().is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("test-")
        }))
}

struct Stage(PathBuf);
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn copy_fresh(source: &Path, destination: &Path) -> Result<()> {
    let mut from = File::open(source)?;
    let mut to = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    let result = (|| -> Result<()> {
        std::io::copy(&mut from, &mut to)?;
        to.flush()?;
        to.sync_all()?;
        Ok(())
    })();
    drop(to);
    if result.is_err() {
        let _ = std::fs::remove_file(destination);
    }
    result
}

fn move_file(source: &Path, destination: &Path, replace: bool) -> Result<()> {
    let source = native::wide(&source.to_string_lossy());
    let destination = native::wide(&destination.to_string_lossy());
    let flags = if replace {
        MOVEFILE_WRITE_THROUGH | MOVEFILE_REPLACE_EXISTING
    } else {
        MOVEFILE_WRITE_THROUGH
    };
    unsafe {
        MoveFileExW(PCWSTR(source.as_ptr()), PCWSTR(destination.as_ptr()), flags)?;
    }
    Ok(())
}

fn ordinary_file(path: &Path) -> Result<bool> {
    match path.symlink_metadata() {
        Ok(metadata) => {
            ensure!(
                metadata.is_file() && metadata.file_attributes() & 0x400 == 0,
                "Refusing to replace a directory or filesystem link: {}",
                path.display()
            );
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn stop_background(store: &Store) -> Result<()> {
    if !native::is_running("background", &store.home)? {
        return Ok(());
    }
    store.set("shutdown", &true)?;
    crate::tray::request_close(&store.home)?;
    let until = Instant::now() + Duration::from_secs(12);
    while native::is_running("background", &store.home)? {
        ensure!(
            Instant::now() < until,
            "Background work is still finishing. Wait for it to stop, then open the new version again."
        );
        std::thread::sleep(Duration::from_millis(100));
        crate::tray::request_close(&store.home)?;
    }
    Ok(())
}

fn install_current(store: &Store) -> Result<PathBuf> {
    install_from(store, &std::env::current_exe()?)
}

fn install_from(store: &Store, source: &Path) -> Result<PathBuf> {
    let _installation = native::exclusive("installation", &store.home)?;
    let current = crate::profile::ordinary_path(std::fs::canonicalize(source)?);
    let installed = installed_exe(&store.home);
    let exists = ordinary_file(&installed)?;
    if norm(&current.to_string_lossy()) == norm(&installed.to_string_lossy()) {
        ensure!(exists, "The installed executable is missing");
        return Ok(installed);
    }
    let expected = hash(&std::fs::read(&current)?);
    let previous = if exists {
        Some(hash(&std::fs::read(&installed)?))
    } else {
        None
    };
    if previous.as_deref() == Some(&expected) {
        return Ok(installed);
    }
    ensure!(
        !exists || engine::setup_complete(store)?,
        "A different desktop-pilot.exe already exists in the selected folder. Choose an empty profile folder."
    );
    let stage_path = store.home.join(format!(
        ".desktop-pilot-update-{}-{}.exe",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    copy_fresh(&current, &stage_path)?;
    let stage = Stage(stage_path);
    ensure!(
        hash(&std::fs::read(&stage.0)?) == expected,
        "The staged update failed verification"
    );
    stop_background(store)?;
    let _stopped = native::exclusive("background", &store.home)?;
    if let Some(previous) = previous {
        ensure!(
            ordinary_file(&installed)? && hash(&std::fs::read(&installed)?) == previous,
            "The installed executable changed during the update"
        );
        let backup = store
            .home
            .join(format!("desktop-pilot.previous-{}.exe", &previous[..16]));
        if ordinary_file(&backup)? {
            ensure!(
                hash(&std::fs::read(&backup)?) == previous,
                "A different file already occupies the executable backup path"
            );
        } else {
            copy_fresh(&installed, &backup)?;
            ensure!(
                hash(&std::fs::read(&backup)?) == previous,
                "The executable backup failed verification"
            );
        }
        // Same-volume atomic replacement, after the previous executable has
        // been copied and flushed. No database, history or desktop files move.
        move_file(&stage.0, &installed, true).context(
            "Cannot replace the installed executable. Close the previous version and try again.",
        )?;
        let _ = store.event(
            "update",
            &format!(
                "Installed Desktop Pilot {}; previous executable: {}",
                env!("CARGO_PKG_VERSION"),
                backup.display()
            ),
        );
    } else {
        move_file(&stage.0, &installed, false)?;
    }
    ensure!(
        hash(&std::fs::read(&installed)?) == expected,
        "The installed executable failed verification"
    );
    store.set("installed_version", &env!("CARGO_PKG_VERSION"))?;
    Ok(installed)
}

pub fn start_background(home: &Path) -> Result<()> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    ensure!(
        engine::setup_complete(&store)?,
        "Complete initial setup before starting background tracking"
    );
    let exe = install_current(&store)?;
    if native::is_running("background", home)? {
        return Ok(());
    }
    store.set("shutdown", &false)?;
    store.set("heartbeat", &0i64)?;
    let mut child = std::process::Command::new(exe)
        .arg("--background")
        .arg("--home")
        .arg(home)
        .creation_flags(0x08000000)
        .spawn()
        .context("Cannot start Desktop Pilot background work")?;
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait()? {
            if native::is_running("background", home)? {
                return Ok(());
            }
            let detail = std::fs::read_to_string(home.join("last-error.txt")).unwrap_or_default();
            bail!("Background work could not start ({status}). {detail}");
        }
        if native::is_running("background", home)? && store.get::<i64>("heartbeat")? > 0 {
            return Ok(());
        }
        ensure!(
            Instant::now() < until,
            "Background startup has not completed yet. Check the status in Settings."
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn save_config(store: &Store, config: &Config) -> Result<()> {
    config.validate()?;
    let previous: Config = store.get("config")?;
    let actual = !test_profile(store)?;
    if actual {
        native::autostart(&installed_exe(&store.home), &store.home, config.autostart)?;
    }
    if let Err(error) = store.set("config", config) {
        if actual
            && let Err(rollback) =
                native::autostart(&installed_exe(&store.home), &store.home, previous.autostart)
        {
            return Err(error.context(format!("Startup rollback also failed: {rollback:#}")));
        }
        return Err(error);
    }
    Ok(())
}

fn make_settings_shortcut(store: &Store, installed: &Path) -> Result<()> {
    if test_profile(store)? {
        return Ok(());
    }
    let _com = native::Com::new()?;
    let directory = native::programs()?;
    std::fs::create_dir_all(&directory)?;
    let arguments = format!(
        "--settings --home {}",
        native::quote_arg(&store.home.to_string_lossy())
    );
    let profile = &hash(norm(&store.home.to_string_lossy()).as_bytes())[..8];
    for index in 0..100 {
        let name = match index {
            0 => "Desktop Pilot.lnk".into(),
            1 => format!("Desktop Pilot ({profile}).lnk"),
            _ => format!("Desktop Pilot ({profile}-{index}).lnk"),
        };
        let shortcut = directory.join(name);
        if shortcut.try_exists()? {
            if native::read_link(&shortcut).is_ok_and(|link| {
                norm(&link.target) == norm(&installed.to_string_lossy())
                    && link.arguments == arguments
            }) {
                return Ok(());
            }
            continue;
        }
        let stage_path = directory.join(format!(
            ".DesktopPilot-{profile}-{}-{index}.lnk",
            std::process::id()
        ));
        let reservation = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&stage_path)?;
        drop(reservation);
        let stage = Stage(stage_path);
        native::make_link(
            &stage.0,
            installed,
            &arguments,
            &installed.to_string_lossy(),
            0,
            "Desktop Pilot settings",
        )?;
        move_file(&stage.0, &shortcut, false)?;
        store.set("settings_shortcut", &shortcut.to_string_lossy())?;
        return Ok(());
    }
    bail!("Cannot create a unique Desktop Pilot shortcut in the Start menu")
}

pub fn finish(store: &Store, config: &Config) -> Result<()> {
    config.validate()?;
    if !test_profile(store)? && config.automatic {
        let layout = native::layout()?;
        ensure!(
            layout.flags & 1 == 0,
            "Turn off Explorer auto-arrange, or choose tracking without automatic changes."
        );
        engine::reserve_positions(&layout, config, &State::default(), &[], config.slots)?;
    }
    let installed = install_current(store)?;
    make_settings_shortcut(store, &installed)?;
    let previous: Config = store.get("config")?;
    let prepared = store.transaction(|| {
        save_config(store, config)?;
        if store
            .get::<crate::learning::Progress>("learning")?
            .started_at
            == 0
        {
            store.set(
                "learning",
                &crate::learning::Progress {
                    started_at: now(),
                    ..Default::default()
                },
            )?;
        }
        engine::refresh_apps(store)?;
        store.set("setup_complete", &true)?;
        store.event("setup", "Initial setup completed")?;
        Ok(())
    });
    if let Err(error) = prepared {
        if !test_profile(store)? {
            let _ = native::autostart(&installed, &store.home, previous.autostart);
        }
        return Err(error);
    }
    start_background(&store.home)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staged_copies_never_overwrite_existing_files() {
        let root =
            std::env::temp_dir().join(format!("test-stage-{}-{}", std::process::id(), now()));
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("source");
        let target = root.join("target");
        std::fs::write(&source, b"update").unwrap();
        std::fs::write(&target, b"user file").unwrap();
        assert!(copy_fresh(&source, &target).is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"user file");
        let fresh = root.join("new");
        copy_fresh(&source, &fresh).unwrap();
        assert_eq!(std::fs::read(&fresh).unwrap(), b"update");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn verified_update_keeps_history_and_original_files() {
        let root =
            std::env::temp_dir().join(format!("test-install-{}-{}", std::process::id(), now()));
        let store = Store::open(&root).unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("custom-sentinel", &"history to keep").unwrap();
        let installed = installed_exe(&store.home);
        let source = root.join("downloaded.exe");
        let original = root.join("original-user-file.txt");
        std::fs::write(&installed, b"old executable").unwrap();
        std::fs::write(&source, b"new executable").unwrap();
        std::fs::write(&original, b"keep unchanged").unwrap();
        install_from(&store, &source).unwrap();
        assert_eq!(std::fs::read(&installed).unwrap(), b"new executable");
        let backup = root.join(format!(
            "desktop-pilot.previous-{}.exe",
            &hash(b"old executable")[..16]
        ));
        assert_eq!(std::fs::read(backup).unwrap(), b"old executable");
        assert_eq!(std::fs::read(original).unwrap(), b"keep unchanged");
        assert_eq!(
            store.get::<String>("custom-sentinel").unwrap(),
            "history to keep"
        );
        install_from(&store, &source).unwrap();
        assert!(!std::fs::read_dir(&root).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".desktop-pilot-update-")
        }));
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fresh_setup_does_not_replace_an_unowned_executable() {
        let root = std::env::temp_dir().join(format!(
            "test-unowned-install-{}-{}",
            std::process::id(),
            now()
        ));
        let store = Store::open(&root).unwrap();
        let installed = installed_exe(&store.home);
        let source = root.join("new.exe");
        std::fs::write(&installed, b"unrelated user executable").unwrap();
        std::fs::write(&source, b"pilot executable").unwrap();
        assert!(install_from(&store, &source).is_err());
        assert_eq!(
            std::fs::read(installed).unwrap(),
            b"unrelated user executable"
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
}
