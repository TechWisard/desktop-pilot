use crate::{learning::Progress, model::*, native, store::Store};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::os::windows::fs::MetadataExt;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize, Deserialize)]
pub struct Change {
    pub kind: String,
    pub file: Owned,
    pub backup: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: String,
    pub before: State,
    pub after: State,
    pub changes: Vec<Change>,
    #[serde(default)]
    pub before_layout: Option<native::Layout>,
}
#[derive(Serialize)]
pub struct Preview {
    pub selected: Vec<String>,
    pub ranking: Vec<Ranked>,
    pub added: Vec<String>,
    pub removed: Vec<String>,
}
pub fn root(store: &Store) -> Result<PathBuf> {
    let override_root: String = store.get("test_desktop")?;
    if override_root.is_empty() {
        native::desktop()
    } else {
        Ok(override_root.into())
    }
}
fn real_desktop(store: &Store) -> Result<bool> {
    Ok(store.get::<String>("test_desktop")?.is_empty())
}
fn safe_file(path: &Path, root: &Path) -> Result<()> {
    ensure!(
        path.parent()
            .is_some_and(|p| norm(&p.to_string_lossy()) == norm(&root.to_string_lossy())),
        "Отказ: путь вне управляемого рабочего стола"
    );
    ensure!(
        path.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("lnk")),
        "Управлять можно только файлами .lnk"
    );
    if let Ok(m) = path.symlink_metadata() {
        ensure!(
            m.file_attributes() & 0x400 == 0 && m.is_file(),
            "Отказ: путь является ссылкой файловой системы или каталогом"
        );
    }
    Ok(())
}
fn fingerprint(path: &Path) -> Result<String> {
    Ok(hash(&std::fs::read(path)?))
}
fn copy_new(source: &Path, destination: &Path) -> Result<()> {
    let bytes = std::fs::read(source)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}
fn wait_removed(paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    for _ in 0..30 {
        let layout = native::layout()?;
        if !layout
            .items
            .iter()
            .any(|i| paths.iter().any(|p| norm(p) == norm(&i.path)))
        {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    bail!("Проводник ещё не завершил обновление списка значков")
}

pub fn preview(store: &Store, initial: bool) -> Result<Preview> {
    let cfg: Config = store.get("config")?;
    cfg.validate()?;
    let apps = store.apps()?;
    let state: State = store.get("state")?;
    let ranking = rank(&apps, &store.days()?, &cfg, now());
    let selected = select(&apps, &ranking, &state, &cfg, initial);
    ensure!(
        selected.len() <= cfg.slots,
        "Ручных закреплений больше доступных мест. Увеличьте число мест или снимите закрепления."
    );
    let added = selected
        .iter()
        .filter(|id| !state.owned.iter().any(|o| o.app == **id))
        .cloned()
        .collect();
    let removed = state
        .owned
        .iter()
        .filter(|o| !selected.contains(&o.app))
        .map(|o| o.app.clone())
        .collect();
    Ok(Preview {
        selected,
        ranking,
        added,
        removed,
    })
}

pub fn refresh_apps(store: &Store) -> Result<()> {
    let real = real_desktop(store)?;
    let config: Config = store.get("config")?;
    let steam = if real {
        native::steam_locations()
    } else {
        HashMap::new()
    };
    let pins = if real && config.respect_taskbar_pins {
        native::pin_identities()?
    } else {
        vec![]
    };
    for mut app in store.apps()? {
        app.valid = native::read_link(Path::new(&app.source))
            .is_ok_and(|actual| actual.valid && actual.identity() == app.identity());
        if real {
            app.pinned = native::is_pinned(&app, &pins);
            if let Some(id) = app.target.strip_prefix("steam://rungameid/") {
                app.observed_roots = steam.get(id).cloned().into_iter().collect();
            }
        }
        store.put_app(&app)?;
    }
    // Scan only launcher-bearing folders. Source trees, node_modules and archives are excluded.
    fn visit(path: &Path, depth: usize, out: &mut Vec<PathBuf>) -> Result<()> {
        if depth > 3 {
            return Ok(());
        }
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let p = entry.path();
            let m = entry.metadata()?;
            if m.file_attributes() & 0x400 != 0 {
                continue;
            }
            if m.is_dir() {
                let n = entry.file_name().to_string_lossy().to_lowercase();
                if ["служебное", "node_modules", "архив ярлыков", ".git", "venv"]
                    .contains(&n.as_str())
                {
                    continue;
                }
                visit(&p, depth + 1, out)?;
            } else if p
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("lnk") || e.eq_ignore_ascii_case("url"))
            {
                out.push(p);
                ensure!(out.len() < 3000, "Слишком большой каталог ярлыков");
            }
        }
        Ok(())
    }
    let desktop = root(store)?;
    let mut paths = Vec::new();
    visit(&desktop, 0, &mut paths)?;
    let known = store.apps()?;
    let state: State = store.get("state")?;
    for path in paths {
        if state
            .owned
            .iter()
            .any(|o| norm(&o.path) == norm(&path.to_string_lossy()))
        {
            continue;
        }
        if let Ok(mut app) = native::read_link(&path) {
            if norm(&app.target) == norm(&std::env::current_exe()?.to_string_lossy()) {
                continue;
            }
            if let Some(old) = known.iter().find(|a| a.id == app.id) {
                if old.valid {
                    continue;
                }
                app.policy = old.policy.clone();
                app.seed = old.seed;
                app.seed_at = old.seed_at;
                app.observed_roots = old.observed_roots.clone();
                app.note = old.note.clone();
            }
            app.category = path
                .strip_prefix(&desktop)
                .ok()
                .and_then(|p| p.components().next())
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .unwrap_or_default();
            app.pinned = real && native::is_pinned(&app, &pins);
            if let Some(id) = app.target.strip_prefix("steam://rungameid/") {
                app.observed_roots = steam.get(id).cloned().into_iter().collect();
            }
            if !Path::new(&app.target).is_file() && !app.target.contains("://") {
                app.note = "Специальный запуск: включение только вручную".into();
                app.policy = Policy::Never;
            }
            store.put_app(&app)?;
        }
    }
    Ok(())
}

pub fn reconcile_manual(store: &Store) -> Result<()> {
    let mut state: State = store.get("state")?;
    if state.owned.is_empty() {
        return Ok(());
    }
    let desktop = root(store)?;
    let snap = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    if let Some(l) = &snap
        && l.topology != state.topology
    {
        bail!(
            "Изменилась геометрия экрана. Примите текущую геометрию в настройках; существующие значки не перемещались."
        );
    }
    let mut kept = Vec::new();
    for mut o in state.owned.clone() {
        safe_file(Path::new(&o.path), &desktop)?;
        if !Path::new(&o.path).exists() {
            let mut renamed = None;
            for entry in std::fs::read_dir(&desktop)? {
                let p = entry?.path();
                if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("lnk"))
                    && fingerprint(&p).is_ok_and(|h| h == o.sha)
                {
                    renamed = Some(p);
                    break;
                }
            }
            if let Some(p) = renamed {
                o.path = p.to_string_lossy().into_owned();
                o.manual = true;
                let mut a = store.app(&o.app)?;
                a.policy = Policy::Always;
                store.put_app(&a)?;
                store.event("manual", &format!("Переименование принято: {}", a.name))?;
            } else {
                store.policy(&o.app, Policy::Never)?;
                store.event(
                    "manual",
                    &format!(
                        "Ручное удаление/перенос: {} исключён из возврата",
                        store.app(&o.app)?.name
                    ),
                )?;
                continue;
            }
        }
        if fingerprint(Path::new(&o.path))? != o.sha {
            store.policy(&o.app, Policy::Never)?;
            store.event(
                "manual",
                "Изменённый пользователем ярлык оставлен без управления",
            )?;
            continue;
        }
        if let Some(l) = &snap
            && let Some(item) = l.items.iter().find(|i| norm(&i.path) == norm(&o.path))
            && item.position != o.position
        {
            o.position = item.position.clone();
            o.manual = true;
            store.policy(&o.app, Policy::Always)?;
            store.event(
                "manual",
                &format!("Ручное положение закреплено: {}", store.app(&o.app)?.name),
            )?;
        }
        kept.push(o);
    }
    state.owned = kept;
    store.set("state", &state)?;
    Ok(())
}

pub(crate) fn reserve_positions(
    layout: &native::Layout,
    cfg: &Config,
    state: &State,
    removed: &[String],
    needed: usize,
) -> Result<Vec<Point>> {
    if needed == 0 {
        return Ok(vec![]);
    }
    let base = layout
        .items
        .iter()
        .find(|i| Path::new(&i.path).is_dir())
        .or_else(|| layout.items.first());
    let sx = layout.spacing.x;
    let sy = layout.spacing.y;
    ensure!(
        sx > 0 && sy > 0 && layout.icon_size <= sx && layout.icon_size < sy,
        "Icon size does not fit the desktop grid"
    );
    let bx = base
        .map(|b| b.position.x.rem_euclid(sx))
        .unwrap_or((sx - layout.icon_size) / 2);
    let by = base.map(|b| b.position.y.rem_euclid(sy)).unwrap_or(2);
    let start_col = -(-layout.work[0]).div_euclid(sx);
    let start_row = -(-layout.work[1]).div_euclid(sy);
    let retiring: HashSet<_> = state
        .owned
        .iter()
        .filter(|o| removed.contains(&o.app))
        .map(|o| norm(&o.path))
        .collect();
    let mut result = Vec::new();
    for index in 0..cfg.slots as i32 {
        let point = Point {
            x: bx + (start_col + cfg.zone_col + index % cfg.zone_columns) * sx,
            y: by + (start_row + cfg.zone_row + index / cfg.zone_columns) * sy,
        };
        // Reserve the entire grid cell, including its caption. Do not move other items.
        if point.x - bx < layout.work[0]
            || point.y - by < layout.work[1]
            || point.x - bx + sx > layout.work[2]
            || point.y - by + sy > layout.work[3]
        {
            continue;
        }
        if layout.items.iter().any(|i| {
            !retiring.contains(&norm(&i.path))
                && (i.position.x - point.x).abs() < sx - 2
                && (i.position.y - point.y).abs() < sy - 2
        }) {
            continue;
        }
        result.push(point);
        if result.len() == needed {
            break;
        }
    }
    ensure!(
        result.len() >= needed,
        "В выбранной области не хватает свободных мест. Существующие значки не перемещены."
    );
    Ok(result)
}

pub fn apply(store: &Store, initial: bool) -> Result<String> {
    let _lock = native::exclusive("changes", &store.home)?;
    ensure!(
        store
            .get::<Progress>("learning")?
            .ready(&store.get::<Config>("config")?),
        "Passive observation is still in progress. No shortcuts were changed."
    );
    let unfinished: i64 = store.db.query_row(
        "SELECT COUNT(*) FROM transactions WHERE status='prepared'",
        [],
        |r| r.get(0),
    )?;
    ensure!(
        unfinished == 0,
        "Есть незавершённая операция. Сначала нажмите «Откатить»."
    );
    refresh_apps(store)?;
    reconcile_manual(store)?;
    let cfg: Config = store.get("config")?;
    let before: State = store.get("state")?;
    let choice = preview(store, initial)?;
    let desktop = root(store)?;
    let snap = if real_desktop(store)? {
        Some(native::layout()?)
    } else {
        None
    };
    if let Some(l) = &snap {
        ensure!(
            l.flags & 1 == 0,
            "Автоупорядочивание Проводника включено. Изменения остановлены."
        );
        ensure!(
            before.topology.is_empty() || l.topology == before.topology,
            "Изменилась геометрия экрана. Нажмите «Принять текущую геометрию»."
        );
    }
    let mut after = before.clone();
    after.last_review = now();
    after.initialized = true;
    if let Some(l) = &snap {
        after.topology = l.topology.clone();
    }
    if choice.added.is_empty() && choice.removed.is_empty() {
        store.set("state", &after)?;
        return Ok("Проверено: текущий состав сохранён".into());
    }
    let points = if let Some(l) = &snap {
        reserve_positions(l, &cfg, &before, &choice.removed, choice.added.len())?
    } else {
        (0..choice.added.len())
            .map(|i| Point {
                x: i as i32 * 76,
                y: 2,
            })
            .collect()
    };
    let id = format!(
        "{}-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S-%f"),
        std::process::id()
    );
    let directory = store.home.join("history").join(&id);
    std::fs::create_dir_all(&directory)?;
    let mut tx = Transaction {
        id: id.clone(),
        before: before.clone(),
        after: State::default(),
        changes: vec![],
        before_layout: snap.clone(),
    };
    for owned in &before.owned {
        if choice.removed.contains(&owned.app) {
            let p = Path::new(&owned.path);
            safe_file(p, &desktop)?;
            ensure!(
                fingerprint(p)? == owned.sha,
                "Ярлык изменился: удаление отменено"
            );
            let backup = directory.join(format!("before-{}.lnk", owned.app));
            copy_new(p, &backup)?;
            tx.changes.push(Change {
                kind: "remove".into(),
                file: owned.clone(),
                backup: backup.to_string_lossy().into_owned(),
            });
        }
    }
    after.owned.retain(|o| !choice.removed.contains(&o.app));
    for (id, point) in choice.added.iter().zip(points) {
        let app = store.app(id)?;
        ensure!(
            Path::new(&app.source).is_file(),
            "Исходный ярлык недоступен"
        );
        let mut name = app
            .name
            .chars()
            .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
            .collect::<String>();
        if name.chars().count() > 24 {
            name = name.chars().take(21).collect::<String>() + "…";
        }
        let mut file = desktop.join(format!("{}.lnk", name.trim().trim_end_matches('.')));
        if file.exists() {
            file = desktop.join(format!(
                "{} · {}.lnk",
                name.chars().take(18).collect::<String>(),
                if cfg.language == "ru" {
                    "быстрый"
                } else {
                    "quick"
                }
            ));
        }
        safe_file(&file, &desktop)?;
        ensure!(
            !file.exists(),
            "Место занято одноимённым ярлыком: {}",
            file.display()
        );
        let stage = directory.join(format!("new-{id}.lnk"));
        let args = format!("--launch {id} --home \"{}\"", store.home.display());
        let exe = std::env::current_exe()?;
        let icon = if app.icon.is_empty() {
            app.target.as_str()
        } else {
            app.icon.as_str()
        };
        native::make_link(
            &stage,
            &exe,
            &args,
            icon,
            app.icon_index,
            &format!(
                "Desktop Pilot: {}. {}",
                app.name,
                if cfg.language == "ru" {
                    "Исходный ярлык сохранён."
                } else {
                    "Original shortcut preserved."
                }
            ),
        )?;
        let owned = Owned {
            app: id.clone(),
            path: file.to_string_lossy().into_owned(),
            sha: fingerprint(&stage)?,
            position: point,
            manual: app.policy == Policy::Always,
            since: now(),
        };
        tx.changes.push(Change {
            kind: "add".into(),
            file: owned.clone(),
            backup: stage.to_string_lossy().into_owned(),
        });
        after.owned.push(owned);
    }
    after.last_change = now();
    tx.after = after;
    store.db.execute(
        "INSERT INTO transactions VALUES(?1,?2,'prepared',?3)",
        params![tx.id, now(), serde_json::to_string(&tx)?],
    )?;
    let operation = (|| -> Result<()> {
        for c in &tx.changes {
            let file = Path::new(&c.file.path);
            safe_file(file, &desktop)?;
            if c.kind == "remove" {
                ensure!(
                    fingerprint(file)? == c.file.sha,
                    "Ярлык изменился во время операции"
                );
                std::fs::remove_file(file)?;
            } else {
                copy_new(Path::new(&c.backup), file)?;
                ensure!(
                    fingerprint(file)? == c.file.sha,
                    "Запись ярлыка не подтверждена"
                );
            }
        }
        if let Some(layout) = &snap {
            native::refresh(&desktop);
            wait_removed(
                &tx.changes
                    .iter()
                    .filter(|c| c.kind == "remove")
                    .map(|c| c.file.path.clone())
                    .collect::<Vec<_>>(),
            )?;
            let mut positions: Vec<_> = layout
                .items
                .iter()
                .filter(|i| {
                    !tx.changes
                        .iter()
                        .any(|c| c.kind == "remove" && norm(&c.file.path) == norm(&i.path))
                })
                .map(|i| (i.path.clone(), i.position.clone()))
                .collect();
            positions.extend(
                tx.changes
                    .iter()
                    .filter(|c| c.kind == "add")
                    .map(|c| (c.file.path.clone(), c.file.position.clone())),
            );
            let mut last = None;
            for _ in 0..20 {
                match native::position_many(&positions) {
                    Ok(()) => {
                        last = None;
                        break;
                    }
                    Err(e) => {
                        last = Some(e);
                        std::thread::sleep(Duration::from_millis(100));
                    }
                }
            }
            if let Some(e) = last {
                return Err(e);
            }
        }
        let dbtx = store.db.unchecked_transaction()?;
        store.set("state", &tx.after)?;
        store.db.execute(
            "UPDATE transactions SET status='committed' WHERE id=?1",
            [&tx.id],
        )?;
        dbtx.commit()?;
        Ok(())
    })();
    if let Err(e) = operation {
        let undo = rollback(store);
        bail!(
            "Изменение прервано: {e:#}. Автоматический откат: {}",
            undo.unwrap_or_else(|e| format!("{e:#}"))
        );
    }
    let message = format!(
        "Добавлено: {}; убрано быстрых копий: {}. Исходные ярлыки сохранены.",
        choice.added.len(),
        choice.removed.len()
    );
    store.event("layout", &message)?;
    Ok(message)
}

pub fn suggest_zone(layout: &native::Layout, slots: usize) -> Result<(i32, i32, i32)> {
    ensure!(
        layout.spacing.x > 0 && layout.spacing.y > 0,
        "Invalid desktop grid"
    );
    let columns = ((layout.work[2] - layout.work[0]) / layout.spacing.x).clamp(0, 40);
    let rows = ((layout.work[3] - layout.work[1]) / layout.spacing.y).clamp(0, 40);
    for width in [2, 1, 4] {
        for row in 0..rows {
            for col in 0..columns {
                let cfg = Config {
                    slots,
                    zone_col: col,
                    zone_row: row,
                    zone_columns: width,
                    ..Default::default()
                };
                if reserve_positions(layout, &cfg, &State::default(), &[], slots).is_ok() {
                    return Ok((col, row, width));
                }
            }
        }
    }
    bail!("No free area for the selected number of shortcuts. Reduce the number of slots.")
}

pub fn setup_complete(store: &Store) -> Result<bool> {
    Ok(store.get::<bool>("setup_complete")? || store.get::<State>("state")?.initialized)
}

pub fn rollback(store: &Store) -> Result<String> {
    let row=store.db.query_row("SELECT id,data,status FROM transactions WHERE status IN ('prepared','committed') ORDER BY at DESC,rowid DESC LIMIT 1",[],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)));
    let (id, data, status) = match row {
        Ok(r) => r,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok("Нет изменений для отката".into()),
        Err(e) => return Err(e.into()),
    };
    let tx: Transaction = serde_json::from_str(&data)?;
    let desktop = root(store)?;
    let layout_before_undo = if real_desktop(store)? {
        if status == "prepared" {
            tx.before_layout.clone().or(Some(native::layout()?))
        } else {
            Some(native::layout()?)
        }
    } else {
        None
    };
    for c in &tx.changes {
        let p = Path::new(&c.file.path);
        safe_file(p, &desktop)?;
        ensure!(
            fingerprint(Path::new(&c.backup))? == c.file.sha,
            "Повреждена резервная копия"
        );
        if p.exists() {
            ensure!(
                fingerprint(p)? == c.file.sha,
                "Пользователь изменил файл; откат остановлен без перезаписи: {}",
                p.display()
            );
        }
    }
    for c in tx.changes.iter().rev() {
        let p = Path::new(&c.file.path);
        if c.kind == "add" {
            if p.exists() {
                std::fs::remove_file(p)?;
            }
        } else if !p.exists() {
            copy_new(Path::new(&c.backup), p)?;
        }
    }
    if let Some(layout) = layout_before_undo {
        native::refresh(&desktop);
        wait_removed(
            &tx.changes
                .iter()
                .filter(|c| c.kind == "add")
                .map(|c| c.file.path.clone())
                .collect::<Vec<_>>(),
        )?;
        let mut positions: Vec<_> = layout
            .items
            .into_iter()
            .filter(|i| {
                !tx.changes
                    .iter()
                    .any(|c| c.kind == "add" && norm(&c.file.path) == norm(&i.path))
            })
            .map(|i| (i.path, i.position))
            .collect();
        for o in &tx.before.owned {
            if Path::new(&o.path).exists()
                && !positions.iter().any(|(p, _)| norm(p) == norm(&o.path))
            {
                positions.push((o.path.clone(), o.position.clone()));
            }
        }
        let mut last = None;
        for _ in 0..20 {
            match native::position_many(&positions) {
                Ok(()) => {
                    last = None;
                    break;
                }
                Err(e) => {
                    last = Some(e);
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        }
        if let Some(e) = last {
            return Err(e);
        }
    }
    let dbtx = store.db.unchecked_transaction()?;
    store.set("state", &tx.before)?;
    let mut cfg: Config = store.get("config")?;
    cfg.automatic = false;
    store.set("config", &cfg)?;
    store.db.execute(
        "UPDATE transactions SET status='rolled_back' WHERE id=?1",
        [&id],
    )?;
    dbtx.commit()?;
    store.event(
        "rollback",
        &format!("Откат {id}; автоматика приостановлена"),
    )?;
    Ok("Предыдущее размещение восстановлено. Автоматика приостановлена.".into())
}

pub fn command(store: &Store, action: &str) -> Result<String> {
    if let Some(rest) = action.strip_prefix("policy:") {
        let (id, mode) = rest.split_once(':').context("Некорректная команда")?;
        let policy = match mode {
            "Auto" => Policy::Auto,
            "Always" => Policy::Always,
            "Never" => Policy::Never,
            _ => bail!("Неизвестное правило"),
        };
        let mut s: State = store.get("state")?;
        for o in &mut s.owned {
            if o.app == id {
                o.manual = policy == Policy::Always;
            }
        }
        store.set("state", &s)?;
        store.policy(id, policy)?;
        return Ok("Правило сохранено. Нажмите «Применить сейчас» для изменения состава.".into());
    }
    match action {
        "apply" => apply(store, !store.get::<State>("state")?.initialized),
        "rollback" => {
            let _lock = native::exclusive("changes", &store.home)?;
            rollback(store)
        }
        "accept_geometry" => {
            let mut s: State = store.get("state")?;
            let l = native::layout()?;
            s.topology = l.topology;
            for o in &mut s.owned {
                if let Some(i) = l.items.iter().find(|i| norm(&i.path) == norm(&o.path)) {
                    o.position = i.position.clone();
                }
            }
            store.set("state", &s)?;
            Ok("Текущая геометрия принята; существующие позиции сохранены".into())
        }
        "rescan" => {
            refresh_apps(store)?;
            Ok("Список ярлыков и закрепления обновлены".into())
        }
        "clear_stats" => {
            store.db.execute("DELETE FROM days", [])?;
            for mut a in store.apps()? {
                a.seed = 0.0;
                a.seed_at = 0;
                store.put_app(&a)?;
            }
            let completed = store.get::<Progress>("learning")?.completed_at;
            store.set(
                "learning",
                &Progress {
                    started_at: now(),
                    completed_at: completed,
                    ..Default::default()
                },
            )?;
            Ok("История использования очищена; ярлыки и правила сохранены".into())
        }
        _ => bail!("Неизвестная команда"),
    }
}

pub fn observe_match(apps: &[App], process: &str) -> Option<String> {
    let process = norm(process);
    let matching: Vec<_> = apps
        .iter()
        .filter(|a| {
            a.valid
                && (norm(&a.target) == process
                    || a.observed_roots.iter().any(|root| {
                        let root = norm(root).trim_end_matches('\\').to_owned();
                        root.len() > 5 && process.starts_with(&(root + "\\"))
                    }))
        })
        .collect();
    let identities: HashSet<_> = matching.iter().map(|a| a.identity()).collect();
    if identities.len() == 1 {
        matching.first().map(|a| a.id.clone())
    } else {
        None
    }
}

fn observe_app_id(apps: &[App], id: &str) -> Option<String> {
    if id.is_empty() {
        return None;
    }
    let ids: HashSet<_> = apps
        .iter()
        .filter(|a| {
            a.valid
                && a.arguments
                    .to_lowercase()
                    .split("shell:appsfolder\\")
                    .nth(1)
                    .is_some_and(|suffix| suffix.trim_matches(['"', ' ']).eq_ignore_ascii_case(id))
        })
        .map(|a| a.id.clone())
        .collect();
    if ids.len() == 1 {
        ids.into_iter().next()
    } else {
        None
    }
}

pub fn automatic_review(store: &Store, quiet: bool) -> Result<bool> {
    let config: Config = store.get("config")?;
    let state: State = store.get("state")?;
    let mut progress: Progress = store.get("learning")?;
    progress.finish_if_ready(&config, now());
    store.set("learning", &progress)?;
    if config.automatic
        && progress.ready(&config)
        && (!state.initialized || now() >= state.last_review + config.interval_days * 86400)
        && quiet
        && (state.initialized || !preview(store, true)?.selected.is_empty())
    {
        apply(store, !state.initialized)?;
        return Ok(true);
    }
    Ok(false)
}
pub fn monitor(home: PathBuf) -> Result<()> {
    let _com = native::Com::new()?;
    let store = Store::open(&home)?;
    let mut progress: Progress = store.get("learning")?;
    if progress.started_at == 0 && store.get::<State>("state")?.initialized {
        progress.completed_at = now();
        progress.started_at = now();
        store.set("learning", &progress)?;
    }
    let mut apps = store.apps()?;
    let mut last_id = String::new();
    let mut focus = crate::activity::FocusEvidence::default();
    let mut seconds = 0;
    let mut session_times: HashMap<String, i64> = HashMap::new();
    let mut last_tick = Instant::now();
    let mut maintenance = Instant::now() - Duration::from_secs(120);
    let mut scanned = Instant::now() - Duration::from_secs(21600);
    let mut last_autostart = None;
    loop {
        if store.get::<bool>("shutdown")? {
            store.set("shutdown", &false)?;
            return Ok(());
        }
        for (id, action) in store.pending()? {
            let r = command(&store, &action);
            store.finish(id, &r)?;
            apps = store.apps()?;
        }
        let config: Config = store.get("config")?;
        config.validate()?;
        crate::activity::set_enabled(config.collect);
        if last_autostart != Some(config.autostart) {
            if real_desktop(&store)? {
                native::autostart(&std::env::current_exe()?, config.autostart)?;
            }
            last_autostart = Some(config.autostart);
        }
        let elapsed = last_tick.elapsed().as_secs_f64();
        last_tick = Instant::now();
        if config.collect && elapsed < 5.0 {
            if let Ok(foreground) = native::foreground() {
                let input = crate::activity::snapshot();
                let tick = crate::activity::tick();
                let engaged = focus.active(foreground.window, input, tick, config.idle_seconds);
                if foreground.idle < config.idle_seconds && input.recent(tick, config.idle_seconds)
                {
                    if config.learning_enabled {
                        let mut progress: Progress = store.get("learning")?;
                        if progress.completed_at == 0 {
                            progress.observe(
                                now(),
                                &chrono::Local::now().date_naive().to_string(),
                                elapsed.round() as u32,
                            );
                            store.set("learning", &progress)?;
                        }
                    }
                    if engaged
                        && let Some(id) = observe_app_id(&apps, &foreground.app_id)
                            .or_else(|| observe_match(&apps, &foreground.process))
                    {
                        if id != last_id {
                            last_id = id.clone();
                            seconds = 0;
                        } else {
                            seconds += 2;
                        }
                        if seconds >= 10 {
                            let session = seconds == 10
                                && now() - session_times.get(&id).copied().unwrap_or(0) >= 1200;
                            let count = if seconds == 10 { 10 } else { 2 };
                            store.observe(&id, count, session, false)?;
                            if session {
                                session_times.insert(id, now());
                            }
                        }
                    } else {
                        last_id.clear();
                        seconds = 0;
                    }
                } else {
                    last_id.clear();
                    seconds = 0;
                }
            } else {
                last_id.clear();
                seconds = 0;
            }
        } else {
            last_id.clear();
            seconds = 0;
            focus = crate::activity::FocusEvidence::default();
        }
        if maintenance.elapsed() >= Duration::from_secs(60) {
            maintenance = Instant::now();
            store.set("heartbeat", &now())?;
            let result = (|| -> Result<()> {
                if scanned.elapsed() >= Duration::from_secs(21600) {
                    refresh_apps(&store)?;
                    apps = store.apps()?;
                    scanned = Instant::now();
                    store.housekeeping()?;
                }
                reconcile_manual(&store)?;
                automatic_review(&store, native::quiet_for_changes())?;
                Ok(())
            })();
            let status = match result {
                Ok(()) => "Фоновый учёт работает".into(),
                Err(e) => format!("Изменения приостановлены: {e:#}"),
            };
            store.set("status", &status)?;
        }
        std::thread::sleep(Duration::from_secs(2));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_profiles_not_merged() {
        let mut a = App {
            target: "C:\\browser.exe".into(),
            arguments: "--profile=A".into(),
            valid: true,
            ..Default::default()
        };
        a.assign_id();
        let mut b = a.clone();
        b.arguments = "--profile=B".into();
        b.assign_id();
        assert!(observe_match(&[a, b], "c:\\browser.exe").is_none());
    }
    #[test]
    fn unknown_files_do_not_fit_zone() {
        let l = native::Layout {
            items: vec![native::Item {
                path: "existing".into(),
                name: "existing".into(),
                position: Point { x: 22, y: 2 },
            }],
            spacing: Point { x: 76, y: 84 },
            origin: Point::default(),
            work: [0, 0, 1920, 1032],
            flags: 4,
            topology: String::new(),
            icon_size: 32,
        };
        let c = Config {
            slots: 1,
            zone_col: 0,
            zone_row: 0,
            ..Default::default()
        };
        assert!(reserve_positions(&l, &c, &State::default(), &[], 1).is_err());
    }
    #[test]
    fn traversal_is_rejected() {
        assert!(safe_file(Path::new(r"C:\elsewhere\x.lnk"), Path::new(r"C:\Desktop")).is_err());
    }
    #[test]
    fn steam_process_is_matched_only_inside_its_game_folder() {
        let a = App {
            id: "game".into(),
            target: "steam://rungameid/1".into(),
            valid: true,
            observed_roots: vec![r"D:\Steam\common\Game".into()],
            ..Default::default()
        };
        assert_eq!(
            observe_match(
                std::slice::from_ref(&a),
                r"d:\steam\common\game\bin\game.exe"
            ),
            Some("game".into())
        );
        assert_eq!(observe_match(&[a], r"D:\Steam\common\Game2\game.exe"), None);
    }
    #[test]
    fn store_app_uses_identity_not_explorer_process() {
        let app = App {
            id: "music".into(),
            arguments: "\"shell:AppsFolder\\Yandex.Music_123!App\"".into(),
            valid: true,
            ..Default::default()
        };
        assert_eq!(
            observe_app_id(std::slice::from_ref(&app), "yandex.music_123!app"),
            Some("music".into())
        );
        assert_eq!(observe_app_id(&[app], "other!app"), None);
    }
    #[test]
    fn display_geometry_matrix() {
        for (width, height) in [
            (1024, 768),
            (1366, 768),
            (1920, 1080),
            (2560, 1440),
            (3840, 2160),
        ] {
            for scale in [100, 125, 150, 200, 300] {
                for (left, top) in [(0, 0), (1920, 0), (3840, 1080), (-1920, -1080)] {
                    let layout = native::Layout {
                        items: vec![],
                        spacing: Point {
                            x: 76 * scale / 100,
                            y: 84 * scale / 100,
                        },
                        origin: Point { x: -left, y: -top },
                        work: [left, top, left + width, top + height - 48 * scale / 100],
                        flags: 4,
                        topology: String::new(),
                        icon_size: 32 * scale / 100,
                    };
                    let (col, row, columns) = suggest_zone(&layout, 4).unwrap();
                    let cfg = Config {
                        slots: 4,
                        zone_col: col,
                        zone_row: row,
                        zone_columns: columns,
                        ..Default::default()
                    };
                    let points =
                        reserve_positions(&layout, &cfg, &State::default(), &[], 4).unwrap();
                    assert_eq!(points.len(), 4);
                    for p in &points {
                        assert!(
                            p.x >= left && p.y >= top && p.x < left + width && p.y < top + height
                        );
                    }
                    for (i, p) in points.iter().enumerate() {
                        for q in &points[i + 1..] {
                            assert!(
                                (p.x - q.x).abs() >= layout.spacing.x
                                    || (p.y - q.y).abs() >= layout.spacing.y
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn occupied_and_too_small_desktops_are_rejected() {
        let mut layout = native::Layout {
            items: vec![],
            spacing: Point { x: 76, y: 84 },
            origin: Point::default(),
            work: [0, 0, 152, 168],
            flags: 4,
            topology: String::new(),
            icon_size: 32,
        };
        for x in [22, 98] {
            for y in [2, 86] {
                layout.items.push(native::Item {
                    name: "existing".into(),
                    path: format!("item-{x}-{y}"),
                    position: Point { x, y },
                });
            }
        }
        assert!(suggest_zone(&layout, 1).is_err());
        layout.items.clear();
        layout.work = [0, 0, 40, 40];
        assert!(suggest_zone(&layout, 1).is_err());
    }
}
