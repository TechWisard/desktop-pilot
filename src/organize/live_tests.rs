//! Opt-in acceptance against Explorer in a disposable Windows runner.
//! Never selected by the ordinary test suite or run on a user's desktop.
use super::*;
use std::{fs as disk, io::Write, time::{Duration, Instant}};

struct Cleanup { files: Vec<(PathBuf, String)>, home: PathBuf }
impl Drop for Cleanup {
    fn drop(&mut self) {
        for (path, id) in &self.files {
            if fs::inspect(path, false).is_ok_and(|stamp| stamp.id == *id) { let _ = disk::remove_file(path); }
        }
        let _ = disk::remove_dir_all(&self.home);
    }
}
fn wait_for_items(ids: &[String]) -> Result<native::Layout> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let layout = native::layout()?;
        if ids.iter().all(|id| layout.items.iter().any(|item| fs::inspect(Path::new(&item.path), false).is_ok_and(|stamp| stamp.id == *id))) { return Ok(layout); }
        ensure!(Instant::now() < deadline, "Explorer did not enumerate the acceptance fixtures");
        std::thread::sleep(Duration::from_millis(100));
    }
}
#[test]
#[ignore = "requires the explicit disposable-runner Explorer acceptance job"]
fn live_explorer_organization_round_trip() -> Result<()> {
    ensure!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") && std::env::var("DESKTOP_PILOT_DISPOSABLE_DESKTOP").as_deref() == Ok("1"), "Live acceptance is restricted to an explicitly selected disposable runner");
    let _com = native::Com::new()?;
    let desktop = crate::profile::ordinary_path(disk::canonicalize(native::desktop()?)?);
    let baseline = native::layout()?;
    ensure!(baseline.flags & 1 == 0, "Explorer auto-arrange must be off; the test does not change system settings");
    let unique = fs::operation_id()?;
    let home = std::env::temp_dir().join(format!("test-live-organizer-{unique}"));
    let mut cleanup = Cleanup { files: Vec::new(), home: home.clone() };
    let store = Store::open(&home)?;
    store.set("setup_complete", &true)?;
    store.set("background_disabled", &true)?;
    let config = Config { language: "en".into(), ..Default::default() };
    store.set("config", &config)?;
    let mut names = Vec::new();
    let mut stamps = Vec::new();
    for suffix in ["Alpha", "Beta"] {
        let name = format!("Desktop Pilot acceptance {unique} {suffix}.url");
        let path = desktop.join(&name);
        let mut file = disk::OpenOptions::new().write(true).create_new(true).open(&path)?;
        write!(file, "[InternetShortcut]\r\nURL=https://example.invalid/{unique}/{suffix}\r\n")?;
        file.sync_all()?;
        drop(file);
        let stamp = fs::inspect(&path, true)?;
        cleanup.files.push((path.clone(), stamp.id.clone()));
        store.put_app(&native::read_link(&path)?)?;
        names.push(name);
        stamps.push(stamp);
    }
    let ids: Vec<_> = stamps.iter().map(|stamp| stamp.id.clone()).collect();
    native::refresh(&desktop);
    let before = wait_for_items(&ids)?;
    let before_positions: Vec<_> = ids.iter().map(|id| before.items.iter().find(|item| fs::inspect(Path::new(&item.path), false).is_ok_and(|stamp| stamp.id == *id)).unwrap().position.clone()).collect();
    let mut plan = analyze(&store, Options::default(), &config, &AtomicBool::new(false))?;
    for item in &mut plan.items { item.selected = names.contains(&item.name); }
    ensure!(plan.summary() == (2, 1, 1), "Expected exactly two fixture moves and one net freed icon");
    let movements = plan.movements();
    let outcome = apply(&store, &plan, &AtomicBool::new(false))?;
    ensure!(outcome.moved == 2, "The real filesystem operation did not move both fixtures");
    ensure!(outcome.warnings.is_empty(), "Explorer apply warnings: {:?}", outcome.warnings);
    for (index, name) in names.iter().enumerate() {
        let movement = movements.iter().find(|movement| movement.item.name == *name).unwrap();
        let destination = desktop.join(&movement.folder).join(name);
        cleanup.files.push((destination.clone(), stamps[index].id.clone()));
        ensure!(fs::inspect(&destination, true)? == stamps[index], "Moving changed a fixture's identity or bytes");
        ensure!(!desktop.join(name).exists(), "An original fixture was not moved");
    }
    let operation = status(&store)?.undo.context("Committed operation has no Undo")?;
    let outcome = undo(&store, &operation)?;
    ensure!(outcome.restored == 2 && outcome.warnings.is_empty(), "Explorer Undo result: {outcome:?}");
    let after = wait_for_items(&ids)?;
    for (index, name) in names.iter().enumerate() {
        ensure!(fs::inspect(&desktop.join(name), true)? == stamps[index], "Undo changed the original fixture");
        let restored = after.items.iter().find(|item| fs::inspect(Path::new(&item.path), false).is_ok_and(|stamp| stamp.id == ids[index])).unwrap();
        ensure!(restored.position == before_positions[index], "Explorer did not restore a fixture's original position");
    }
    for original in &baseline.items {
        let current = after.items.iter().find(|item| norm(&item.path) == norm(&original.path)).context("An unrelated Explorer item disappeared")?;
        ensure!(current.position == original.position, "An unrelated Explorer icon moved: {}", original.name);
    }
    ensure!(!status(&store)?.pending && status(&store)?.undo.is_none(), "Undo left an active transaction");
    ensure!(store.get::<bool>("background_disabled")?, "Acceptance restarted a disabled worker");
    println!("LIVE_EXPLORER_ACCEPTANCE: two identity-preserving moves, one net icon freed, two restored positions, unrelated icons unchanged");
    drop(store);
    drop(cleanup);
    native::refresh(&desktop);
    Ok(())
}
