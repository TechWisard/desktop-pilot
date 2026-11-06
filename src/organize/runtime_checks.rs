//! Native executable acceptance: the worker must really stop and restart.
use super::*;
use crate::setup;
use std::fs as disk;

struct StopOnExit(PathBuf);
impl Drop for StopOnExit {
    fn drop(&mut self) { let _ = setup::stop_background(&self.0); }
}

pub(crate) fn run(parent: &Path) -> Result<()> {
    let home = parent.join("test-organizer-runtime");
    ensure!(!home.exists(), "Organizer runtime acceptance requires a fresh profile");
    let store = Store::open(&home)?;
    let _stop = StopOnExit(store.home.clone());
    let desktop = store.home.join("Desktop");
    disk::create_dir(&desktop)?;
    store.set("test_desktop", &desktop.to_string_lossy())?;
    store.set("setup_complete", &true)?;
    let config = Config { collect: false, automatic: false, autostart: false, language: "en".into(), ..Default::default() };
    store.set("config", &config)?;
    let mut originals = Vec::new();
    for name in ["Alpha", "Beta"] {
        let path = desktop.join(format!("{name}.url"));
        disk::write(&path, format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"))?;
        let app = native::read_link(&path)?;
        store.put_app(&app)?;
        originals.push((app, fs::inspect(&path, true)?));
    }
    setup::start_background(&store.home)?;
    ensure!(native::is_running("background", &store.home)? && store.get::<i64>("heartbeat")? > 0, "Native organizer worker never became ready");
    let plan = analyze(&store, Options::default(), &config, &AtomicBool::new(false))?;
    ensure!(plan.summary() == (2, 1, 1), "Unexpected runtime organizer plan");
    let (outcome, warning) = setup::with_organization_paused(&store.home, |stopped| {
        ensure!(stopped.get::<bool>("shutdown")?, "Worker was not asked to stop before organizer changes");
        let result = apply_stopped(stopped, &plan, &AtomicBool::new(false))?;
        let mut saved: Config = stopped.get("config")?;
        saved.max_replacements = 0;
        stopped.set("config", &saved)?;
        stopped.policy(&originals[0].0.id, Policy::Always)?;
        Ok(result)
    })?;
    ensure!(outcome.moved == 2 && outcome.warnings.is_empty() && warning.is_none(), "Native organizer apply failed: {outcome:?}; {warning:?}");
    ensure!(native::is_running("background", &store.home)? && store.get::<i64>("heartbeat")? > 0, "Worker was not restarted after committed organization");
    ensure!(store.get::<Config>("config")?.max_replacements == 0 && store.app(&originals[0].0.id)?.policy == Policy::Always, "Worker restart overwrote a newer manual choice");
    let operation = status(&store)?.undo.context("Missing runtime organizer Undo")?;
    let (outcome, warning) = setup::with_organization_paused(&store.home, |stopped| undo_stopped(stopped, &operation))?;
    ensure!(outcome.restored == 2 && outcome.warnings.is_empty() && warning.is_none(), "Native organizer Undo failed: {outcome:?}; {warning:?}");
    ensure!(native::is_running("background", &store.home)?, "Worker did not restart after Undo");
    for (app, stamp) in &originals {
        ensure!(fs::inspect(Path::new(&app.source), true)? == *stamp, "Runtime organizer changed original identity or bytes");
        ensure!(store.app(&app.id)?.source == app.source, "Undo did not restore the catalog source");
    }
    let (_, warning) = setup::with_organization_paused(&store.home, |stopped| stopped.set("background_disabled", &true))?;
    ensure!(warning.is_none() && !native::is_running("background", &store.home)?, "A newer Turn off choice was ignored on organizer exit");
    ensure!(!status(&store)?.pending && status(&store)?.undo.is_none(), "Runtime organizer left an unfinished operation");
    Ok(())
}
