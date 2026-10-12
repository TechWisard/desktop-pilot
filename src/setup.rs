use crate::{engine, model::*, native, store::Store};
use anyhow::{Result, ensure};
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
};

pub fn installed_exe(home: &Path) -> PathBuf {
    home.join("desktop-pilot.exe")
}
pub fn start_background(home: &Path) -> Result<()> {
    if native::exclusive("background", home).is_err() {
        return Ok(());
    }
    let installed = installed_exe(home);
    let exe = if installed.is_file() {
        installed
    } else {
        std::env::current_exe()?
    };
    std::process::Command::new(exe)
        .arg("--background")
        .arg("--home")
        .arg(home)
        .creation_flags(0x08000000)
        .spawn()?;
    Ok(())
}
pub fn finish(store: &Store, config: &Config) -> Result<()> {
    config.validate()?;
    let layout = native::layout()?;
    ensure!(
        layout.flags & 1 == 0 || !config.automatic,
        "Turn off Explorer auto-arrange, or choose tracking without automatic changes."
    );
    if config.respect_taskbar_pins {
        native::pin_identities()?;
    }
    if config.automatic {
        engine::reserve_positions(&layout, config, &State::default(), &[], config.slots)?;
    }
    let current = std::env::current_exe()?;
    let installed = installed_exe(&store.home);
    if norm(&current.to_string_lossy()) != norm(&installed.to_string_lossy()) {
        if installed.exists() {
            ensure!(
                hash(&std::fs::read(&installed)?) == hash(&std::fs::read(&current)?),
                "A different version is already installed. Close it before updating."
            );
        } else {
            std::fs::copy(&current, &installed)?;
        }
    }
    store.set("config", config)?;
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
    if store.get::<String>("test_desktop")?.is_empty() {
        let shortcut = native::programs()?.join("Desktop Pilot.lnk");
        if !shortcut.exists() {
            native::make_link(
                &shortcut,
                &installed,
                &format!("--settings --home \"{}\"", store.home.display()),
                &installed.to_string_lossy(),
                0,
                "Desktop Pilot settings",
            )?;
        }
    }
    store.set("setup_complete", &true)?;
    store.event("setup", "Initial setup completed")?;
    start_background(&store.home)
}
