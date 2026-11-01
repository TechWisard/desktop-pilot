#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod activity;
#[cfg(test)]
mod background_entry_tests;
mod cli;
#[cfg(test)]
mod cli_startup_race_tests;
mod engine;
mod i18n;
#[cfg(test)]
mod launch_validity_tests;
mod learning;
mod model;
mod native;
mod profile;
mod report;
mod setup;
mod store;
mod tray;
mod ui;
use anyhow::{Context, Result, ensure};
use model::*;
use std::path::{Path, PathBuf};
use store::Store;

fn home(args: &[String]) -> Result<PathBuf> {
    profile::home(args)
}
fn emit(args: &[String], value: &impl serde::Serialize) -> Result<()> {
    let text = serde_json::to_string_pretty(value)?;
    if let Some(i) = args.iter().position(|s| s == "--out") {
        let written = report::write(
            Path::new(args.get(i + 1).context("Нет пути --out")?),
            text.as_bytes(),
        );
        if matches!(
            args[0].as_str(),
            "--init" | "--apply-initial" | "--apply" | "--rollback" | "--disable" | "--self-test"
        ) {
            written.context("The action completed, but its report could not be saved. Do not repeat the action just to export a report.")?;
        } else {
            written?;
        }
    } else {
        println!("{text}");
    }
    Ok(())
}
fn prepare_report(args: &[String]) -> Result<()> {
    if let Some(index) = args.iter().position(|argument| argument == "--out") {
        report::preflight(Path::new(&args[index + 1]))?;
    }
    Ok(())
}
/// Input provenance hooks are needed only while activity collection is
/// enabled. The monitor rechecks mode and shutdown after installation because
/// Settings may pause collection or turn off background work concurrently.
fn install_input_hooks_if_enabled<T>(
    store: &Store,
    install: impl FnOnce() -> Result<T>,
) -> Result<Option<T>> {
    if store.get::<bool>("background_disabled")? || !store.get::<Config>("config")?.collect {
        return Ok(None);
    }
    Ok(Some(install()?))
}

/// Registers the actual background entry after the profile's singleton is claimed.
/// Kept separate so the database transition is exercised by native regressions.
fn register_background_session(store: &Store) -> Result<bool> {
    // The background singleton is already held. Turn off retains that lock
    // until its SQLite disable transaction commits, so this admission decision
    // cannot overtake a completed user stop.
    store.transaction(|| {
        if store.get::<bool>("background_disabled")? {
            return Ok(false);
        }
        store.set(
            "input_filter",
            &"non-injected keyboard/buttons/wheel; fresh foreground interaction",
        )?;
        store.set("shutdown", &false)?;
        store.set("heartbeat", &0i64)?;
        store.set("background_pid", &std::process::id())?;
        store.set("background_version", &env!("CARGO_PKG_VERSION"))?;
        Ok(true)
    })
}

fn background_running(home: &Path) -> Result<bool> {
    native::is_running("background", home)
}

/// Coordinate direct CLI changes with explicit Settings startup/disable.
fn direct_cli_change<T>(store: &Store, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let _starting = native::exclusive("startup", &store.home)?;
    ensure!(
        !background_running(&store.home)?,
        "При работающем фоне используйте окно настроек"
    );
    #[cfg(test)]
    cli_startup_race_tests::pause_before_change(&store.home);
    action()
}
// Validate the original link on each legacy launch, rather than trusting
// catalog data that could predate a drive removal or an application update.
fn checked_launch_source(store: &Store, id: &str) -> Result<App> {
    let app = store.app(id)?;
    ensure!(
        Path::new(&app.source).is_file(),
        "Исходный ярлык перемещён или удалён. Откройте настройки и обновите список."
    );
    let actual = native::read_link(Path::new(&app.source))?;
    ensure!(
        actual.identity() == app.identity(),
        "Исходный ярлык изменён. Обновите список в настройках перед запуском."
    );
    ensure!(
        actual.valid,
        "Цель ярлыка или рабочая папка недоступны. Восстановите приложение или подключите диск."
    );
    Ok(actual)
}
fn run(error_profile: &mut Option<PathBuf>) -> Result<()> {
    let args = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if let Some(index) = args.iter().position(|argument| argument == "--out") {
        report::validate(Path::new(&args[index + 1]))?;
    }
    match args[0].as_str() {
        "--help" => {
            println!("{}", cli::HELP);
            return Ok(());
        }
        "--version" => {
            println!("Desktop Pilot {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        "--probe" => {
            use std::io::Write;
            let mut marker = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&args[1])?;
            marker.write_all(&serde_json::to_vec(&serde_json::json!({
                "cwd": std::env::current_dir()?, "args": args
            }))?)?;
            marker.sync_all()?;
            return Ok(());
        }
        _ => {}
    }
    native::dpi();
    let _com = native::Com::new()?;
    match args[0].as_str() {
        "--layout" => return emit(&args, &native::layout()?),
        "--pins" => return emit(&args, &native::pin_identities()?),
        "--catalog" => {
            let paths: Vec<String> = serde_json::from_slice(&std::fs::read(&args[1])?)?;
            let desktop = native::desktop()?;
            let mut apps = Vec::new();
            for path in paths {
                let mut app = native::read_link(Path::new(&path))?;
                app.category = Path::new(&path)
                    .strip_prefix(&desktop)
                    .ok()
                    .and_then(|relative| relative.parent())
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .and_then(|parent| parent.components().next())
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .unwrap_or_default();
                apps.push(app);
            }
            return emit(&args, &apps);
        }
        _ => {}
    }
    let h = home(&args)?;
    if args[0] == "--self-test" {
        return self_test(&h, &args, error_profile);
    }
    if args[0] == "--ui-smoke" {
        ensure!(
            h.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("test-")),
            "UI screenshots require an isolated test-* profile"
        );
    }
    let s = Store::open(&h)?;
    // A rejected --home folder is not ours. Only write diagnostics after the
    // profile has been accepted, without creating files in unrelated folders.
    *error_profile = Some(s.home.clone());
    prepare_report(&args)?;
    let h = s.home.clone();
    // Stopping a process must remain possible even when its configuration is damaged.
    if let Ok(config) = s.get::<Config>("config") {
        i18n::set(&config.language);
    }
    match args.first().map(String::as_str).unwrap_or("--settings") {
        "--init" => {
            let _starting = native::exclusive("startup", &h)?;
            ensure!(
                !background_running(&h)?,
                "При работающем фоне используйте окно настроек"
            );
            let _lock = native::exclusive("changes", &h)?;
            ensure!(
                s.apps()?.is_empty(),
                "База уже инициализирована; импорт поверх неё запрещён"
            );
            let path = args.get(1).context("Нет seed.json")?;
            let list: Vec<App> = serde_json::from_slice(&std::fs::read(path)?)?;
            let mut validated = Vec::with_capacity(list.len());
            let mut identities = std::collections::HashSet::new();
            for mut app in list {
                let actual = native::read_link(Path::new(&app.source))?;
                ensure!(
                    app.identity() == actual.identity(),
                    "Цель исходного ярлыка изменилась: {}",
                    app.name
                );
                app.id = actual.id;
                app.source = actual.source;
                app.icon = actual.icon;
                app.icon_index = actual.icon_index;
                app.valid = actual.valid;
                ensure!(
                    identities.insert(app.id.clone()),
                    "Catalog contains duplicate shortcut identities"
                );
                validated.push(app);
            }
            s.transaction(|| {
                ensure!(
                    s.apps()?.is_empty(),
                    "База уже инициализирована; импорт поверх неё запрещён"
                );
                for app in &validated {
                    s.put_app(app)?;
                }
                s.set("config", &Config::default())?;
                s.set("state", &State::default())?;
                s.event(
                    "setup",
                    "Импорт исходных ярлыков и исторических приоритетов завершён",
                )
            })?;
            emit(
                &args,
                &serde_json::json!({"apps":s.apps()?.len(),"config":s.get::<Config>("config")?}),
            )
        }
        "--inspect" => emit(
            &args,
            &serde_json::json!({"version":env!("CARGO_PKG_VERSION"),"config":s.get::<Config>("config")?,"learning":s.get::<learning::Progress>("learning")?,"state":s.get::<State>("state")?,"preview":engine::preview(&s,!s.get::<State>("state")?.initialized)?,"heartbeat":s.get::<i64>("heartbeat")?,"status":s.get::<String>("status")?,"background":background_running(&h)?}),
        ),
        "--apply-initial" => direct_cli_change(&s, || {
            ensure!(
                !s.get::<State>("state")?.initialized,
                "Первичное размещение уже выполнено"
            );
            emit(&args, &engine::apply(&s, true)?)
        }),
        "--apply" => direct_cli_change(&s, || emit(&args, &engine::apply(&s, false)?)),
        "--rollback" => direct_cli_change(&s, || {
            let _lock = native::exclusive("changes", &h)?;
            emit(&args, &engine::rollback(&s)?)
        }),
        "--launch" => {
            let id = args.get(1).context("Нет идентификатора ярлыка")?;
            let actual = checked_launch_source(&s, id)?;
            ensure!(
                model::norm(&actual.target)
                    != model::norm(&std::env::current_exe()?.to_string_lossy())
                    || actual.arguments.starts_with("--probe "),
                "Обнаружен цикл запуска"
            );
            native::open_file(Path::new(&actual.source))?;
            if s.get::<Config>("config")?.collect {
                s.observe(id, 0, false, true)?;
            }
            Ok(())
        }
        "--validate-launch" => {
            let id = args.get(1).context("Нет id")?;
            let actual = checked_launch_source(&s, id)?;
            emit(
                &args,
                &serde_json::json!({"original":actual.source,"target":actual.target,"arguments":actual.arguments,"cwd":actual.cwd,"valid":true,"launched":false}),
            )
        }
        "--background" => {
            ensure!(
                engine::setup_complete(&s)?,
                "Complete initial setup before starting background tracking"
            );
            let Some(_instance) = native::try_exclusive("background", &h)? else {
                return Ok(());
            };
            // Session admission precedes worker startup; the worker owns the
            // input-hook lifetime and never installs hooks in manual-only mode.
            if !register_background_session(&s)? {
                return Ok(());
            }
            let worker_home = h.clone();
            let worker = std::thread::spawn(move || {
                let r = std::panic::catch_unwind(|| engine::monitor(worker_home.clone()))
                    .unwrap_or_else(|_| {
                        Err(anyhow::anyhow!("Background process ended unexpectedly"))
                    });
                if let Ok(s) = Store::open(&worker_home) {
                    if let Err(e) = &r {
                        let _ = s.set("status", &format!("Фоновый учёт остановлен: {e:#}"));
                        let _ = s.event("error", &format!("{e:#}"));
                    }
                    let _ = s.set("shutdown", &true);
                }
                let _ = tray::request_close(&worker_home);
                r
            });
            let result = tray::run(h.clone());
            let stopped = s.set("shutdown", &true);
            let worker_result = worker
                .join()
                .map_err(|_| anyhow::anyhow!("Background process ended unexpectedly"))?;
            let _ = s.set("background_pid", &0u32);
            result.and(stopped).and(worker_result)
        }
        "--stop" => setup::stop_background(&h),
        "--command" => {
            ensure!(
                background_running(&h)?,
                "Background process is not running. Open Settings first."
            );
            let action = args.get(1).context("Нет команды")?;
            s.transaction(|| {
                ensure!(
                    s.get::<i64>("heartbeat")? > 0 && !s.get::<bool>("shutdown")?,
                    "Background work is starting or stopping. Wait a moment and try again."
                );
                s.command(action)
            })
        }
        "--disable" => {
            let config = setup::disable_background(&h)?;
            emit(&args, &serde_json::json!({"disabled":true,"config":config}))
        }
        "--ui-smoke" => ui::run(
            h,
            Some(PathBuf::from(args.get(1).context("Нет папки снимков")?)),
        ),
        "--settings" => {
            let Some(_ui) = native::try_exclusive("settings", &h)? else {
                for _ in 0..10 {
                    let pid: u32 = s.get("settings_pid")?;
                    if pid != 0 && native::activate_process_window(pid)? {
                        ensure!(
                            s.get::<u32>("settings_version_pid")? == pid
                                && s.get::<String>("settings_version")?
                                    == env!("CARGO_PKG_VERSION"),
                            "Another version of Settings is open. Close that window, then open this version again to update."
                        );
                        return Ok(());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                anyhow::bail!("Settings is already opening. Try again in a moment.");
            };
            s.transaction(|| {
                s.set("settings_pid", &std::process::id())?;
                s.set("settings_version_pid", &std::process::id())?;
                s.set("settings_version", &env!("CARGO_PKG_VERSION"))
            })?;
            let result = ui::run(h.clone(), None);
            let _ = s.transaction(|| {
                s.set("settings_pid", &0u32)?;
                s.set("settings_version_pid", &0u32)?;
                s.set("settings_version", &"")
            });
            result
        }
        _ => anyhow::bail!("Неизвестная команда"),
    }
}
fn self_test(home: &Path, args: &[String], error_profile: &mut Option<PathBuf>) -> Result<()> {
    struct BackgroundCleanup(PathBuf);
    impl Drop for BackgroundCleanup {
        fn drop(&mut self) {
            let _ = setup::stop_background(&self.0);
        }
    }
    ensure!(
        home.file_name()
            .is_some_and(|s| s.to_string_lossy().starts_with("test-")),
        "Для self-test нужен отдельный каталог test-*"
    );
    ensure!(
        !home.exists() || std::fs::read_dir(home)?.next().is_none(),
        "Self-test requires a new, empty test-* directory"
    );
    let s = Store::open(home)?;
    *error_profile = Some(s.home.clone());
    prepare_report(args)?;
    ensure!(s.apps()?.is_empty(), "Каталог проверки не пуст");
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop)?;
    s.set("test_desktop", &desktop.to_string_lossy())?;
    let original = home.join("original.lnk");
    let marker = home.join("probe.json");
    let exe = std::env::current_exe()?;
    // Exercise the real executable's CLI validation, not a duplicate parser.
    // A mistaken report filename must neither replace an unrelated user file
    // nor open/create a profile before reporting the error.
    let unopened_profile = home.join("unopened-report-profile");
    for filename in [
        "important-notes.txt",
        "family-photo.jpg",
        "project-archive.zip",
        "saved-document.pdf",
    ] {
        let destination = home.join(filename);
        let original_bytes = b"keep the user's original bytes";
        std::fs::write(&destination, original_bytes)?;
        let rejected = std::process::Command::new(&exe)
            .args(["--inspect", "--home"])
            .arg(&unopened_profile)
            .arg("--out")
            .arg(&destination)
            .output()?;
        ensure!(
            !rejected.status.success()
                && std::fs::read(&destination)?.as_slice() == original_bytes
                && !unopened_profile.exists(),
            "A non-JSON report destination replaced a user file or opened a profile: {filename}"
        );
        std::fs::remove_file(destination)?;
    }
    native::make_link(
        &original,
        &exe,
        &format!(
            "--probe \"{}\" --home \"{}\"",
            marker.display(),
            home.display()
        ),
        "",
        0,
        "fixture",
    )?;
    let mut app = native::read_link(&original)?;
    app.name = "Test shortcut".into();
    app.seed = 20.0;
    app.seed_at = now();
    app.policy = Policy::Always;
    s.put_app(&app)?;
    s.set(
        "config",
        &Config {
            autostart: false,
            collect: true,
            learning_enabled: false,
            ..Default::default()
        },
    )?;
    s.set("state", &State::default())?;
    let original_hash = hash(&std::fs::read(&original)?);
    let mut fixture_config: Config = s.get("config")?;
    fixture_config.learning_enabled = true;
    fixture_config.automatic = true;
    s.set("config", &fixture_config)?;
    ensure!(
        engine::apply(&s, true).is_err(),
        "Passive learning allowed early placement"
    );
    ensure!(
        std::fs::read_dir(&desktop)?.next().is_none(),
        "Passive learning changed the desktop"
    );
    ensure!(
        !engine::automatic_review(&s, true)?,
        "Automatic review bypassed passive learning"
    );
    let mut progress = learning::Progress::default();
    for session in 0..7 {
        for second in (0..300).step_by(2) {
            progress.observe(10000 + session * 4000 + second, "2026-01-01", 2);
        }
    }
    s.set("learning", &progress)?;
    ensure!(
        !engine::automatic_review(&s, false)?,
        "Placement ignored user activity"
    );
    ensure!(
        engine::automatic_review(&s, true)?,
        "Automatic first placement did not start after learning"
    );
    let state: State = s.get("state")?;
    ensure!(state.owned.len() == 1, "Ярлык не создан");
    let managed = &state.owned[0];
    let info = native::read_link(Path::new(&managed.path))?;
    ensure!(
        info.identity() == app.identity(),
        "Quick shortcut identity changed"
    );
    ensure!(
        std::fs::read(&managed.path)? == std::fs::read(&original)?,
        "Quick shortcut is not an exact copy"
    );
    // A missing catalog entry must not prevent the shortcut's original target from opening.
    s.db.execute("DELETE FROM apps WHERE id=?1", [&app.id])?;
    // Exercise Shell only against our inert fixture binary.
    native::open_file(Path::new(&managed.path))?;
    for _ in 0..50 {
        if marker.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    ensure!(marker.exists(), "Shell не выполнил цепочку быстрого ярлыка");
    s.put_app(&app)?;
    ensure!(
        s.days()?.is_empty(),
        "Shortcut launch changed the demand score"
    );
    engine::rollback(&s)?;
    ensure!(
        !Path::new(&managed.path).exists(),
        "Копия осталась после отката"
    );
    ensure!(
        hash(&std::fs::read(&original)?) == original_hash,
        "Исходный ярлык изменён"
    );
    // A prepared journal is recoverable even if the process stopped after writing files.
    engine::apply(&s, true)?;
    let interrupted: State = s.get("state")?;
    s.db.execute(
        "UPDATE transactions SET status='prepared' WHERE status='committed'",
        [],
    )?;
    ensure!(
        engine::apply(&s, false).is_err(),
        "Незавершённая операция не блокирует новые изменения"
    );
    engine::rollback(&s)?;
    ensure!(
        interrupted
            .owned
            .iter()
            .all(|o| !Path::new(&o.path).exists()),
        "Восстановление прерванной операции не завершено"
    );

    engine::apply(&s, true)?;
    let moving: State = s.get("state")?;
    let renamed = desktop.join("Renamed by user.lnk");
    std::fs::rename(&moving.owned[0].path, &renamed)?;
    engine::reconcile_manual(&s)?;
    let renamed_state: State = s.get("state")?;
    ensure!(
        renamed_state.owned[0].manual && Path::new(&renamed_state.owned[0].path) == renamed,
        "Переименование не сохранено"
    );
    std::fs::remove_file(&renamed)?;
    engine::reconcile_manual(&s)?;
    ensure!(
        s.get::<State>("state")?.owned.is_empty() && s.app(&app.id)?.policy == Policy::Never,
        "Удалённый ярлык не исключён"
    );
    s.policy(&app.id, Policy::Always)?;
    engine::apply(&s, true)?;
    let st: State = s.get("state")?;
    let file = &st.owned[0].path;
    std::fs::write(file, b"manual edit")?;
    ensure!(
        engine::rollback(&s).is_err(),
        "Откат перезаписывает пользовательский файл"
    );
    engine::reconcile_manual(&s)?;
    ensure!(
        s.get::<State>("state")?.owned.is_empty(),
        "Ручное изменение не освобождено от управления"
    );
    ensure!(
        std::fs::read(file)? == b"manual edit",
        "Ручной файл потерян"
    );
    let fresh = Store::open(&home.join("test-first-run"))?;
    let _fresh_cleanup = BackgroundCleanup(fresh.home.clone());
    let empty_desktop = fresh.home.join("Desktop");
    std::fs::create_dir_all(&empty_desktop)?;
    fresh.set("test_desktop", &empty_desktop.to_string_lossy())?;
    ensure!(
        !engine::setup_complete(&fresh)?,
        "Fresh profile is already initialized"
    );
    setup::finish(
        &fresh,
        &Config {
            respect_taskbar_pins: false,
            ..Default::default()
        },
    )?;
    let ready = (0..100).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        fresh.get::<i64>("heartbeat").unwrap_or_default() > 0
    });
    let stop = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args(["--stop", "--home"])
        .arg(&fresh.home)
        .status()?;
    let stopped = (0..60).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        background_running(&fresh.home).is_ok_and(|running| !running)
    });
    ensure!(
        ready && stopped && stop.success(),
        "First-run background lifecycle failed"
    );
    ensure!(
        engine::setup_complete(&fresh)?,
        "Setup completion was not persisted"
    );
    ensure!(
        fresh.days()?.is_empty(),
        "Disabled collection recorded usage"
    );
    ensure!(
        std::fs::read_dir(&empty_desktop)?.next().is_none(),
        "Tracking-only setup created shortcuts"
    );
    // Unfinished requests from a previous worker must not make delayed changes
    // when the app is resumed on a different day.
    fresh.command("apply")?;
    fresh.command("rollback")?;
    setup::start_background(&fresh.home)?;
    let abandoned: i64 = fresh.db.query_row(
        "SELECT COUNT(*) FROM commands WHERE status='error'",
        [],
        |row| row.get(0),
    )?;
    ensure!(
        abandoned == 2 && fresh.pending()?.is_empty(),
        "Old pending commands were replayed on restart"
    );
    let queued = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args(["--command", "rescan", "--home"])
        .arg(&fresh.home)
        .output()?;
    ensure!(
        queued.status.success(),
        "A ready worker rejected a new command"
    );
    let completed = (0..100).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        fresh
            .db
            .query_row(
                "SELECT COUNT(*) FROM commands WHERE action='rescan' AND status='done'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .is_ok_and(|count| count == 1)
    });
    ensure!(
        completed,
        "The queued command did not complete exactly once"
    );
    fresh.set("disable_sentinel", &"keep profile and rules")?;
    let disabled_report = fresh.home.join("disabled.json");
    let disabled = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args(["--disable", "--home"])
        .arg(&fresh.home)
        .arg("--out")
        .arg(&disabled_report)
        .output()?;
    ensure!(
        disabled.status.success() && !background_running(&fresh.home)?,
        "Disabling did not stop the running worker"
    );
    let disabled_config: Config = fresh.get("config")?;
    ensure!(
        !disabled_config.collect
            && !disabled_config.automatic
            && !disabled_config.autostart
            && fresh.get::<bool>("background_disabled")?,
        "Disabling did not persist all settings"
    );
    setup::prepare_settings(&fresh.home)?;
    ensure!(
        !background_running(&fresh.home)?
            && fresh.get::<String>("disable_sentinel")? == "keep profile and rules",
        "Opening a disabled profile restarted work or lost profile data"
    );
    // The Windows Run entry invokes --background directly. A delayed/stale
    // launch after Turn off must exit without resetting the saved shutdown.
    let disabled_heartbeat: i64 = fresh.get("heartbeat")?;
    let late = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args(["--background", "--home"])
        .arg(&fresh.home)
        .output()?;
    ensure!(
        late.status.success()
            && !background_running(&fresh.home)?
            && fresh.get::<bool>("background_disabled")?
            && fresh.get::<bool>("shutdown")?
            && fresh.get::<i64>("heartbeat")? == disabled_heartbeat
            && fresh.get::<String>("disable_sentinel")? == "keep profile and rules",
        "A delayed background process ignored Turn off or modified the disabled profile"
    );
    setup::start_background(&fresh.home)?;
    ensure!(
        background_running(&fresh.home)? && !fresh.get::<bool>("background_disabled")?,
        "Explicit resume did not clear the disabled state"
    );
    {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
        let previous_pid: u32 = fresh.get("background_pid")?;
        // Keep a handle until the assertion so Windows cannot reuse the old PID.
        let _previous_process = unsafe {
            OwnedHandle::from_raw_handle(
                OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, previous_pid)?.0,
            )
        };
        fresh.set("shutdown", &true)?;
        setup::start_background(&fresh.home)?;
        ensure!(
            background_running(&fresh.home)?
                && fresh.get::<u32>("background_pid")? != previous_pid
                && fresh.get::<i64>("heartbeat")? > 0
                && !fresh.get::<bool>("shutdown")?,
            "Resume returned before replacing a worker that was already stopping"
        );
    }
    setup::stop_background(&fresh.home)?;
    let database_before = std::fs::read(fresh.home.join("usage.sqlite"))?;
    let rejected = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args(["--inspect", "--home"])
        .arg(&fresh.home)
        .arg("--out")
        .arg(fresh.home.join("usage.sqlite"))
        .output()?;
    ensure!(
        !rejected.status.success()
            && std::fs::read(fresh.home.join("usage.sqlite"))? == database_before
            && fresh.get::<String>("disable_sentinel")? == "keep profile and rules",
        "A report overwrote the profile database"
    );
    let catalog_list = home.join("catalog-paths.json");
    std::fs::write(&catalog_list, serde_json::to_vec(&vec![&original])?)?;
    let unused_profile = home.join("test-diagnostics-no-profile");
    let inspected = std::process::Command::new(&exe)
        .arg("--catalog")
        .arg(&catalog_list)
        .arg("--home")
        .arg(&unused_profile)
        .arg("--out")
        .arg(home.join("catalog-report.json"))
        .output()?;
    ensure!(
        inspected.status.success() && !unused_profile.exists(),
        "Stateless shortcut inspection created a profile"
    );
    // Refusing an unrelated database must leave the entire folder untouched,
    // including a user file that happens to have our diagnostic log's name.
    let unrelated = home.join("test-unrelated-database");
    std::fs::create_dir(&unrelated)?;
    let unrelated_db = unrelated.join("usage.sqlite");
    {
        let database = rusqlite::Connection::open(&unrelated_db)?;
        database.execute_batch(
            "CREATE TABLE notes(body TEXT); INSERT INTO notes VALUES('keep original data');",
        )?;
    }
    let unrelated_before = std::fs::read(&unrelated_db)?;
    let original_log = b"This file belongs to the selected folder's owner.";
    std::fs::write(unrelated.join("last-error.txt"), original_log)?;
    let refused = std::process::Command::new(&exe)
        .args(["--inspect", "--home"])
        .arg(&unrelated)
        .output()?;
    ensure!(
        !refused.status.success()
            && std::fs::read(&unrelated_db)? == unrelated_before
            && std::fs::read(unrelated.join("last-error.txt"))? == original_log
            && std::fs::read_dir(&unrelated)?.count() == 2,
        "Rejecting an unrelated database modified files in that folder"
    );
    let occupied = home.join("test-nonempty-self-test");
    std::fs::create_dir(&occupied)?;
    std::fs::write(occupied.join("keep.txt"), original_log)?;
    let refused = std::process::Command::new(&exe)
        .args(["--self-test", "--home"])
        .arg(&occupied)
        .output()?;
    ensure!(
        !refused.status.success()
            && std::fs::read(occupied.join("keep.txt"))? == original_log
            && std::fs::read_dir(&occupied)?.count() == 1,
        "A refused self-test created files in a nonempty folder"
    );
    // Even within an accepted profile, replacing an old diagnostic must not
    // follow a hard link and overwrite another file's contents.
    let user_note = fresh.home.join("keep-note.txt");
    let error_log = fresh.home.join("last-error.txt");
    std::fs::write(&user_note, original_log)?;
    if error_log.try_exists()? {
        std::fs::remove_file(&error_log)?;
    }
    std::fs::hard_link(&user_note, &error_log)?;
    let failed_launch = std::process::Command::new(setup::installed_exe(&fresh.home))
        .args([
            "--validate-launch",
            "missing-diagnostic-fixture-id",
            "--home",
        ])
        .arg(&fresh.home)
        .output()?;
    ensure!(
        !failed_launch.status.success()
            && std::fs::read(&user_note)? == original_log
            && std::fs::read(&error_log)? != original_log,
        "Writing an error log overwrote another file through a hard link"
    );
    // Force a real Windows sharing violation after a healthy worker stops.
    // The installed executable remains runnable and must be restarted intact.
    let recovery = Store::open(&home.join("test-update-recovery"))?;
    let _recovery_cleanup = BackgroundCleanup(recovery.home.clone());
    let recovery_desktop = recovery.home.join("Desktop");
    std::fs::create_dir_all(&recovery_desktop)?;
    recovery.set("test_desktop", &recovery_desktop.to_string_lossy())?;
    recovery.set("setup_complete", &true)?;
    recovery.set("update_sentinel", &"preserve data after failed update")?;
    recovery.set(
        "config",
        &Config {
            learning_enabled: false,
            respect_taskbar_pins: false,
            ..Default::default()
        },
    )?;
    let recovery_exe = setup::installed_exe(&recovery.home);
    let mut fixture_bytes = std::fs::read(&exe)?;
    fixture_bytes.extend_from_slice(b"\nupdate recovery fixture\n");
    std::fs::write(&recovery_exe, &fixture_bytes)?;
    let mut previous_worker = std::process::Command::new(&recovery_exe)
        .args(["--background", "--home"])
        .arg(&recovery.home)
        .spawn()?;
    let recovery_ready = (0..150).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(100));
        recovery.get::<i64>("heartbeat").unwrap_or_default() > 0
    });
    ensure!(recovery_ready, "Update recovery fixture did not start");
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let _held_executable = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0)
            .open(&recovery_exe)?;
        ensure!(
            setup::start_background(&recovery.home).is_err(),
            "Update ignored an executable sharing conflict"
        );
        ensure!(
            previous_worker.try_wait()?.is_some()
                && background_running(&recovery.home)?
                && recovery.get::<i64>("heartbeat")? > 0
                && std::fs::read(&recovery_exe)? == fixture_bytes
                && recovery.get::<String>("update_sentinel")?
                    == "preserve data after failed update",
            "A failed update did not restore the previous background worker and data"
        );
    }
    setup::stop_background(&recovery.home)?;
    let _ = previous_worker.wait();
    let protocol = Store::open(&home.join("test-protocol"))?;
    let protocol_desktop = protocol.home.join("Desktop");
    std::fs::create_dir_all(&protocol_desktop)?;
    protocol.set("test_desktop", &protocol_desktop.to_string_lossy())?;
    protocol.set(
        "config",
        &Config {
            learning_enabled: false,
            respect_taskbar_pins: false,
            ..Default::default()
        },
    )?;
    let source_url = protocol.home.join("original.url");
    std::fs::write(
        &source_url,
        b"[InternetShortcut]\r\nURL=steam://rungameid/42\r\n",
    )?;
    let mut url_app = native::read_link(&source_url)?;
    url_app.policy = Policy::Always;
    protocol.put_app(&url_app)?;
    engine::apply(&protocol, true)?;
    let protocol_state: State = protocol.get("state")?;
    let url_copy = Path::new(&protocol_state.owned[0].path);
    ensure!(
        url_copy
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("url")),
        "Protocol shortcut extension changed"
    );
    ensure!(
        std::fs::read(url_copy)? == std::fs::read(&source_url)?,
        "Protocol shortcut bytes changed"
    );
    ensure!(
        native::read_link(url_copy)?.identity() == url_app.identity(),
        "Protocol identity changed"
    );
    engine::rollback(&protocol)?;
    emit(
        args,
        &serde_json::json!({
            "passed": true,
            "checks": [
                "native shortcut creation",
                "target and launch identity",
                "transactional apply",
                "rollback original bytes",
                "Shell launch of inert fixture",
                "launch without a catalog entry",
                "byte-identical protocol shortcuts",
                "manual edit ownership release",
                "interrupted transaction blocks new changes and rolls back",
                "manual rename pins position",
                "manual deletion prevents return",
                "rollback refuses changed files",
                "clean first-run install and background lifecycle",
                "collection disabled and tracking-only desktop unchanged",
                "passive learning blocks placement",
                "seven qualified sessions unlock initial placement",
                "old pending commands are not replayed on restart",
                "new queued commands complete exactly once",
                "disable stops and persists collection automation and startup choices",
                "opening disabled settings keeps the worker stopped and preserves data",
                "delayed Windows startup honors Turn off without changing the profile",
                "explicit resume restarts the worker",
                "resume replaces an already stopping worker before reporting success",
                "non-JSON report destinations preserve user files without opening a profile",
                "report destination cannot overwrite the profile database",
                "stateless catalog inspection does not create a profile",
                "rejected foreign databases leave existing files untouched",
                "refused self-tests leave nonempty folders untouched",
                "error logs preserve hard-linked user files",
                "failed executable replacement restores the previous worker and profile",
            ],
            "originals_unchanged": true,
        }),
    )
}
fn main() {
    let mut error_profile = None;
    if let Err(error) = run(&mut error_profile) {
        let msg = i18n::runtime(&format!("{error:#}"));
        eprintln!("{msg}");
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        let parsed = cli::parse(&raw_args).ok();
        if let Some(h) = error_profile {
            // Atomic replacement also prevents a hard-linked old log from
            // redirecting an error into another file. Reparse points are refused.
            let _ = report::write(&h.join("last-error.txt"), msg.as_bytes());
        }
        if raw_args.is_empty()
            || parsed
                .as_ref()
                .is_some_and(|args| matches!(args[0].as_str(), "--settings" | "--launch"))
        {
            unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::MessageBoxW(
                    None,
                    windows::core::PCWSTR(native::wide(&msg).as_ptr()),
                    windows::core::w!("Desktop Pilot"),
                    windows::Win32::UI::WindowsAndMessaging::MB_OK
                        | windows::Win32::UI::WindowsAndMessaging::MB_ICONERROR,
                );
            }
        }
        std::process::exit(1);
    }
}
