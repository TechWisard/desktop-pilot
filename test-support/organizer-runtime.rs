//! Acceptance-only executable entry; not part of the distributed program.
//! Runs only on an explicitly authorized disposable GitHub Windows desktop.
use super::*;
use crate::{engine, model::App, setup};
use std::{collections::HashMap, fs as disk, time::{Duration, Instant}};

struct StopWorker(PathBuf);
impl Drop for StopWorker {
    fn drop(&mut self) { let _ = setup::stop_background(&self.0); }
}

fn wait_for_icons(paths: &[PathBuf]) -> Result<native::Layout> {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let layout = native::layout()?;
        if paths.iter().all(|path| layout.items.iter().any(|item|
            norm(&item.path) == norm(&path.to_string_lossy())
                || fs::inspect(path, true).is_ok_and(|stamp|
                    fs::matches_id(Path::new(&item.path), &stamp.id).unwrap_or(false)))) {
            return Ok(layout);
        }
        ensure!(Instant::now() < deadline, "Explorer did not enumerate the synthetic desktop objects");
        native::refresh(&native::desktop()?);
        std::thread::sleep(Duration::from_millis(150));
    }
}

fn preserved_layout(before: &native::Layout, after: &native::Layout) -> Result<()> {
    for item in &before.items {
        let current = after.items.iter().find(|other| norm(&other.path) == norm(&item.path))
            .with_context(|| format!("An unrelated desktop object disappeared: {}", item.name))?;
        ensure!(item.position == current.position,
            "An unrelated icon moved: {} {:?} -> {:?}", item.name, item.position, current.position);
    }
    Ok(())
}

fn healthy_worker(store: &Store, expected: &Config) -> Result<u32> {
    ensure!(native::is_running("background", &store.home)?, "Background process did not resume");
    ensure!(!store.get::<bool>("shutdown")?, "Resumed process still has shutdown intent");
    ensure!(store.get::<i64>("heartbeat")? > 0, "Resumed process has no readiness heartbeat");
    ensure!(store.get::<Config>("config")? == *expected, "Organization changed observation/startup preferences");
    let pid = store.get::<u32>("background_pid")?;
    ensure!(pid != 0 && pid != std::process::id(), "No separate worker process was observed");
    Ok(pid)
}

pub(crate) fn run(home: &Path) -> Result<serde_json::Value> {
    ensure!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true")
        && std::env::var("DP_ALLOW_DISPOSABLE_DESKTOP_TEST").as_deref() == Ok("YES_SYNTHETIC_FIXTURES_ONLY"),
        "This acceptance command is restricted to an explicitly enabled disposable runner");
    ensure!(home.file_name().is_some_and(|name| name.to_string_lossy().starts_with("test-")),
        "An isolated test-* profile is required");
    ensure!(!home.exists(), "Acceptance will not reuse an existing profile");
    let store = Store::open(home)?;
    let _stop = StopWorker(store.home.clone());
    let baseline = native::layout()?;
    ensure!(baseline.flags & 1 == 0, "Explorer auto-arrange is enabled; no desktop settings were changed");
    let desktop = root(&store)?;
    let config = Config { slots: 4, learning_enabled: false, respect_taskbar_pins: false,
        automatic: false, collect: false, autostart: false, ..Default::default() };
    let fixture_config = Config { slots: 8, ..config.clone() };
    let area = plan::find_free_zone(&baseline, &fixture_config)
        .context("No room for the bounded synthetic desktop fixture")?;
    let points = engine::reserve_positions(&baseline, &area, &State::default(), &[], 8)?;
    let prefix = format!("DP-acceptance-{}", fs::operation_id()?);
    let mut originals = Vec::new();
    let mut apps = Vec::<App>::new();
    let mut bytes_by_name = HashMap::new();
    for index in 0..8 {
        let name = format!("{prefix}-{index}.url");
        let path = desktop.join(&name);
        let bytes = format!("[InternetShortcut]\r\nURL=https://example.invalid/{prefix}/{index}\r\n").into_bytes();
        use std::io::Write;
        let mut file = disk::OpenOptions::new().write(true).create_new(true).open(&path)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        let app = native::read_link(&path)?;
        store.put_app(&app)?;
        apps.push(app);
        bytes_by_name.insert(name, bytes);
        originals.push(path);
    }
    native::refresh(&desktop);
    wait_for_icons(&originals)?;
    let positions: Vec<_> = originals.iter().zip(&points)
        .map(|(path, point)| (path.to_string_lossy().into_owned(), point.clone())).collect();
    native::position_many(&positions)?;
    let original_layout = native::layout()?;
    preserved_layout(&baseline, &original_layout)?;
    let identities: Vec<_> = originals.iter().map(|path| fs::inspect(path, true)).collect::<Result<_>>()?;
    store.set("config", &config)?;
    store.set("setup_complete", &true)?;
    store.set("background_disabled", &false)?;
    store.observe(&apps[0].id, 180, true, false)?;
    let history = serde_json::to_value(store.days()?)?;
    disk::copy(std::env::current_exe()?, setup::installed_exe(&store.home))?;
    setup::start_background(&store.home)?;
    let first_pid = healthy_worker(&store, &config)?;

    let mut plan = super::plan::analyze(&store, Options::default(), &config, &AtomicBool::new(false))?;
    // Never authorize a standard runner icon or any pre-existing object.
    for item in &mut plan.items { item.selected = item.name.starts_with(&prefix); }
    ensure!(plan.summary() == (8, 1, 7), "Unexpected actual-desktop organization plan: {:?}", plan.summary());
    let projected = plan.projected_zone(&config).context("No quick-access area projected after grouping")?;
    let (organized, warning) = setup::with_organization_paused(&store.home, |paused| {
        // The startup barrier is tested from another thread, as in another Settings window.
        let other = paused.home.clone();
        let refused = std::thread::spawn(move || setup::disable_background(&other).is_err())
            .join().map_err(|_| anyhow::anyhow!("Concurrent Settings probe panicked"))?;
        ensure!(refused, "Concurrent Turn off interleaved with organization startup lock");
        journal::apply_stopped(paused, &plan, &AtomicBool::new(false))
    })?;
    ensure!(warning.is_none(), "Worker resume warning: {warning:?}");
    ensure!(organized.moved == 8 && organized.folders_created == 1, "The actual desktop did not organize the full fixture");
    ensure!(organized.warnings.is_empty(), "Actual placement warnings: {:?}", organized.warnings);
    let second_pid = healthy_worker(&store, &config)?;
    ensure!(second_pid != first_pid, "The worker was not stopped and restarted around moves");
    let operation = status(&store)?.undo.context("Organization Undo missing")?;
    let durable = load(&store, &operation)?;
    let folder = durable.folders.first().context("Category folder missing")?;
    let after_layout = native::layout()?;
    preserved_layout(&baseline, &after_layout)?;
    for (index, original) in originals.iter().enumerate() {
        ensure!(!original.exists(), "Original was copied rather than moved");
        let relocated = desktop.join(&folder.name).join(original.file_name().unwrap());
        ensure!(fs::inspect(&relocated, true)? == identities[index], "Relocation changed original file identity");
    }
    engine::reserve_positions(&after_layout, &projected, &State::default(), &[], config.slots)?;

    // Work with the organized catalog, then execute the two real Undo mechanisms.
    let mut placement_config = config.clone();
    placement_config.zone_col = projected.zone_col;
    placement_config.zone_row = projected.zone_row;
    placement_config.zone_columns = projected.zone_columns;
    store.set("config", &placement_config)?;
    store.policy(&apps[0].id, Policy::Always)?;
    let (_, warning) = setup::with_organization_paused(&store.home, |paused| engine::apply(paused, true))?;
    ensure!(warning.is_none(), "Worker did not resume after actual quick placement");
    let placed: State = store.get("state")?;
    ensure!(placed.owned.len() == 1, "The explicit quick shortcut was not placed");
    let quick = &placed.owned[0];
    ensure!(quick.file_id.as_ref() != Some(&identities[0].id), "Quick placement claimed the original");
    if norm(&quick.path) == norm(&apps[0].source) {
        let failure = setup::with_organization_paused(&store.home, |paused| journal::undo_stopped(paused, &operation));
        ensure!(failure.is_err() && !status(&store)?.pending, "The filename collision must be rejected before partial Undo");
        healthy_worker(&store, &placement_config)?;
    }
    let (_, warning) = setup::with_organization_paused(&store.home, engine::rollback)?;
    ensure!(warning.is_none(), "Worker did not resume after quick Undo");
    ensure!(store.get::<State>("state")?.owned.is_empty(), "Quick placement Undo left owned copies");
    let (restored, warning) = setup::with_organization_paused(&store.home, |paused| journal::undo_stopped(paused, &operation))?;
    ensure!(warning.is_none(), "Worker did not resume after organization Undo");
    ensure!(restored.restored == 8 && restored.warnings.is_empty(), "Desktop restoration was incomplete: {restored:?}");
    let final_pid = healthy_worker(&store, &placement_config)?;
    wait_for_icons(&originals)?;
    let final_layout = native::layout()?;
    preserved_layout(&original_layout, &final_layout)?;
    for (index, path) in originals.iter().enumerate() {
        ensure!(fs::inspect(path, true)? == identities[index], "Undo changed an original identity");
        ensure!(disk::read(path)? == bytes_by_name[path.file_name().unwrap().to_str().unwrap()], "Undo changed original bytes");
        ensure!(norm(&store.app(&apps[index].id)?.source) == norm(&apps[index].source), "Catalog source was not restored");
    }
    ensure!(store.app(&apps[0].id)?.policy == Policy::Always, "Manual rule was lost");
    ensure!(serde_json::to_value(store.days()?)? == history, "Organization changed usage history");
    setup::disable_background(&store.home)?;
    ensure!(!native::is_running("background", &store.home)?, "Explicit Turn off did not stop the worker");
    ensure!(store.get::<bool>("background_disabled")?, "Explicit Turn off preference was lost");
    // Remove only verified synthetic originals; never delete a foreign collision.
    for (path, stamp) in originals.iter().zip(&identities) {
        ensure!(fs::inspect(path, true)? == *stamp, "Cleanup refused a replaced fixture");
        disk::remove_file(path)?;
    }
    native::refresh(&desktop);
    Ok(serde_json::json!({
        "success": true, "actual_explorer": true, "synthetic_originals": 8,
        "organized": organized, "restored": restored, "projected_free_slots": config.slots,
        "worker_pids": [first_pid, second_pid, final_pid],
        "unrelated_icons_and_restored_positions_verified": true,
        "catalog_history_rules_and_original_identities_verified": true,
        "concurrent_settings_refused": true,
        "physical_hardware_test": false,
        "source": "Instrumented acceptance copy of the committed production modules"
    }))
}
