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

fn stop_for_change(store: &Store) -> Result<native::Lock> {
    if let Some(stopped) = native::try_exclusive("background", &store.home)? {
        return Ok(stopped);
    }
    store.set("shutdown", &true)?;
    crate::tray::request_close(&store.home)?;
    let until = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(stopped) = native::try_exclusive("background", &store.home)? {
            return Ok(stopped);
        }
        ensure!(
            Instant::now() < until,
            "Background work is still finishing. Wait for it to stop, then open the new version again."
        );
        std::thread::sleep(Duration::from_millis(100));
        crate::tray::request_close(&store.home)?;
    }
}

pub fn stop_background(home: &Path) -> Result<()> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    let _stopped = stop_for_change(&store)?;
    store.set("shutdown", &true)
}

fn install_current(store: &Store) -> Result<PathBuf> {
    install_from(store, &std::env::current_exe()?)
}

fn check_upgrade_version(source: Option<[u16; 4]>, installed: Option<[u16; 4]>) -> Result<()> {
    if let Some(installed) = installed {
        let source =
            source.context("The downloaded executable has no valid version information")?;
        ensure!(
            source >= installed,
            "A newer Desktop Pilot version is already installed. Open the installed version from the Start menu."
        );
    }
    Ok(())
}

fn install_from(store: &Store, source: &Path) -> Result<PathBuf> {
    let _installation = native::exclusive("installation", &store.home)?;
    let current = crate::profile::ordinary_path(std::fs::canonicalize(source)?);
    let installed = installed_exe(&store.home);
    let exists = ordinary_file(&installed)?;
    if norm(&current.to_string_lossy()) == norm(&installed.to_string_lossy()) {
        ensure!(exists, "The installed executable is missing");
        store.set("installed_version", &env!("CARGO_PKG_VERSION"))?;
        return Ok(installed);
    }
    let expected = hash(&std::fs::read(&current)?);
    let previous = if exists {
        Some(hash(&std::fs::read(&installed)?))
    } else {
        None
    };
    if previous.as_deref() == Some(&expected) {
        store.set("installed_version", &env!("CARGO_PKG_VERSION"))?;
        return Ok(installed);
    }
    ensure!(
        !exists || engine::setup_complete(store)?,
        "A different desktop-pilot.exe already exists in the selected folder. Choose an empty profile folder."
    );
    if exists {
        check_upgrade_version(
            native::executable_version(&current)?,
            native::executable_version(&installed)?,
        )?;
    }
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
    // Check and flush the backup before stopping a healthy worker. A collision,
    // full disk or unreadable backup must not unnecessarily interrupt tracking.
    let backup = if let Some(previous) = previous.as_ref() {
        ensure!(
            ordinary_file(&installed)? && hash(&std::fs::read(&installed)?) == *previous,
            "The installed executable changed during the update"
        );
        let backup = store
            .home
            .join(format!("desktop-pilot.previous-{}.exe", &previous[..16]));
        if ordinary_file(&backup)? {
            ensure!(
                hash(&std::fs::read(&backup)?) == *previous,
                "A different file already occupies the executable backup path"
            );
        } else {
            copy_fresh(&installed, &backup)?;
            ensure!(
                hash(&std::fs::read(&backup)?) == *previous,
                "The executable backup failed verification"
            );
        }
        Some(backup)
    } else {
        None
    };
    let was_running = native::is_running("background", &store.home)?;
    let result = (|| -> Result<()> {
        let _stopped = stop_for_change(store)?;
        if let Some(previous) = previous.as_ref() {
            ensure!(
                ordinary_file(&installed)? && hash(&std::fs::read(&installed)?) == *previous,
                "The installed executable changed during the update"
            );
        }
        // Same-volume atomic replacement after the backup has been flushed.
        // No database, history or desktop files move.
        move_file(&stage.0, &installed, exists).context(
            "Cannot replace the installed executable. Close the previous version and try again.",
        )?;
        ensure!(
            hash(&std::fs::read(&installed)?) == expected,
            "The installed executable failed verification"
        );
        store.set("installed_version", &env!("CARGO_PKG_VERSION"))?;
        if let Some(backup) = backup.as_ref() {
            let _ = store.event(
                "update",
                &format!(
                    "Installed Desktop Pilot {}; previous executable: {}",
                    env!("CARGO_PKG_VERSION"),
                    backup.display()
                ),
            );
        }
        Ok(())
    })();
    if let Err(error) = result {
        if was_running {
            let recovery = (|| -> Result<()> {
                let actual = hash(&std::fs::read(&installed)?);
                ensure!(
                    actual == expected || previous.as_ref() == Some(&actual),
                    "The executable changed; automatic restart was skipped"
                );
                start_installed(store, &installed)
            })();
            if let Err(recovery) = recovery {
                return Err(error.context(format!(
                    "Background work could not be restored after the failed update: {recovery:#}"
                )));
            }
        }
        return Err(error);
    }
    Ok(installed)
}

pub fn start_background(home: &Path) -> Result<()> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    ensure!(
        engine::setup_complete(&store)?,
        "Complete initial setup before starting background tracking"
    );
    start_background_locked(&store)
}

/// A Settings command is not permission to resume explicitly disabled tracking.
/// Serialize the decision with Turn off / explicit Start under the startup lock.

#[cfg(test)]
pub(crate) struct QueueTestPause {
    pub home: PathBuf,
    pub arrived: std::sync::mpsc::Sender<()>,
    pub resume: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
pub(crate) static QUEUE_TEST_PAUSE: std::sync::Mutex<Option<QueueTestPause>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
pub(crate) fn pause_before_enqueue(home: &Path) {
    let gate = QUEUE_TEST_PAUSE.lock().unwrap();
    if let Some(gate) = gate.as_ref().filter(|gate| gate.home == home) {
        gate.arrived.send(()).expect("queue must reach the race checkpoint");
        gate.resume
            .recv_timeout(Duration::from_secs(20))
            .expect("queue checkpoint must be released");
    }
}

pub fn start_background_if_enabled(home: &Path) -> Result<bool> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    ensure!(
        engine::setup_complete(&store)?,
        "Complete initial setup before starting background tracking"
    );
    if store.get::<bool>("background_disabled")? {
        return Ok(false);
    }
    start_background_locked(&store)?;
    Ok(true)
}

fn start_background_locked(store: &Store) -> Result<()> {
    let exe = install_current(store)?;
    start_installed(store, &exe)?;
    store.set("background_disabled", &false)
}

fn start_installed(store: &Store, exe: &Path) -> Result<()> {
    let home = &store.home;
    if native::is_running("background", home)? {
        if store.get::<bool>("shutdown")? {
            // Exit from the tray does not hold the startup lock. Finish that
            // shutdown before launching a replacement instead of reporting
            // success for a worker that is already on its way out.
            drop(stop_for_change(store)?);
        } else {
            return wait_for_background(store, None);
        }
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
    wait_for_background(store, Some(&mut child))
}

fn wait_for_background(store: &Store, mut child: Option<&mut std::process::Child>) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        let running = native::is_running("background", &store.home)?;
        if running && !store.get::<bool>("shutdown")? && store.get::<i64>("heartbeat")? > 0 {
            return Ok(());
        }
        let exited = child
            .as_mut()
            .map(|child| child.try_wait())
            .transpose()?
            .flatten();
        if !running && (child.is_none() || exited.is_some()) {
            let detail =
                std::fs::read_to_string(store.home.join("last-error.txt")).unwrap_or_default();
            if let Some(status) = exited {
                bail!("Background work could not start ({status}). {detail}");
            }
            bail!("Background work stopped before becoming ready. {detail}");
        }
        ensure!(
            Instant::now() < until,
            "Background startup has not completed yet. Check the status in Settings."
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn prepare_settings(home: &Path) -> Result<()> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    ensure!(
        engine::setup_complete(&store)?,
        "Complete initial setup before starting background tracking"
    );
    let installed = install_current(&store)?;
    record_settings_shortcut_result(&store, make_settings_shortcut(&store, &installed))?;
    if !store.get::<bool>("background_disabled")? {
        start_installed(&store, &installed)?;
    }
    Ok(())
}

fn restore_autostart(store: &Store, error: anyhow::Error) -> anyhow::Error {
    // A failed outer transaction can roll back after save_config succeeded.
    // Synchronize the registry with the committed config while holding SQLite's
    // writer reservation, so another settings save cannot be overwritten.
    let rollback = store.transaction(|| {
        if !test_profile(store)? {
            let persisted: Config = store.get("config")?;
            native::autostart(
                &installed_exe(&store.home),
                &store.home,
                persisted.autostart,
            )?;
        }
        Ok(())
    });
    if let Err(rollback) = rollback {
        error.context(format!("Startup rollback also failed: {rollback:#}"))
    } else {
        error
    }
}

pub fn disable_background(home: &Path) -> Result<Config> {
    let _starting = native::exclusive("startup", home)?;
    let store = Store::open(home)?;
    let _stopped = stop_for_change(&store)?;
    let _changes = native::exclusive("changes", home)?;
    store
        .transaction(|| {
            let mut config: Config = store.get("config")?;
            config.automatic = false;
            config.collect = false;
            config.autostart = false;
            config.learning_enabled = false;
            save_config(&store, &config)?;
            let message = "Command cancelled because background work was disabled.";
            let cancelled = store.db.execute(
                "UPDATE commands SET status='error',message=?1 WHERE status IN ('pending','running')",
                [message],
            )?;
            if cancelled > 0 {
                store.event("settings", message)?;
            }
            store.set("background_disabled", &true)?;
            store.set("shutdown", &true)?;
            store.event("settings", "Background work and Windows startup disabled")?;
            Ok(config)
        })
        .map_err(|error| restore_autostart(&store, error))
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
    make_settings_shortcut_in(store, installed, &native::programs()?)
}

fn make_settings_shortcut_in(store: &Store, installed: &Path, directory: &Path) -> Result<()> {
    let _com = native::Com::new()?;
    std::fs::create_dir_all(directory)?;
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
                store.set("settings_shortcut", &shortcut.to_string_lossy())?;
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

fn record_settings_shortcut_result(store: &Store, result: Result<()>) -> Result<()> {
    let warning = match result {
        Ok(()) => String::new(),
        Err(error) => format!(
            "The Start menu shortcut could not be created. Open Settings by launching Desktop Pilot again. {error:#}"
        ),
    };
    store.transaction(|| {
        if store.get::<String>("settings_shortcut_warning")? != warning {
            store.set("settings_shortcut_warning", &warning)?;
            if !warning.is_empty() {
                store.event("warning", &warning)?;
            }
        }
        Ok(())
    })
}

pub fn finish(store: &Store, config: &Config) -> Result<()> {
    let _starting = native::exclusive("startup", &store.home)?;
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
    record_settings_shortcut_result(store, make_settings_shortcut(store, &installed))?;
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
        store.set("background_disabled", &false)?;
        store.event("setup", "Initial setup completed")?;
        Ok(())
    });
    if let Err(error) = prepared {
        return Err(restore_autostart(store, error));
    }
    start_installed(store, &installed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_home(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "test-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn updates_reject_downgrades_and_unknown_versions_over_versioned_installs() {
        assert!(check_upgrade_version(Some([0, 2, 1, 0]), Some([0, 3, 0, 0])).is_err());
        assert!(check_upgrade_version(None, Some([0, 3, 0, 0])).is_err());
        assert!(check_upgrade_version(Some([0, 10, 0, 0]), Some([0, 9, 9, 0])).is_ok());
        assert!(check_upgrade_version(Some([1, 0, 0, 0]), Some([1, 0, 0, 0])).is_ok());
        assert!(check_upgrade_version(Some([1, 0, 0, 0]), None).is_ok());
    }

    #[test]
    fn backup_collision_does_not_stop_an_existing_worker() {
        let root = test_home("backup-preflight");
        let store = Store::open(&root).unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("shutdown", &false).unwrap();
        let installed = installed_exe(&root);
        let source = root.join("downloaded.exe");
        std::fs::write(&installed, b"old executable").unwrap();
        std::fs::write(&source, b"new executable").unwrap();
        let backup = root.join(format!(
            "desktop-pilot.previous-{}.exe",
            &hash(b"old executable")[..16]
        ));
        std::fs::write(&backup, b"an unrelated file").unwrap();
        let worker = native::exclusive("background", &root).unwrap();
        let error = install_from(&store, &source).unwrap_err();
        assert!(format!("{error:#}").contains("backup path"), "{error:#}");
        assert!(!store.get::<bool>("shutdown").unwrap());
        assert!(native::is_running("background", &root).unwrap());
        assert_eq!(std::fs::read(installed).unwrap(), b"old executable");
        assert_eq!(std::fs::read(backup).unwrap(), b"an unrelated file");
        drop(worker);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabling_a_profile_preserves_history_and_does_not_touch_another_profile() {
        let root = test_home("disable");
        let other_home = test_home("other-profile");
        let store = Store::open(&root).unwrap();
        let other = Store::open(&other_home).unwrap();
        let config = Config {
            collect: true,
            automatic: true,
            autostart: true,
            ..Default::default()
        };
        store.set("config", &config).unwrap();
        other.set("config", &config).unwrap();
        store.set("custom-history", &"keep history").unwrap();
        let original = root.join("original-user-file.txt");
        std::fs::write(&original, b"keep file").unwrap();
        let other_worker = native::exclusive("background", &other_home).unwrap();
        let disabled = disable_background(&root).unwrap();
        assert!(
            !disabled.collect
                && !disabled.automatic
                && !disabled.autostart
                && !disabled.learning_enabled
        );
        assert_eq!(store.get::<Config>("config").unwrap(), disabled);
        assert!(store.get::<bool>("background_disabled").unwrap());
        assert!(store.get::<bool>("shutdown").unwrap());
        assert_eq!(
            store.get::<String>("custom-history").unwrap(),
            "keep history"
        );
        assert_eq!(std::fs::read(original).unwrap(), b"keep file");
        assert_eq!(other.get::<Config>("config").unwrap(), config);
        assert!(!other.get::<bool>("background_disabled").unwrap());
        assert!(!other.get::<bool>("shutdown").unwrap());
        assert!(native::is_running("background", &other_home).unwrap());
        drop(other_worker);
        drop(other);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(other_home).unwrap();
    }

    #[test]
    fn disable_cancels_unfinished_commands_and_allows_manual_control() {
        let root = test_home("disable-commands");
        let store = Store::open(&root).unwrap();
        let config = Config {
            learning_enabled: true,
            ..Default::default()
        };
        store.set("config", &config).unwrap();
        store.command("apply").unwrap();
        store.command("rollback").unwrap();
        store.command("rescan").unwrap();
        let commands = store.pending().unwrap();
        assert!(store.claim_command(commands[1].0).unwrap());
        store
            .finish(commands[2].0, &Ok("Completed earlier".into()))
            .unwrap();
        let disabled = disable_background(&root).unwrap();
        assert!(!disabled.learning_enabled);
        assert!(store.pending().unwrap().is_empty());
        let mut query = store
            .db
            .prepare("SELECT status,message FROM commands ORDER BY id")
            .unwrap();
        let states: Vec<(String, String)> = query
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(states[0].0, "error");
        assert_eq!(states[1].0, "error");
        assert!(states[0].1.contains("background work was disabled"));
        assert_eq!(states[1].1, states[0].1);
        assert_eq!(states[2], ("done".into(), "Completed earlier".into()));
        drop(query);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_disable_rolls_back_configuration_and_persistent_disabled_flag() {
        let root = test_home("disable-rollback");
        let store = Store::open(&root).unwrap();
        let config = Config {
            collect: true,
            automatic: true,
            autostart: true,
            ..Default::default()
        };
        store.set("config", &config).unwrap();
        store.command("apply").unwrap();
        store.command("rollback").unwrap();
        let commands = store.pending().unwrap();
        assert!(store.claim_command(commands[1].0).unwrap());
        store.set("background_disabled", &false).unwrap();
        store.db.execute_batch("CREATE TRIGGER refuse_disabled BEFORE INSERT ON kv WHEN NEW.k='background_disabled' BEGIN SELECT RAISE(ABORT, 'fixture failure'); END;").unwrap();
        assert!(disable_background(&root).is_err());
        assert_eq!(store.get::<Config>("config").unwrap(), config);
        assert!(!store.get::<bool>("background_disabled").unwrap());
        assert!(!store.get::<bool>("shutdown").unwrap());
        assert_eq!(store.pending().unwrap(), vec![commands[0].clone()]);
        let running: String = store
            .db
            .query_row(
                "SELECT status FROM commands WHERE id=?1",
                [commands[1].0],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(running, "running");
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn opening_a_disabled_profile_updates_the_executable_without_starting_it() {
        let root = test_home("disabled-update-кириллица");
        let store = Store::open(&root).unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("background_disabled", &true).unwrap();
        store.set("custom-history", &"keep history").unwrap();
        let installed = installed_exe(&root);
        std::fs::write(&installed, b"old executable").unwrap();
        prepare_settings(&root).unwrap();
        assert!(!native::is_running("background", &root).unwrap());
        assert!(store.get::<bool>("background_disabled").unwrap());
        assert_eq!(
            store.get::<String>("custom-history").unwrap(),
            "keep history"
        );
        assert_eq!(
            hash(&std::fs::read(installed).unwrap()),
            hash(&std::fs::read(std::env::current_exe().unwrap()).unwrap())
        );
        assert_eq!(
            std::fs::read(root.join(format!(
                "desktop-pilot.previous-{}.exe",
                &hash(b"old executable")[..16]
            )))
            .unwrap(),
            b"old executable"
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_serializes_stop_disable_and_settings_preparation() {
        let root = test_home("startup-serialization");
        let store = Store::open(&root).unwrap();
        let startup = native::exclusive("startup", &root).unwrap();
        assert!(stop_background(&root).is_err());
        assert!(disable_background(&root).is_err());
        assert!(prepare_settings(&root).is_err());
        assert!(start_background(&root).is_err());
        assert!(!store.get::<bool>("shutdown").unwrap());
        assert!(!store.get::<bool>("background_disabled").unwrap());
        drop(startup);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn an_unavailable_start_menu_is_nonfatal_and_recovers_without_overwriting_shortcuts() {
        let root = test_home("optional-start-menu");
        std::fs::create_dir_all(&root).unwrap();
        // Match profile::home, including expansion of an 8.3 TEMP ancestor.
        let root = crate::profile::ordinary_path(std::fs::canonicalize(root).unwrap());
        let store = Store::open(&root).unwrap();
        let installed = installed_exe(&root);
        std::fs::write(&installed, b"inert executable fixture").unwrap();
        let directory = root.join("Programs");
        std::fs::write(&directory, b"existing user file").unwrap();
        record_settings_shortcut_result(
            &store,
            make_settings_shortcut_in(&store, &installed, &directory),
        )
        .unwrap();
        assert!(
            store
                .get::<String>("settings_shortcut_warning")
                .unwrap()
                .starts_with("The Start menu shortcut could not be created.")
        );
        assert_eq!(std::fs::read(&directory).unwrap(), b"existing user file");
        assert!(store.get::<String>("settings_shortcut").unwrap().is_empty());
        std::fs::rename(&directory, root.join("original-user-file")).unwrap();
        std::fs::create_dir(&directory).unwrap();
        let unrelated = directory.join("Desktop Pilot.lnk");
        std::fs::write(&unrelated, b"unrelated shortcut").unwrap();
        for _ in 0..2 {
            record_settings_shortcut_result(
                &store,
                make_settings_shortcut_in(&store, &installed, &directory),
            )
            .unwrap();
            assert!(
                store
                    .get::<String>("settings_shortcut_warning")
                    .unwrap()
                    .is_empty()
            );
            let shortcut = PathBuf::from(store.get::<String>("settings_shortcut").unwrap());
            assert!(shortcut.is_file() && shortcut != unrelated);
            let _com = native::Com::new().unwrap();
            let app = native::read_link(&shortcut).unwrap();
            assert_eq!(norm(&app.target), norm(&installed.to_string_lossy()));
            assert!(app.arguments.starts_with("--settings --home "));
            assert_eq!(std::fs::read(&unrelated).unwrap(), b"unrelated shortcut");
            assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 2);
        }
        assert_eq!(
            store
                .events()
                .unwrap()
                .iter()
                .filter(|(_, kind, _)| kind == "warning")
                .count(),
            1
        );
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn starting_waits_for_an_existing_worker_to_be_ready() {
        let root = test_home("background-ready");
        let store = Store::open(&root).unwrap();
        store.set("shutdown", &false).unwrap();
        store.set("heartbeat", &0i64).unwrap();
        let worker = native::exclusive("background", &root).unwrap();
        let worker_home = root.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let starter = std::thread::spawn(move || {
            let store = Store::open(&worker_home).unwrap();
            sender
                .send(start_installed(&store, &worker_home.join("missing.exe")))
                .unwrap();
        });
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(150)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        store.set("heartbeat", &now()).unwrap();
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        starter.join().unwrap();
        drop(worker);
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn starting_during_shutdown_waits_then_launches_a_replacement() {
        let root = test_home("background-stopping");
        let store = Store::open(&root).unwrap();
        store.set("shutdown", &true).unwrap();
        store.set("heartbeat", &now()).unwrap();
        let worker = native::exclusive("background", &root).unwrap();
        let worker_home = root.clone();
        let (sender, receiver) = std::sync::mpsc::channel();
        let starter = std::thread::spawn(move || {
            let store = Store::open(&worker_home).unwrap();
            sender
                .send(start_installed(&store, &worker_home.join("missing.exe")))
                .unwrap();
        });
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(150)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(store.get::<bool>("shutdown").unwrap());
        drop(worker);
        let error = receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err();
        // A missing executable distinguishes a real launch attempt from a
        // premature success based only on the old worker's mutex.
        assert!(format!("{error:#}").contains("Cannot start Desktop Pilot background work"));
        assert!(!native::is_running("background", &root).unwrap());
        assert_eq!(store.get::<i64>("heartbeat").unwrap(), 0);
        starter.join().unwrap();
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }

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
