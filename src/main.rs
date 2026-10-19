#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod activity;
mod cli;
mod engine;
mod i18n;
mod learning;
mod model;
mod native;
mod profile;
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
        std::fs::write(args.get(i + 1).context("Нет пути --out")?, text)?;
    } else {
        println!("{text}");
    }
    Ok(())
}
fn background_running(home: &Path) -> Result<bool> {
    native::is_running("background", home)
}
fn run() -> Result<()> {
    let args = cli::parse(&std::env::args().skip(1).collect::<Vec<_>>())?;
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
    let h = home(&args)?;
    if args[0] == "--self-test" {
        return self_test(&h, &args);
    }
    if args[0] == "--ui-smoke" {
        ensure!(
            h.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with("test-")),
            "UI screenshots require an isolated test-* profile"
        );
    }
    let s = Store::open(&h)?;
    let h = s.home.clone();
    // Stopping a process must remain possible even when its configuration is damaged.
    if let Ok(config) = s.get::<Config>("config") {
        i18n::set(&config.language);
    }
    match args.first().map(String::as_str).unwrap_or("--settings") {
        "--init" => {
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
        "--layout" => emit(&args, &native::layout()?),
        "--pins" => emit(&args, &native::pin_identities()?),
        "--catalog" => {
            let paths: Vec<String> =
                serde_json::from_slice(&std::fs::read(args.get(1).context("Нет списка путей")?)?)?;
            let mut apps = Vec::new();
            for path in paths {
                let mut a = native::read_link(Path::new(&path))?;
                a.category = Path::new(&path)
                    .strip_prefix(native::desktop()?)
                    .ok()
                    .and_then(|p| p.components().next())
                    .map(|p| p.as_os_str().to_string_lossy().into_owned())
                    .unwrap_or_default();
                apps.push(a);
            }
            emit(&args, &apps)
        }
        "--apply-initial" => {
            ensure!(
                !background_running(&h)?,
                "При работающем фоне используйте окно настроек"
            );
            ensure!(
                !s.get::<State>("state")?.initialized,
                "Первичное размещение уже выполнено"
            );
            emit(&args, &engine::apply(&s, true)?)
        }
        "--apply" => {
            ensure!(
                !background_running(&h)?,
                "При работающем фоне используйте окно настроек"
            );
            emit(&args, &engine::apply(&s, false)?)
        }
        "--rollback" => {
            ensure!(
                !background_running(&h)?,
                "При работающем фоне используйте окно настроек"
            );
            let _lock = native::exclusive("changes", &h)?;
            emit(&args, &engine::rollback(&s)?)
        }
        "--launch" => {
            let id = args.get(1).context("Нет идентификатора ярлыка")?;
            let app = s.app(id)?;
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
                model::norm(&actual.target)
                    != model::norm(&std::env::current_exe()?.to_string_lossy())
                    || actual.arguments.starts_with("--probe "),
                "Обнаружен цикл запуска"
            );
            native::open_file(Path::new(&app.source))?;
            if s.get::<Config>("config")?.collect {
                s.observe(id, 0, false, true)?;
            }
            Ok(())
        }
        "--validate-launch" => {
            let id = args.get(1).context("Нет id")?;
            let a = s.app(id)?;
            let actual = native::read_link(Path::new(&a.source))?;
            ensure!(
                actual.valid && actual.identity() == a.identity(),
                "Цель запуска изменилась"
            );
            emit(
                &args,
                &serde_json::json!({"original":a.source,"target":actual.target,"arguments":actual.arguments,"cwd":actual.cwd,"valid":true,"launched":false}),
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
            let _input_hooks = activity::Hooks::install()?;
            s.set(
                "input_filter",
                &"non-injected keyboard/buttons/wheel; fresh foreground interaction",
            )?;
            s.set("shutdown", &false)?;
            s.set("heartbeat", &0i64)?;
            s.set("background_pid", &std::process::id())?;
            s.set("background_version", &env!("CARGO_PKG_VERSION"))?;
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
        "--stop" => {
            s.set("shutdown", &true)?;
            tray::request_close(&h)
        }
        "--command" => {
            ensure!(
                background_running(&h)?,
                "Background process is not running. Open Settings first."
            );
            let action = args.get(1).context("Нет команды")?;
            s.command(action)?;
            Ok(())
        }
        "--disable" => {
            ensure!(
                !background_running(&h)?,
                "Сначала завершите фоновое приложение"
            );
            let mut config: Config = s.get("config")?;
            config.automatic = false;
            config.collect = false;
            config.autostart = false;
            s.set("config", &config)?;
            native::autostart(&std::env::current_exe()?, &h, false)?;
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
                        return Ok(());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                anyhow::bail!("Settings is already opening. Try again in a moment.");
            };
            s.set("settings_pid", &std::process::id())?;
            let result = ui::run(h.clone(), None);
            let _ = s.set("settings_pid", &0u32);
            result
        }
        _ => anyhow::bail!("Неизвестная команда"),
    }
}
fn self_test(home: &Path, args: &[String]) -> Result<()> {
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
    ensure!(s.apps()?.is_empty(), "Каталог проверки не пуст");
    let desktop = home.join("Desktop");
    std::fs::create_dir_all(&desktop)?;
    s.set("test_desktop", &desktop.to_string_lossy())?;
    let original = home.join("original.lnk");
    let marker = home.join("probe.json");
    let exe = std::env::current_exe()?;
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
        &serde_json::json!({"passed":true,"checks":["native shortcut creation","target and launch identity","transactional apply","rollback original bytes","Shell launch of inert fixture","launch without a catalog entry","byte-identical protocol shortcuts","manual edit ownership release","interrupted transaction blocks new changes and rolls back","manual rename pins position","manual deletion prevents return","rollback refuses changed files","clean first-run install and background lifecycle","collection disabled and tracking-only desktop unchanged","passive learning blocks placement","seven qualified sessions unlock initial placement"],"originals_unchanged":true}),
    )
}
fn main() {
    if let Err(error) = run() {
        let msg = i18n::runtime(&format!("{error:#}"));
        eprintln!("{msg}");
        let raw_args: Vec<String> = std::env::args().skip(1).collect();
        let parsed = cli::parse(&raw_args).ok();
        if let Some(args) = &parsed
            && !matches!(args[0].as_str(), "--help" | "--version" | "--probe")
            && let Ok(h) = home(args)
        {
            let _ = std::fs::create_dir_all(&h);
            let _ = std::fs::write(h.join("last-error.txt"), &msg);
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
