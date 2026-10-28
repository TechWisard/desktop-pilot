use crate::{
    engine,
    i18n::{self, text as t},
    learning::{Mode, Progress},
    model::*,
    native, setup,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use eframe::egui::{self, Color32, RichText};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const ACCENT: Color32 = Color32::from_rgb(86, 207, 187);
const MUTED: Color32 = Color32::from_rgb(163, 177, 193);
const WARNING: Color32 = Color32::from_rgb(246, 196, 110);
const ERROR: Color32 = Color32::from_rgb(255, 153, 153);
const SURFACE: Color32 = Color32::from_rgb(29, 38, 50);
const UI_SCALES: [f32; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];
const APPS_PER_PAGE: usize = 24;

#[derive(Default)]
struct CatalogCache {
    search: Vec<String>,
    ranking: HashMap<String, usize>,
    query: Option<(String, usize, usize)>,
    indices: Vec<usize>,
}
impl CatalogCache {
    fn rebuild(&mut self, apps: &[App], ranking: &[Ranked]) {
        self.search = apps.iter().map(search_text).collect();
        self.ranking = ranking
            .iter()
            .enumerate()
            .map(|(index, ranked)| (ranked.id.clone(), index))
            .collect();
        self.query = None;
    }
    fn update(
        &mut self,
        apps: &[App],
        ranking: &[Ranked],
        query: &str,
        filter: usize,
        sort: usize,
        recent: &HashSet<String>,
    ) {
        if self.query.as_ref().is_some_and(|previous| {
            previous.0 == query && previous.1 == filter && previous.2 == sort
        }) {
            return;
        }
        let query_lower = query.to_lowercase();
        let words: Vec<_> = query_lower.split_whitespace().collect();
        let ranked = |app: &App| self.ranking.get(&app.id).and_then(|&i| ranking.get(i));
        self.indices = apps
            .iter()
            .enumerate()
            .filter(|(index, app)| {
                words.iter().all(|word| self.search[*index].contains(word))
                    && match filter {
                        1 => ranked(app).is_some_and(|ranked| ranked.eligible),
                        2 => app.policy == Policy::Always,
                        3 => app.policy == Policy::Never,
                        4 => !app.valid,
                        5 => recent.contains(&app.id),
                        _ => true,
                    }
            })
            .map(|(index, _)| index)
            .collect();
        if sort == 0 && !words.is_empty() {
            // A profile directory or a shared executable name can make thousands
            // of shortcuts match the same query. Show an exact app name before
            // path/category-only hits without removing any existing results.
            // Cached keys avoid repeated Unicode lowercasing during sorting.
            let phrase = words.join(" ");
            self.indices.sort_by_cached_key(|&index| {
                let name = apps[index].name.to_lowercase();
                if name == phrase {
                    0u8
                } else if name.starts_with(&phrase) {
                    1
                } else if name.ends_with(&phrase) {
                    2
                } else if name.contains(&phrase) {
                    3
                } else if words.iter().all(|word| name.contains(word)) {
                    4
                } else if words.iter().any(|word| name.contains(word)) {
                    5
                } else {
                    6
                }
            });
        } else if sort == 1 || sort == 2 {
            self.indices.sort_by(|&a, &b| {
                let a = ranked(&apps[a]);
                let b = ranked(&apps[b]);
                if sort == 1 {
                    b.map_or(0.0, |ranked| ranked.score)
                        .total_cmp(&a.map_or(0.0, |ranked| ranked.score))
                } else {
                    b.map_or(0, |ranked| ranked.seconds)
                        .cmp(&a.map_or(0, |ranked| ranked.seconds))
                }
            });
        }
        self.query = Some((query.into(), filter, sort));
    }
}

#[derive(Clone, Copy, PartialEq)]
enum NoticeKind {
    Info,
    Success,
    Error,
}
struct Notice {
    text: String,
    kind: NoticeKind,
}
struct PendingCommand {
    id: i64,
    action: String,
    submitted: Instant,
}
struct ImportResult {
    // All distinct, successfully selected catalog identities, including no-ops.
    added: Vec<(String, String)>,
    failed: Vec<(PathBuf, String)>,
    created: usize,
    updated: usize,
}
fn import_summary(result: &ImportResult) -> String {
    let already = result
        .added
        .len()
        .saturating_sub(result.created + result.updated);
    let mut parts = Vec::new();
    if result.created > 0 {
        parts.push(format!("{}: {}", t("Добавлено", "Added"), result.created));
    }
    if result.updated > 0 {
        parts.push(format!("{}: {}", t("Обновлено", "Updated"), result.updated));
    }
    if already > 0 {
        parts.push(format!(
            "{}: {}",
            t("Уже в списке", "Already in list"),
            already
        ));
    }
    let ending = if result.created + result.updated == 0 {
        t("Без изменений.", "No changes made.")
    } else {
        t("Правила и история сохранены.", "Rules and history kept.")
    };
    format!("{}. {}", parts.join(" · "), ending)
}
enum JobResult {
    Setup(Config),
    Saved(Config, Option<String>),
    Queued(i64, String),
    Started,
    Prepared,
    Added(ImportResult),
    Disabled(Config),
}
struct Job {
    label: &'static str,
    started: Instant,
    receiver: Receiver<std::result::Result<JobResult, String>>,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum OperatingMode {
    Adaptive,
    Observe,
    Manual,
}
#[derive(Clone, Copy, PartialEq, Debug)]
enum Health {
    Offline,
    Starting,
    Unresponsive,
    Attention,
    Observing,
    ObservationPaused,
    Automatic,
    Suggestions,
    Manual,
}

pub struct Settings {
    store: Store,
    // config is the editable draft; saved_config is the running configuration.
    config: Config,
    saved_config: Config,
    edit_base: Config,
    apps: Vec<App>,
    state: State,
    learning: Progress,
    ranking: Vec<Ranked>,
    preview: Option<engine::Preview>,
    preview_error: String,
    can_undo: bool,
    interrupted: bool,
    tab: usize,
    settings_section: usize,
    filter: String,
    app_filter: usize,
    app_sort: usize,
    app_page: usize,
    catalog: CatalogCache,
    details_app: Option<String>,
    pending_policy: Option<(String, Policy)>,
    notice: Option<Notice>,
    notice_details: bool,
    status: String,
    catalog_warning: String,
    autostart_warning: String,
    tray_warning: String,
    settings_shortcut_warning: String,
    background_alive: bool,
    background_disabled: bool,
    heartbeat: i64,
    last: Instant,
    catalog_checkpoint: Option<(i64, i64, Instant)>,
    dirty: bool,
    onboarding: bool,
    onboarding_step: usize,
    confirm_clear: bool,
    confirm_close: bool,
    confirm_setup_close: bool,
    confirm_disable: bool,
    import_open: bool,
    import_paths: String,
    import_error: String,
    focus_import: bool,
    focus_search: bool,
    highlighted_apps: HashSet<String>,
    page_scroll_reset: bool,
    allow_close: bool,
    close_after_save: bool,
    pending_commands: Vec<PendingCommand>,
    active_commands: usize,
    job: Option<Job>,
    layout: Option<native::Layout>,
    layout_error: String,
    layout_receiver: Option<Receiver<std::result::Result<native::Layout, String>>>,
    find_after_layout: bool,
    ui_scale: f32,
    history_errors_only: bool,
    events: Vec<(i64, String, String)>,
    events_error: String,
    capture: Option<PathBuf>,
    capture_view: Option<String>,
    capture_scroll_bottom: bool,
    capture_at: Instant,
    capture_pending: bool,
}

fn shortcut_paths(value: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for line in value.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let line = line
            .strip_prefix('"')
            .and_then(|line| line.strip_suffix('"'))
            .unwrap_or(line);
        let path = PathBuf::from(line);
        ensure!(path.is_absolute(), "Use a full shortcut path: {line}");
        ensure!(
            path.extension().is_some_and(|extension| matches!(
                extension.to_str().map(str::to_ascii_lowercase).as_deref(),
                Some("lnk" | "url")
            )),
            "Choose a .lnk or .url shortcut: {line}"
        );
        if seen.insert(norm(line)) {
            paths.push(path);
        }
    }
    ensure!(
        !paths.is_empty(),
        "Choose a shortcut or paste its full path."
    );
    Ok(paths)
}

fn preserve_app_preferences(app: &mut App, existing: Option<&App>) {
    if let Some(existing) = existing {
        app.id = existing.id.clone();
        app.policy = existing.policy.clone();
        app.seed = existing.seed;
        app.seed_at = existing.seed_at;
        app.pinned = existing.pinned;
        app.observed_roots = existing.observed_roots.clone();
        if app.note.is_empty() {
            app.note = existing.note.clone();
        }
        app.category = existing.category.clone();
    } else {
        app.category = "Manual".into();
    }
}

#[cfg(test)]
fn prepare_imported_app(app: &mut App, existing: &HashMap<String, App>) -> Result<()> {
    ImportCatalog::new(existing.values().cloned()).prepare(app)
}

fn should_resume_background(disabled: bool, base: &Config, draft: &Config) -> bool {
    disabled
        && ((!base.collect && draft.collect)
            || (!base.automatic && draft.automatic)
            || (!base.autostart && draft.autostart))
}

fn when(timestamp: i64) -> String {
    if timestamp <= 0 {
        return t("ещё не выполнялось", "not yet").into();
    }
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| t("дата недоступна", "date unavailable").into())
}
fn policy_name(policy: &Policy) -> &'static str {
    match policy {
        Policy::Auto => t("Автоматический выбор", "Automatic selection"),
        Policy::Always => t("Всегда на столе", "Always on desktop"),
        Policy::Never => t("Не добавлять", "Never add"),
    }
}
fn app_filter_name(filter: usize) -> &'static str {
    match filter {
        1 => t("Подходят для размещения", "Eligible for placement"),
        2 => t("Всегда на столе", "Always on desktop"),
        3 => t("Не добавлять", "Never add"),
        4 => t("Недоступные", "Unavailable"),
        5 => t("Последний импорт", "Last import"),
        _ => t("Все приложения", "All apps"),
    }
}
fn app_sort_name(sort: usize, search_active: bool) -> &'static str {
    match sort {
        1 => t("По рейтингу", "By ranking"),
        2 => t("По времени использования", "By usage time"),
        0 if search_active => t("По соответствию", "By relevance"),
        _ => t("По алфавиту", "Alphabetically"),
    }
}
fn operating_mode(config: &Config) -> OperatingMode {
    if config.automatic {
        OperatingMode::Adaptive
    } else if config.collect {
        OperatingMode::Observe
    } else {
        OperatingMode::Manual
    }
}
fn apply_mode(config: &mut Config, mode: OperatingMode) {
    let was_manual = !config.automatic && !config.collect;
    match mode {
        OperatingMode::Adaptive => {
            config.automatic = true;
            config.collect = true;
            if was_manual {
                config.learning_enabled = true;
            }
        }
        OperatingMode::Observe => {
            config.automatic = false;
            config.collect = true;
            if was_manual {
                config.learning_enabled = true;
            }
        }
        OperatingMode::Manual => {
            config.automatic = false;
            config.collect = false;
            config.learning_enabled = false;
        }
    }
}
// A stale "running" status is not an error. Only the explicit worker
// termination marker may be shown when there is no active background process.
fn stopped_worker_error(alive: bool, disabled: bool, status: &str) -> Option<&str> {
    if alive || disabled {
        return None;
    }
    let first_line = status.lines().next()?.trim();
    (first_line.starts_with("Фоновый учёт остановлен:")
        || first_line.starts_with("Background tracking stopped:"))
    .then_some(first_line)
}
fn health(
    alive: bool,
    heartbeat: i64,
    at: i64,
    status: &str,
    config: &Config,
    progress: &Progress,
) -> Health {
    if !alive {
        return Health::Offline;
    }
    if heartbeat <= 0 {
        return Health::Starting;
    }
    // The worker normally publishes every 15 seconds. A stale heartbeat
    // is unverified in either direction: after a backward Windows clock
    // correction, a frozen worker may leave a checkpoint far in the future.
    // Keep the existing 150-second tolerance for ordinary clock drift.
    if heartbeat.abs_diff(at) > 150 {
        return Health::Unresponsive;
    }
    if status.starts_with("Изменения приостановлены")
        || status.starts_with("Changes paused")
        || status.starts_with("Фоновый учёт остановлен")
        || status.starts_with("Background tracking stopped")
    {
        return Health::Attention;
    }
    if !progress.ready(config) {
        return if config.collect {
            Health::Observing
        } else {
            Health::ObservationPaused
        };
    }
    if config.automatic {
        Health::Automatic
    } else if config.collect {
        Health::Suggestions
    } else {
        Health::Manual
    }
}
fn health_label(value: Health) -> &'static str {
    match value {
        Health::Offline => t("Фоновая работа остановлена", "Background process stopped"),
        Health::Starting => t("Запускается…", "Starting…"),
        Health::Unresponsive => t(
            "Фон давно не отвечал",
            "Background process is not responding",
        ),
        Health::Attention => t("Нужно ваше внимание", "Needs attention"),
        Health::Observing => t("Изучает ваши привычки", "Learning your routine"),
        Health::ObservationPaused => t("Наблюдение на паузе", "Observation paused"),
        Health::Automatic => t("Подстраивается автоматически", "Adapting automatically"),
        Health::Suggestions => t("Наблюдение и предложения", "Observation and suggestions"),
        Health::Manual => t("Ручное управление", "Manual control"),
    }
}
fn health_color(value: Health) -> Color32 {
    match value {
        Health::Offline | Health::Unresponsive | Health::Attention => WARNING,
        Health::ObservationPaused | Health::Starting => MUTED,
        _ => ACCENT,
    }
}
fn command_in_progress(status: &str) -> bool {
    matches!(status, "pending" | "running")
}
fn config_equal(a: &Config, b: &Config) -> bool {
    a == b
}
fn merge_config(base: &Config, draft: &Config, latest: &Config) -> Result<Config> {
    let base = serde_json::to_value(base)?;
    let draft = serde_json::to_value(draft)?;
    let mut latest = serde_json::to_value(latest)?;
    let values = latest.as_object_mut().context("Invalid settings format")?;
    for (key, value) in draft.as_object().context("Invalid settings format")? {
        if base.get(key) != Some(value) {
            ensure!(
                values.get(key) == base.get(key) || values.get(key) == Some(value),
                "These settings changed in the background. Discard your edits, then make the change again."
            );
            values.insert(key.clone(), value.clone());
        }
    }
    let merged: Config = serde_json::from_value(latest)?;
    merged.validate()?;
    Ok(merged)
}
fn validated_scale(scale: f32) -> f32 {
    if scale.is_finite() && UI_SCALES.iter().any(|v| (scale - v).abs() < 0.01) {
        scale
    } else {
        1.0
    }
}
fn requested_scale(ctx: &egui::Context, current: f32) -> Option<f32> {
    let action = ctx.input_mut(|input| {
        if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Num0) {
            Some(0)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Plus)
            || input.consume_key(egui::Modifiers::COMMAND, egui::Key::Equals)
        {
            Some(1)
        } else if input.consume_key(egui::Modifiers::COMMAND, egui::Key::Minus) {
            Some(-1)
        } else {
            None
        }
    })?;
    let index = UI_SCALES
        .iter()
        .position(|scale| (*scale - current).abs() < 0.01)
        .unwrap_or_default();
    Some(match action {
        1 => UI_SCALES[(index + 1).min(UI_SCALES.len() - 1)],
        -1 => UI_SCALES[index.saturating_sub(1)],
        _ => 1.0,
    })
}
fn search_text(app: &App) -> String {
    let category_alias = match app.category.as_str() {
        "Manual" => "Добавлено вами Added by you",
        "Desktop" => "Рабочий стол Desktop",
        "Start menu" => "Меню Пуск Start menu",
        _ => "",
    };
    format!(
        "{} {} {} {} {} {} {} {}",
        app.name,
        app.category,
        category_alias,
        app.source,
        app.target,
        app.arguments,
        app.cwd,
        app.note,
    )
    .to_lowercase()
}
fn page_controls(ui: &mut egui::Ui, page: &mut usize, count: usize) -> bool {
    let previous = *page;
    let pages = count.div_ceil(APPS_PER_PAGE).max(1);
    *page = (*page).min(pages - 1);
    if pages <= 1 {
        return *page != previous;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(*page > 0, egui::Button::new(t("Назад", "Previous")))
            .clicked()
        {
            *page -= 1;
        }
        ui.label(t("Страница", "Page"));
        let mut requested = *page + 1;
        if ui
            .add_sized(
                [76.0, 38.0],
                egui::DragValue::new(&mut requested)
                    .range(1..=pages)
                    .update_while_editing(false),
            )
            .on_hover_text(t(
                "Нажмите число и введите нужную страницу. Ctrl+Page Up / Page Down — соседние страницы.",
                "Click the number to enter a page. Ctrl+Page Up / Page Down moves between pages.",
            ))
            .changed()
        {
            *page = requested.saturating_sub(1).min(pages - 1);
        }
        ui.label(format!("/ {pages}"));
        if ui
            .add_enabled(*page + 1 < pages, egui::Button::new(t("Вперёд", "Next")))
            .clicked()
        {
            *page += 1;
        }
    });
    *page != previous
}
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(SURFACE)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().slider_width = (ui.available_width() - 120.0).clamp(80.0, 250.0);
            content(ui);
        });
    ui.add_space(8.0);
}
fn collapsing_header(ui: &egui::Ui, title: &str) -> egui::CollapsingHeader {
    // egui headers otherwise extend even when the surrounding page wraps.
    let width =
        (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(1.0);
    let text = egui::WidgetText::from(title).into_galley(
        ui,
        Some(egui::TextWrapMode::Wrap),
        width,
        egui::TextStyle::Button,
    );
    egui::CollapsingHeader::new(text).id_salt(title)
}
fn constrain_page_width(ui: &mut egui::Ui) {
    // set_max_width can expand a Ui. Keep the original viewport constraint,
    // including its scrollbar and the frame margins.
    ui.set_max_width(ui.available_width().min(1060.0));
}
fn configure_style(ctx: &egui::Context) {
    // Keyboard zoom must use the same persisted values as the visible selector.
    ctx.options_mut(|options| options.zoom_with_keyboard = false);
    // The panels use a dark palette. Selecting only the active style would
    // let the first Windows light-theme event replace it with egui defaults.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|style| {
        style.visuals = egui::Visuals::dark();
        style.visuals.selection.bg_fill = Color32::from_rgb(35, 101, 100);
        style.visuals.selection.stroke = egui::Stroke::new(1.0, ACCENT);
        style.visuals.override_text_color = Some(Color32::from_rgb(234, 239, 245));
        style.visuals.hyperlink_color = ACCENT;
        style.spacing.item_spacing = egui::vec2(12.0, 10.0);
        style.spacing.button_padding = egui::vec2(14.0, 9.0);
        style.spacing.interact_size = egui::vec2(42.0, 38.0);
        style.spacing.slider_width = 250.0;
        style.spacing.combo_width = 250.0;
        for (kind, size) in [
            (egui::TextStyle::Heading, 23.0),
            (egui::TextStyle::Body, 18.0),
            (egui::TextStyle::Button, 17.0),
            (egui::TextStyle::Small, 15.0),
            (egui::TextStyle::Monospace, 16.0),
        ] {
            style
                .text_styles
                .insert(kind, egui::FontId::proportional(size));
        }
    });
}
fn primary(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).strong())
            .fill(Color32::from_rgb(30, 100, 96))
            .min_size(egui::vec2(0.0, 42.0)),
    )
}
fn dialog(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    title: &str,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::ModalResponse<()> {
    dialog_frame(
        ctx,
        id,
        width,
        |_| {},
        |ui| {
            ui.heading(title);
            ui.add_space(8.0);
            content(ui);
        },
    )
}
fn dialog_frame<R>(
    ctx: &egui::Context,
    id: &str,
    width: f32,
    header: impl FnOnce(&mut egui::Ui) -> R,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::ModalResponse<R> {
    egui::Modal::new(egui::Id::new(id))
        .frame(egui::Frame::popup(&ctx.style()).inner_margin(18.0))
        .show(ctx, |ui| {
            ui.set_width(width.min((ctx.screen_rect().width() - 60.0).max(120.0)));
            let height = (ctx.screen_rect().height() - 72.0).max(90.0);
            // An Area remembers its previous size. Let a longer dialog grow
            // instead of making its scroll viewport inherit an old short size.
            ui.set_max_height(height);
            ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
            let start = ui.cursor().min.y;
            let result = header(ui);
            let body_height = (height - (ui.cursor().min.y - start)).max(1.0);
            egui::ScrollArea::vertical()
                .id_salt(id)
                .max_height(body_height)
                .show(ui, content);
            result
        })
}
fn number(ui: &mut egui::Ui, label: &str, value: &mut i64, min: i64, max: i64) -> bool {
    let before = *value;
    ui.label(label);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                *value > min,
                egui::Button::new("−").min_size(egui::vec2(42.0, 40.0)),
            )
            .clicked()
        {
            *value = value.saturating_sub(1).max(min);
        }
        ui.add_sized(
            [92.0, 40.0],
            egui::DragValue::new(value).range(min..=max).speed(1.0),
        )
        .on_hover_text(t(
            "Нажмите число, чтобы ввести значение с клавиатуры",
            "Click the number to type a value",
        ));
        if ui
            .add_enabled(
                *value < max,
                egui::Button::new("+").min_size(egui::vec2(42.0, 40.0)),
            )
            .clicked()
        {
            *value = value.saturating_add(1).min(max);
        }
    });
    *value != before
}

impl Settings {
    fn new(home: PathBuf) -> Result<Self> {
        let store = Store::open(&home)?;
        let onboarding = !engine::setup_complete(&store)?;
        let saved_config: Config = store.get("config")?;
        let mut config = saved_config.clone();
        if onboarding {
            if let Some(draft) = store.get::<Option<Config>>("setup_draft")?
                && draft.validate().is_ok()
            {
                config = draft;
                config.language = saved_config.language.clone();
            } else {
                config.automatic = true;
                config.collect = true;
                config.autostart = true;
            }
        }
        let onboarding_step = if onboarding {
            store.get::<usize>("setup_step")?.min(2)
        } else {
            0
        };
        i18n::set(&config.language);
        let ui_scale = validated_scale(store.get::<f32>("ui_scale")?);
        let mut app = Self {
            store,
            edit_base: saved_config.clone(),
            saved_config,
            config,
            apps: vec![],
            state: State::default(),
            learning: Progress::default(),
            ranking: vec![],
            preview: None,
            preview_error: String::new(),
            can_undo: false,
            interrupted: false,
            tab: 0,
            settings_section: 0,
            filter: String::new(),
            app_filter: 0,
            app_sort: 0,
            app_page: 0,
            catalog: CatalogCache::default(),
            details_app: None,
            pending_policy: None,
            notice: None,
            notice_details: false,
            status: String::new(),
            catalog_warning: String::new(),
            autostart_warning: String::new(),
            tray_warning: String::new(),
            settings_shortcut_warning: String::new(),
            background_alive: false,
            background_disabled: false,
            heartbeat: 0,
            last: Instant::now(),
            catalog_checkpoint: None,
            dirty: onboarding,
            onboarding,
            onboarding_step,
            confirm_clear: false,
            confirm_close: false,
            confirm_setup_close: false,
            confirm_disable: false,
            import_open: false,
            import_paths: String::new(),
            import_error: String::new(),
            focus_import: false,
            focus_search: false,
            highlighted_apps: HashSet::new(),
            page_scroll_reset: false,
            allow_close: false,
            close_after_save: false,
            pending_commands: vec![],
            active_commands: 0,
            job: None,
            layout: None,
            layout_error: String::new(),
            layout_receiver: None,
            find_after_layout: false,
            ui_scale,
            history_errors_only: false,
            events: vec![],
            events_error: String::new(),
            capture: None,
            capture_view: None,
            capture_scroll_bottom: false,
            capture_at: Instant::now() + Duration::from_secs(1),
            capture_pending: false,
        };
        app.reload()?;
        app.refresh_layout(onboarding);
        Ok(app)
    }
    fn inform(&mut self, text: impl Into<String>, kind: NoticeKind) {
        self.notice_details = false;
        self.notice = Some(Notice {
            text: text.into(),
            kind,
        });
    }
    fn fail(&mut self, error: impl std::fmt::Display) {
        self.inform(error.to_string(), NoticeKind::Error);
    }
    fn busy(&self) -> bool {
        self.job.is_some()
            || !self.pending_commands.is_empty()
            || self.active_commands > 0
            || (self.find_after_layout && self.layout_receiver.is_some())
    }
    fn modal_open(&self) -> bool {
        self.confirm_clear
            || self.confirm_close
            || self.confirm_setup_close
            || self.confirm_disable
            || self.import_open
            || self.notice_details
            || self.details_app.is_some()
    }
    fn refresh_dirty(&mut self) {
        self.dirty = self.onboarding || !config_equal(&self.config, &self.edit_base);
    }
    fn reload(&mut self) -> Result<()> {
        // External SQLite commits change data_version; local commits on this
        // Settings connection change total_changes instead. Both are needed
        // before reusing the last fully verified catalog snapshot.
        let data_version: i64 = self
            .store
            .db
            .pragma_query_value(None, "data_version", |row| row.get(0))?;
        let local_changes: i64 = self
            .store
            .db
            .query_row("SELECT total_changes()", [], |row| row.get(0))?;
        let unchanged = match &self.catalog_checkpoint {
            Some((version, changes, loaded)) => {
                *version == data_version
                    && *changes == local_changes
                    // Time-dependent ranking and expiry must still advance
                    // even in a truly idle, unchanged profile.
                    && loaded.elapsed() < Duration::from_secs(60)
            }
            None => false,
        };
        if unchanged {
            // A crashed worker may release its mutex without saving a final
            // SQLite status. Keep that safety indicator fresh every poll.
            self.background_alive = native::is_running("background", &self.store.home)?;
            return Ok(());
        }
        // A failed full reload must never authorize a later stale fast path.
        self.catalog_checkpoint = None;
        // A command result and the state it changed must come from one snapshot.
        // A worker may finish between individual SELECTs while Settings is open.
        let snapshot = self.store.db.unchecked_transaction()?;
        // Keep the Store's ID order for selection before sorting the on-screen
        // list by name. Manual anchors and equal-score tie breaks must not move.
        let mut apps = self.store.apps()?;
        let state: State = self.store.get("state")?;
        let learning = self.store.get("learning")?;
        let saved_config = self.store.get("config")?;
        let days = self.store.days()?;
        let at = now();
        let preview =
            engine::preview_loaded(&saved_config, &apps, &days, &state, !state.initialized, at);
        let ranking = match &preview {
            Ok(preview) => preview.ranking.clone(),
            Err(_) => rank(&apps, &days, &saved_config, at),
        };
        apps.sort_by_key(|a| a.name.to_lowercase());
        let status = self.store.get("status")?;
        let catalog_warning = self.store.get("catalog_warning")?;
        let autostart_warning = self.store.get("autostart_warning")?;
        let tray_warning = self.store.get("tray_warning")?;
        let settings_shortcut_warning = self.store.get("settings_shortcut_warning")?;
        let heartbeat = self.store.get("heartbeat")?;
        let background_alive = native::is_running("background", &self.store.home)?;
        let background_disabled = self.store.get("background_disabled")?;
        let (can_undo, interrupted): (bool, bool) = self.store.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM transactions WHERE status IN ('prepared','committed','rolling_back')), EXISTS(SELECT 1 FROM transactions WHERE status IN ('prepared','rolling_back'))",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        )?;
        let active_commands = self.store.db.query_row(
            "SELECT COUNT(*) FROM commands WHERE status IN ('pending','running')",
            [],
            |r| Ok(r.get::<_, i64>(0)? as usize),
        )?;
        let events = self.store.events();
        let mut completed = Vec::new();
        for pending in &self.pending_commands {
            let result = self.store.db.query_row(
                "SELECT status,message FROM commands WHERE id=?1",
                [pending.id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            );
            let (status, message) = match result {
                Ok(value) => value,
                Err(rusqlite::Error::QueryReturnedNoRows) => (
                    "error".into(),
                    "The command result is no longer available. Check the current state on Home."
                        .into(),
                ),
                Err(e) => return Err(e.into()),
            };
            if !command_in_progress(&status) {
                completed.push((pending.id, status, message));
            }
        }
        snapshot.commit()?;
        self.apps = apps;
        self.state = state;
        self.learning = learning;
        self.ranking = ranking;
        self.catalog.rebuild(&self.apps, &self.ranking);
        self.status = status;
        self.catalog_warning = catalog_warning;
        self.autostart_warning = autostart_warning;
        self.tray_warning = tray_warning;
        self.settings_shortcut_warning = settings_shortcut_warning;
        self.heartbeat = heartbeat;
        self.background_alive = background_alive;
        self.background_disabled = background_disabled;
        self.can_undo = can_undo;
        self.interrupted = interrupted;
        self.active_commands = active_commands;
        match events {
            Ok(events) => {
                self.events = events;
                self.events_error.clear();
            }
            Err(e) => self.events_error = format!("{e:#}"),
        }
        if !self.dirty {
            self.config = saved_config.clone();
            self.edit_base = saved_config.clone();
        }
        self.saved_config = saved_config;
        self.preview = match preview {
            Ok(value) => {
                self.preview_error.clear();
                Some(value)
            }
            Err(e) => {
                self.preview_error = format!("{e:#}");
                None
            }
        };
        if !completed.is_empty() {
            self.pending_commands
                .retain(|p| !completed.iter().any(|(id, _, _)| *id == p.id));
            let kind = if completed.iter().any(|(_, status, _)| status != "done") {
                NoticeKind::Error
            } else {
                NoticeKind::Success
            };
            self.inform(
                completed
                    .into_iter()
                    .map(|(_, _, message)| message)
                    .collect::<Vec<_>>()
                    .join("\n"),
                kind,
            );
        }
        if self.job.is_none() && self.pending_commands.is_empty() {
            self.pending_policy = None;
        }
        self.catalog_checkpoint = Some((data_version, local_changes, Instant::now()));
        Ok(())
    }
    fn start_job(
        &mut self,
        label: &'static str,
        work: impl FnOnce() -> Result<JobResult> + Send + 'static,
    ) {
        if self.job.is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = work().map_err(|e| format!("{e:#}"));
            let _ = sender.send(result);
        });
        self.job = Some(Job {
            label,
            started: Instant::now(),
            receiver,
        });
        self.notice = None;
    }
    fn queue(&mut self, command: &str) {
        if self.busy() {
            return;
        }
        let home = self.store.home.clone();
        let action = command.to_owned();
        self.start_job("Sending the command…", move || {
            // Verify that a worker is available before leaving a command in the queue.
            setup::start_background(&home)?;
            let store = Store::open(&home)?;
            store.command(&action)?;
            Ok(JobResult::Queued(store.db.last_insert_rowid(), action))
        });
    }
    fn change_policy(&mut self, id: String, policy: Policy) {
        if !self.busy() {
            self.queue(&format!("policy:{id}:{policy:?}"));
            self.pending_policy = Some((id, policy));
        }
    }
    fn restart_background(&mut self) {
        let home = self.store.home.clone();
        self.start_job("Starting background work…", move || {
            setup::start_background(&home)?;
            Ok(JobResult::Started)
        });
    }
    fn prepare_background(&mut self) {
        let home = self.store.home.clone();
        self.start_job("Opening Desktop Pilot…", move || {
            setup::prepare_settings(&home)?;
            Ok(JobResult::Prepared)
        });
    }
    fn disable_background(&mut self) {
        let home = self.store.home.clone();
        self.start_job("Stopping background work…", move || {
            Ok(JobResult::Disabled(setup::disable_background(&home)?))
        });
    }
    fn save(&mut self) {
        if self.busy() || !self.dirty || self.onboarding {
            return;
        }
        let home = self.store.home.clone();
        let config = self.config.clone();
        let base = self.edit_base.clone();
        self.start_job("Saving settings…", move || {
            let store = Store::open(&home)?;
            let restart =
                should_resume_background(store.get::<bool>("background_disabled")?, &base, &config);
            let mut native_applied = false;
            let result = store.transaction(|| {
                let latest = store.get("config")?;
                let merged = merge_config(&base, &config, &latest)?;
                setup::save_config(&store, &merged)?;
                native_applied = true;
                Ok(merged)
            });
            let merged = match result {
                Ok(merged) => merged,
                Err(error) => {
                    if native_applied {
                        // A failed SQLite commit must not leave Windows startup
                        // using values that were never persisted.
                        let restored = store.transaction(|| {
                            let persisted: Config = store.get("config")?;
                            setup::save_config(&store, &persisted)
                        });
                        if let Err(rollback) = restored {
                            return Err(error
                                .context(format!("Startup rollback also failed: {rollback:#}")));
                        }
                    }
                    return Err(error);
                }
            };
            let warning = if restart {
                setup::start_background(&home).err().map(|error| {
                    format!("Settings were saved, but background work could not start: {error:#}")
                })
            } else {
                None
            };
            Ok(JobResult::Saved(merged, warning))
        });
    }
    fn discard(&mut self) {
        match self.store.get::<Config>("config") {
            Ok(config) => {
                self.config = config.clone();
                self.edit_base = config.clone();
                self.saved_config = config;
                self.dirty = false;
                self.inform("Unsaved changes discarded.", NoticeKind::Info);
            }
            Err(e) => self.fail(format!("{e:#}")),
        }
    }
    fn finish_setup(&mut self) {
        let home = self.store.home.clone();
        let config = self.config.clone();
        self.start_job("Preparing Desktop Pilot…", move || {
            let _com = native::Com::new()?;
            let store = Store::open(&home)?;
            setup::finish(&store, &config)?;
            store.set("setup_draft", &Option::<Config>::None)?;
            store.set("setup_step", &0usize)?;
            Ok(JobResult::Setup(config))
        });
    }
    fn add_shortcuts(&mut self, paths: Vec<PathBuf>) {
        let home = self.store.home.clone();
        self.start_job("Adding shortcuts…", move || {
            let _com = native::Com::new()?;
            #[cfg(test)]
            let import_started = Instant::now();
            #[cfg(test)]
            IDENTITY_EVALUATIONS.with(|calls| calls.set(0));
            let store = Store::open(&home)?;
            // Import changes source ownership too. Serialize it with placement
            // and Undo, and never alter the source set of an unfinished change.
            let _changes = native::exclusive("changes", &store.home)?;
            engine::ensure_no_unfinished(&store)?;
            let mut apps = Vec::new();
            let mut result = ImportResult {
                added: Vec::new(),
                failed: Vec::new(),
                created: 0,
                updated: 0,
            };
            for path in paths {
                match native::read_link(&path) {
                    Ok(app) => apps.push(app),
                    Err(error) => result.failed.push((path, format!("{error:#}"))),
                }
            }
            #[cfg(test)]
            let transaction_started = Instant::now();
            store.transaction(|| {
                let mut catalog_changed = false;
                let mut existing = ImportCatalog::new(store.apps()?);
                // Snapshot only touched entries, not the entire saved catalog.
                // The final classification is based on what actually persisted,
                // regardless of duplicate paths or import ordering.
                let mut before_import: HashMap<String, Option<App>> = HashMap::new();
                for mut app in apps {
                    match engine::imported_quick_copy(&store, &app) {
                        Ok(Some(original)) => {
                            before_import
                                .entry(original.id.clone())
                                .or_insert_with(|| existing.get(&original.id).cloned());
                            result.added.push((original.id, original.name));
                            continue;
                        }
                        Ok(None) => {}
                        Err(error) => {
                            result
                                .failed
                                .push((PathBuf::from(&app.source), format!("{error:#}")));
                            continue;
                        }
                    }
                    if let Err(error) = existing.prepare(&mut app) {
                        result
                            .failed
                            .push((PathBuf::from(&app.source), format!("{error:#}")));
                        continue;
                    }
                    before_import
                        .entry(app.id.clone())
                        .or_insert_with(|| existing.get(&app.id).cloned());
                    if existing.get(&app.id).is_none_or(|saved| saved != &app) {
                        store.put_app(&app)?;
                        catalog_changed = true;
                    }
                    existing.insert(app.id.clone(), app.clone());
                    result.added.push((app.id, app.name));
                }
                let mut seen = HashSet::new();
                result.added.retain(|(id, _)| seen.insert(id.clone()));
                result.created = result
                    .added
                    .iter()
                    .filter(|(id, _)| before_import.get(id).is_some_and(|old| old.is_none()))
                    .count();
                result.updated = result
                    .added
                    .iter()
                    .filter(|(id, _)| {
                        before_import
                            .get(id)
                            .and_then(Option::as_ref)
                            .is_some_and(|old| existing.get(id).is_some_and(|new| new != old))
                    })
                    .count();
                if catalog_changed {
                    // A single revision bump per committed batch notifies the
                    // running monitor without starting an expensive rescan.
                    // No-op and quick-copy reimports leave the revision alone.
                    let epoch: u64 = store.get("catalog_epoch")?;
                    let next = epoch.checked_add(1).context("Catalog revision exhausted")?;
                    store.set("catalog_epoch", &next)?;
                }
                Ok(())
            })?;
            #[cfg(test)]
            import_scaling_tests::record(
                &home,
                IDENTITY_EVALUATIONS.with(|calls| calls.get()),
                import_started.elapsed(),
                transaction_started.elapsed(),
            );
            Ok(JobResult::Added(result))
        });
    }
    fn refresh_layout(&mut self, find: bool) {
        if self.capture.is_some()
            && self
                .store
                .get::<String>("test_desktop")
                .is_ok_and(|value| !value.is_empty())
            && let Ok(Some(layout)) = self.store.get::<Option<native::Layout>>("test_layout")
        {
            if find {
                match engine::suggest_zone(&layout, self.config.slots) {
                    Ok((col, row, width)) => {
                        self.config.zone_col = col;
                        self.config.zone_row = row;
                        self.config.zone_columns = width;
                        self.refresh_dirty();
                    }
                    Err(e) => self.layout_error = format!("{e:#}"),
                }
            }
            self.layout = Some(layout);
            return;
        }
        if self.layout_receiver.is_some() {
            self.find_after_layout |= find;
            return;
        }
        self.find_after_layout = find;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let result = (|| {
                let _com = native::Com::new()?;
                native::layout()
            })()
            .map_err(|e| format!("{e:#}"));
            let _ = sender.send(result);
        });
        self.layout_receiver = Some(receiver);
    }
    fn poll_jobs(&mut self, ctx: &egui::Context) {
        if let Some(job) = &self.job {
            match job.receiver.try_recv() {
                Ok(result) => {
                    self.job = None;
                    match result {
                        Ok(JobResult::Setup(config)) | Ok(JobResult::Saved(config, None)) => {
                            let was_onboarding = self.onboarding;
                            self.config = config.clone();
                            self.saved_config = config.clone();
                            self.edit_base = config;
                            self.dirty = false;
                            self.onboarding = false;
                            self.inform(if was_onboarding {
                                "Desktop Pilot is ready. You can close this window; it will keep working in the tray."
                            } else { "Settings saved." }, NoticeKind::Success);
                            if self.close_after_save {
                                self.allow_close = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                            }
                            self.close_after_save = false;
                        }
                        Ok(JobResult::Saved(config, Some(warning))) => {
                            self.config = config.clone();
                            self.saved_config = config.clone();
                            self.edit_base = config;
                            self.dirty = false;
                            self.close_after_save = false;
                            self.fail(warning);
                        }
                        Ok(JobResult::Queued(id, action)) => {
                            self.pending_commands.push(PendingCommand {
                                id,
                                action,
                                submitted: Instant::now(),
                            });
                        }
                        Ok(JobResult::Started) => {
                            self.inform("Background work started.", NoticeKind::Success)
                        }
                        Ok(JobResult::Prepared) => {}
                        Ok(JobResult::Disabled(config)) => {
                            self.config = config.clone();
                            self.saved_config = config.clone();
                            self.edit_base = config;
                            self.dirty = false;
                            self.inform("Desktop Pilot is turned off. Your shortcuts and local data are kept. Choose a mode in Settings to use it again.", NoticeKind::Success);
                        }
                        Ok(JobResult::Added(result)) => {
                            if !result.added.is_empty() {
                                self.highlighted_apps =
                                    result.added.iter().map(|(id, _)| id.clone()).collect();
                                self.filter.clear();
                                self.app_filter = 5;
                                self.app_page = 0;
                                self.tab = 1;
                                self.page_scroll_reset = true;
                                let kind = if result.created + result.updated > 0 {
                                    NoticeKind::Success
                                } else {
                                    NoticeKind::Info
                                };
                                self.inform(import_summary(&result), kind);
                            }
                            self.import_paths = result
                                .failed
                                .iter()
                                .map(|(path, _)| path.display().to_string())
                                .collect::<Vec<_>>()
                                .join("\n");
                            self.import_error = result
                                .failed
                                .iter()
                                .map(|(path, error)| format!("{}\n{error}", path.display()))
                                .collect::<Vec<_>>()
                                .join("\n\n");
                            self.import_open = !result.failed.is_empty();
                            if self.import_open {
                                self.fail("Some shortcuts could not be added. Check the paths in the dialog and try again.");
                            }
                        }
                        Err(e) => {
                            self.close_after_save = false;
                            self.pending_policy = None;
                            if self.onboarding
                                && engine::setup_complete(&self.store).unwrap_or(false)
                            {
                                // Setup may be complete even if starting the worker failed.
                                // Keep the installed app accessible instead of repeating setup.
                                self.onboarding = false;
                                self.dirty = false;
                            }
                            self.fail(e);
                        }
                    }
                    if let Err(e) = self.reload() {
                        self.fail(format!("{e:#}"));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.pending_policy = None;
                    self.close_after_save = false;
                    self.fail("The operation stopped unexpectedly. Your saved settings are available; try again.");
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.layout_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    self.layout_receiver = None;
                    match result {
                        Ok(layout) => {
                            self.layout_error.clear();
                            if self.find_after_layout {
                                match engine::suggest_zone(&layout, self.config.slots) {
                                    Ok((col, row, width)) => {
                                        self.config.zone_col = col;
                                        self.config.zone_row = row;
                                        self.config.zone_columns = width;
                                        self.refresh_dirty();
                                    }
                                    Err(e) => self.layout_error = format!("{e:#}"),
                                }
                            }
                            if self.onboarding && layout.flags & 1 != 0 {
                                self.config.automatic = false;
                            }
                            self.layout = Some(layout);
                        }
                        Err(e) => self.layout_error = e,
                    }
                    self.find_after_layout = false;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.layout_receiver = None;
                    self.layout_error =
                        "Could not read the desktop layout. Try refreshing it.".into();
                    self.find_after_layout = false;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.job.is_some() || self.layout_receiver.is_some() || !self.pending_commands.is_empty()
        {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
    fn open_import(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            if !self.import_paths.is_empty() {
                self.import_paths.push('\n');
            }
            self.import_paths.push_str(&path.display().to_string());
        }
        self.import_open = true;
        self.import_error.clear();
        self.focus_import = true;
    }
    fn language_and_scale(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let mut language = self.config.language.clone();
        egui::ComboBox::from_id_salt("language")
            .selected_text(if language == "ru" {
                "Русский"
            } else {
                "English"
            })
            .width(116.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut language, "ru".into(), "Русский");
                ui.selectable_value(&mut language, "en".into(), "English");
            })
            .response
            .on_hover_text("Язык / Language");
        if language != self.config.language {
            let saved = self.store.transaction(|| {
                let mut current: Config = self.store.get("config")?;
                current.language = language.clone();
                self.store.set("config", &current)
            });
            match saved {
                Ok(()) => {
                    self.config.language = language.clone();
                    self.saved_config.language = language.clone();
                    self.edit_base.language = language;
                    i18n::set(&self.config.language);
                }
                Err(e) => self.fail(format!("{e:#}")),
            }
        }
        let mut scale = self.ui_scale;
        egui::ComboBox::from_id_salt("ui-scale")
            .selected_text(format!("{:.0}%", self.ui_scale * 100.0))
            .width(88.0)
            .show_ui(ui, |ui| {
                for value in UI_SCALES {
                    ui.selectable_value(&mut scale, value, format!("{:.0}%", value * 100.0));
                }
            })
            .response
            .on_hover_text(t(
                "Дополнительный масштаб поверх настроек Windows. Ctrl+плюс / минус — изменить; Ctrl+0 — 100%.",
                "Additional scale on top of Windows display settings. Ctrl+plus / minus to change; Ctrl+0 for 100%.",
            ));
        self.set_scale(ctx, scale);
    }
    fn set_scale(&mut self, ctx: &egui::Context, requested: f32) {
        let scale = validated_scale(requested);
        if (scale - self.ui_scale).abs() < 0.01 {
            return;
        }
        match self.store.set("ui_scale", &scale) {
            Ok(()) => {
                self.ui_scale = scale;
                ctx.set_zoom_factor(scale);
            }
            Err(error) => self.fail(format!("{error:#}")),
        }
    }
    fn navigation(&mut self, ui: &mut egui::Ui, vertical: bool) {
        let labels = [
            t("Главная", "Home"),
            t("Приложения", "Apps"),
            t("Настройки", "Settings"),
            t("История", "History"),
        ];
        if !vertical && ui.available_width() < 620.0 {
            egui::ComboBox::from_id_salt("main-navigation")
                .selected_text(labels[self.tab.min(labels.len() - 1)])
                .width(ui.available_width().min(360.0))
                .show_ui(ui, |ui| {
                    for (index, label) in labels.iter().enumerate() {
                        ui.selectable_value(&mut self.tab, index, *label);
                    }
                });
            return;
        }
        for (index, label) in labels.iter().enumerate() {
            let selected = self.tab == index;
            let width = if vertical { ui.available_width() } else { 0.0 };
            let response = ui.add_sized(
                [width, 46.0],
                egui::Button::new(RichText::new(*label).size(18.0))
                    .selected(selected)
                    .fill(if selected {
                        Color32::from_rgb(35, 78, 81)
                    } else {
                        Color32::TRANSPARENT
                    }),
            );
            if response.clicked() {
                self.tab = index;
            }
        }
    }
    fn modes(&mut self, ui: &mut egui::Ui) {
        let selected = operating_mode(&self.config);
        for (mode, title, description) in [
            (
                OperatingMode::Adaptive,
                t("Подстраивать автоматически", "Adapt automatically"),
                t(
                    "Программа изучает привычки и сама обновляет быстрые ярлыки. Изменения происходят, когда вы не заняты.",
                    "Learn your routine and update quick shortcuts automatically while you are away.",
                ),
            ),
            (
                OperatingMode::Observe,
                t("Только наблюдать", "Observe only"),
                t(
                    "Собирать статистику и показывать предложения. Состав ярлыков меняется только по вашей кнопке.",
                    "Collect usage statistics and suggest apps. Change shortcuts only when you choose Apply.",
                ),
            ),
            (
                OperatingMode::Manual,
                t("Управлять вручную", "Control manually"),
                t(
                    "Вы выбираете приложения и применяете изменения. Новая статистика не собирается.",
                    "Choose apps and apply changes yourself. New usage statistics are not collected.",
                ),
            ),
        ] {
            let active = selected == mode;
            let auto_arrange = self.layout.as_ref().is_some_and(|l| l.flags & 1 != 0);
            egui::Frame::group(ui.style())
                .fill(if active {
                    Color32::from_rgb(32, 67, 70)
                } else {
                    SURFACE
                })
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    if ui
                        .add_enabled(
                            mode != OperatingMode::Adaptive || !auto_arrange,
                            egui::Button::new(RichText::new(title).size(19.0).strong())
                                .selected(active)
                                .min_size(egui::vec2(0.0, 40.0)),
                        )
                        .clicked()
                    {
                        apply_mode(&mut self.config, mode);
                        self.refresh_dirty();
                    }
                    ui.label(description);
                });
            ui.add_space(6.0);
        }
        if self.layout.as_ref().is_some_and(|l| l.flags & 1 != 0) {
            ui.colored_label(WARNING, t(
                "В Windows включено автоматическое упорядочивание значков. Для автоматического размещения: правый клик на рабочем столе → Вид → отключить «Упорядочить значки автоматически», затем обновите схему ниже.",
                "Windows auto-arrange is on. To place shortcuts automatically: right-click the desktop → View → clear Auto arrange icons, then refresh the layout below."
            ));
        }
    }
    fn slots(&mut self, ui: &mut egui::Ui) {
        ui.label(t(
            "Сколько быстрых ярлыков оставить под рукой?",
            "How many quick shortcuts would you like?",
        ));
        let before = self.config.slots;
        ui.horizontal_wrapped(|ui| {
            for value in 1..=8 {
                if ui
                    .add_sized(
                        [46.0, 44.0],
                        egui::Button::new(RichText::new(value.to_string()).size(20.0))
                            .selected(self.config.slots == value),
                    )
                    .clicked()
                {
                    self.config.slots = value;
                }
            }
        });
        if before != self.config.slots {
            self.config.max_changes = self.config.max_changes.min(self.config.slots);
            self.refresh_dirty();
        }
    }
    fn learning_settings(&mut self, ui: &mut egui::Ui) {
        let changed = ui
            .checkbox(
                &mut self.config.learning_enabled,
                t(
                    "Сначала изучить мои привычки",
                    "Learn my routine before placing shortcuts",
                ),
            )
            .changed();
        if changed {
            self.refresh_dirty();
        }
        ui.label(t(
            "До окончания наблюдения программа не создаёт ярлыки. Прогресс сохраняется после перезапуска.",
            "No shortcuts are created during observation. Your progress is kept when the app restarts."
        ));
        if !self.config.learning_enabled {
            return;
        }
        ui.add_space(4.0);
        let before = self.config.learning_mode;
        let label = |mode| match mode {
            Mode::Sessions => t(
                "После нескольких сеансов работы",
                "After a number of computer sessions",
            ),
            Mode::Days => t(
                "После нескольких активных дней",
                "After a number of active days",
            ),
            Mode::Either => t(
                "Сеансы или дни — что наступит раньше",
                "Sessions or days — whichever comes first",
            ),
            Mode::Both => t(
                "Нужны и сеансы, и активные дни",
                "Require both sessions and active days",
            ),
        };
        egui::ComboBox::from_id_salt("learning-mode")
            .width(ui.available_width().min(430.0))
            .wrap()
            .selected_text(label(before))
            .show_ui(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                for mode in [Mode::Sessions, Mode::Days, Mode::Either, Mode::Both] {
                    ui.selectable_value(&mut self.config.learning_mode, mode, label(mode));
                }
            });
        if before != self.config.learning_mode {
            self.refresh_dirty();
        }
        if self.config.learning_mode != Mode::Days {
            let mut count = i64::from(self.config.learning_sessions);
            if number(
                ui,
                t("Сеансов за компьютером", "Computer sessions"),
                &mut count,
                1,
                100,
            ) {
                self.config.learning_sessions = count as u32;
                self.refresh_dirty();
            }
        }
        if self.config.learning_mode != Mode::Sessions {
            let mut count = i64::from(self.config.learning_days);
            if number(ui, t("Активных дней", "Active days"), &mut count, 1, 90) {
                self.config.learning_days = count as u32;
                self.refresh_dirty();
            }
        }
        ui.small(t(
            "Сеанс и активный день засчитываются после 5 минут работы. Перерыв от 30 минут начинает новый сеанс.",
            "A session and an active day count after 5 minutes of activity. A break of 30 minutes starts a new session."
        ));
        if !self.config.collect {
            ui.colored_label(WARNING, t(
                "Сбор статистики выключен: наблюдение не продвигается. Включите наблюдение или отключите ожидание выше.",
                "Statistics are off, so observation cannot progress. Enable observation or turn off the requirement above."
            ));
        }
    }
    fn learning_progress(&self, ui: &mut egui::Ui) {
        if !self.saved_config.learning_enabled || self.learning.ready(&self.saved_config) {
            return;
        }
        ui.label(t(
            "До первых рекомендаций",
            "Before the first recommendations",
        ));
        if self.saved_config.learning_mode != Mode::Days {
            let target = self.saved_config.learning_sessions.max(1);
            let fraction = (self.learning.sessions as f32 / target as f32).clamp(0.0, 1.0);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_height(26.0)
                    .text(format!(
                        "{} / {} {}",
                        self.learning.sessions,
                        target,
                        t("сеансов", "sessions")
                    )),
            );
        }
        if self.saved_config.learning_mode != Mode::Sessions {
            let target = self.saved_config.learning_days.max(1);
            let fraction = (self.learning.active_days() as f32 / target as f32).clamp(0.0, 1.0);
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_height(26.0)
                    .text(format!(
                        "{} / {} {}",
                        self.learning.active_days(),
                        target,
                        t("активных дней", "active days")
                    )),
            );
        }
        if self.saved_config.learning_mode == Mode::Either {
            ui.small(t(
                "Достаточно любого из двух условий.",
                "Either requirement is enough.",
            ));
        } else if self.saved_config.learning_mode == Mode::Both {
            ui.small(t(
                "Нужно выполнить оба условия.",
                "Both requirements must be met.",
            ));
        }
        ui.small(t(
            "Продолжайте пользоваться компьютером. Рейтинг станет полезнее по мере накопления наблюдений.",
            "Keep using your computer normally. Recommendations improve as observations accumulate."
        ));
    }
    fn zone(&mut self, ui: &mut egui::Ui) {
        ui.label(t(
            "Выберите место прямо на схеме или поручите подбор программе.",
            "Choose a place on the layout, or let the app find one.",
        ));
        ui.horizontal_wrapped(|ui| {
            if primary(
                ui,
                self.layout_receiver.is_none(),
                t("Подобрать свободное место", "Find a free place"),
            )
            .clicked()
            {
                self.refresh_layout(true);
            }
            if ui
                .add_enabled(
                    self.layout_receiver.is_none(),
                    egui::Button::new(t("Обновить схему", "Refresh layout")),
                )
                .clicked()
            {
                self.refresh_layout(false);
            }
            if self.layout_receiver.is_some() {
                ui.spinner();
                ui.label(t("Проверяем рабочий стол…", "Checking the desktop…"));
            }
        });
        if !self.layout_error.is_empty() {
            ui.colored_label(WARNING, i18n::runtime(&self.layout_error));
        }
        if let Some(layout) = self.layout.clone() {
            self.desktop_map(ui, &layout);
        } else if self.layout_receiver.is_none() {
            ui.label(t(
                "Схема пока недоступна. Можно продолжить наблюдение и выбрать место позже.",
                "The layout is unavailable. You can keep observing and choose a place later.",
            ));
        }
        ui.small(t(
            "Показан основной монитор. Ваши существующие значки сохраняют позиции. Схема не является снимком экрана.",
            "Primary monitor. Existing icons keep their positions. This is a layout diagram, not a screenshot."
        ));
        ui.add_space(4.0);
        ui.label(t("Форма области", "Area shape"));
        let previous = self.config.zone_columns;
        ui.horizontal_wrapped(|ui| {
            for (columns, label) in [
                (1, t("Один столбец", "One column")),
                (2, t("Два столбца", "Two columns")),
                (self.config.slots as i32, t("Одна строка", "One row")),
            ] {
                ui.selectable_value(&mut self.config.zone_columns, columns, label);
            }
        });
        if previous != self.config.zone_columns {
            self.refresh_dirty();
        }
        collapsing_header(ui, t("Точное положение", "Exact position")).show(ui, |ui| {
            ui.small(t(
                "Столбцы и строки отсчитываются от нуля.",
                "Columns and rows are numbered from zero.",
            ));
            let mut col = i64::from(self.config.zone_col);
            let mut row = i64::from(self.config.zone_row);
            let mut width = i64::from(self.config.zone_columns);
            if number(ui, t("Начальный столбец", "First column"), &mut col, 0, 40) {
                self.config.zone_col = col as i32;
                self.refresh_dirty();
            }
            if number(ui, t("Начальная строка", "First row"), &mut row, 0, 40) {
                self.config.zone_row = row as i32;
                self.refresh_dirty();
            }
            if number(
                ui,
                t("Столбцов в области", "Columns in the area"),
                &mut width,
                1,
                8,
            ) {
                self.config.zone_columns = width as i32;
                self.refresh_dirty();
            }
        });
    }
    fn desktop_map(&mut self, ui: &mut egui::Ui, layout: &native::Layout) {
        let width = layout.work[2].saturating_sub(layout.work[0]);
        let height = layout.work[3].saturating_sub(layout.work[1]);
        if width <= 0 || height <= 0 || layout.spacing.x <= 0 || layout.spacing.y <= 0 {
            ui.colored_label(
                WARNING,
                t(
                    "Геометрия монитора недоступна",
                    "Monitor geometry is unavailable",
                ),
            );
            return;
        }
        let anchor_cfg = Config {
            slots: 1,
            zone_col: 0,
            zone_row: 0,
            zone_columns: 1,
            ..self.config.clone()
        };
        let anchor = match engine::zone_candidates(layout, &anchor_cfg)
            .and_then(|points| points.into_iter().next().context("Invalid desktop grid"))
        {
            Ok(point) => point,
            Err(e) => {
                ui.colored_label(WARNING, i18n::runtime(&format!("{e:#}")));
                return;
            }
        };
        let scale = (ui.available_width().min(760.0) / width as f32).min(310.0 / height as f32);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(width as f32 * scale, height as f32 * scale),
            egui::Sense::click(),
        );
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 8.0, Color32::from_rgb(17, 25, 35));
        let project = |point: &Point| {
            rect.min
                + egui::vec2(
                    (point.x - layout.work[0]) as f32 * scale,
                    (point.y - layout.work[1]) as f32 * scale,
                )
        };
        let sx = layout.spacing.x;
        let sy = layout.spacing.y;
        let bx = anchor.x.rem_euclid(sx);
        let by = anchor.y.rem_euclid(sy);
        let cell_rect = |point: &Point| {
            egui::Rect::from_min_size(
                project(&Point {
                    x: point.x - bx,
                    y: point.y - by,
                }),
                egui::vec2(sx as f32 * scale, sy as f32 * scale),
            )
            .shrink(2.0)
        };
        let columns = ((width / sx) + 1).min(41);
        let rows = ((height / sy) + 1).min(41);
        for row in 0..rows {
            for col in 0..columns {
                let point = Point {
                    x: anchor.x + col * sx,
                    y: anchor.y + row * sy,
                };
                let cell = cell_rect(&point);
                if !rect.contains_rect(cell) {
                    continue;
                }
                painter.rect_filled(cell, 3.0, Color32::from_rgb(34, 45, 59));
            }
        }
        for item in &layout.items {
            let cell = cell_rect(&item.position);
            if cell.intersects(rect) {
                painter.rect_filled(cell, 3.0, Color32::from_rgb(86, 102, 121));
                if self
                    .state
                    .owned
                    .iter()
                    .any(|o| norm(&o.path) == norm(&item.path))
                {
                    painter.rect_filled(cell.shrink(3.0), 2.0, Color32::from_rgb(48, 116, 112));
                }
            }
        }
        let mut free = 0;
        if let Ok(points) = engine::zone_candidates(layout, &self.config) {
            for (index, point) in points.iter().enumerate() {
                let cell = cell_rect(point);
                let occupied = layout.items.iter().any(|i| {
                    (i.position.x - point.x).abs() < sx - 2
                        && (i.position.y - point.y).abs() < sy - 2
                });
                let inside = point.x - bx >= layout.work[0]
                    && point.y - by >= layout.work[1]
                    && point.x - bx + sx <= layout.work[2]
                    && point.y - by + sy <= layout.work[3];
                if inside && !occupied {
                    free += 1;
                }
                let color = if inside && !occupied { ACCENT } else { WARNING };
                painter.rect_filled(cell, 3.0, color);
                if cell.width() >= 18.0 && cell.height() >= 18.0 {
                    painter.text(
                        cell.center(),
                        egui::Align2::CENTER_CENTER,
                        (index + 1).to_string(),
                        egui::FontId::proportional(
                            cell.width().min(cell.height()).clamp(13.0, 18.0),
                        ),
                        Color32::from_rgb(18, 33, 39),
                    );
                }
            }
        }
        if let Some(pointer) = response
            .interact_pointer_pos()
            .filter(|_| response.clicked())
        {
            let x = layout.work[0] as f32 + (pointer.x - rect.left()) / scale;
            let y = layout.work[1] as f32 + (pointer.y - rect.top()) / scale;
            let col = ((x - (anchor.x - bx) as f32) / sx as f32).floor() as i32;
            let row = ((y - (anchor.y - by) as f32) / sy as f32).floor() as i32;
            if (0..=40).contains(&col) && (0..=40).contains(&row) {
                self.config.zone_col = col;
                self.config.zone_row = row;
                self.refresh_dirty();
            }
            response.request_focus();
        }
        if response.has_focus() {
            let mut col = self.config.zone_col;
            let mut row = self.config.zone_row;
            ui.input_mut(|input| {
                for (key, dc, dr) in [
                    (egui::Key::ArrowLeft, -1, 0),
                    (egui::Key::ArrowRight, 1, 0),
                    (egui::Key::ArrowUp, 0, -1),
                    (egui::Key::ArrowDown, 0, 1),
                ] {
                    if input.consume_key(egui::Modifiers::NONE, key) {
                        col = (col + dc).clamp(0, 40);
                        row = (row + dr).clamp(0, 40);
                    }
                }
            });
            if col != self.config.zone_col || row != self.config.zone_row {
                self.config.zone_col = col;
                self.config.zone_row = row;
                self.refresh_dirty();
            }
            painter.rect_stroke(
                rect.shrink(1.0),
                8.0,
                egui::Stroke::new(2.0, ACCENT),
                egui::StrokeKind::Inside,
            );
        }
        response.on_hover_text(t(
            "Нажмите свободную ячейку, чтобы выбрать начало области. Сфокусировав схему клавишей Tab, меняйте положение стрелками.",
            "Click a free cell to choose the start of the area. Focus the layout with Tab, then use the arrow keys to move it.",
        ));
        ui.horizontal_wrapped(|ui| {
            ui.colored_label(
                ACCENT,
                t("■ Выбранные свободные места", "■ Selected free places"),
            );
            ui.colored_label(MUTED, t("■ Существующие значки", "■ Existing icons"));
            if free < self.config.slots {
                ui.colored_label(WARNING, t("■ Занято или за краем", "■ Occupied or outside"));
            }
        });
        ui.small(format!(
            "{}: {} / {}",
            t(
                "Свободно для новых ярлыков",
                "Free places for new shortcuts"
            ),
            free,
            self.config.slots
        ));
    }
    fn welcome(&mut self, ui: &mut egui::Ui) {
        let (title, subtitle) = match self.onboarding_step {
            0 => (
                t("Быстрый доступ в вашем стиле", "Quick access, your way"),
                t(
                    "Desktop Pilot добавляет удобные копии ярлыков. Ваши файлы, папки и исходные ярлыки остаются на месте.",
                    "Desktop Pilot adds convenient quick shortcuts. Your files, folders and original shortcuts stay where they are.",
                ),
            ),
            1 => (
                t("Немного места для нужного", "Make room for what you use"),
                t(
                    "Выберите число ярлыков и время на изучение привычек. Эти настройки можно изменить позже.",
                    "Choose how many shortcuts you want and how long to observe your routine. You can change this later.",
                ),
            ),
            _ => (
                t("Где разместить ярлыки?", "Where should the shortcuts go?"),
                t(
                    "Выберите свободную область на основном мониторе. Остальные значки программа не передвигает.",
                    "Choose a free area on your primary monitor. Other icons keep their positions.",
                ),
            ),
        };
        ui.heading(RichText::new(title).size(28.0));
        ui.label(subtitle);
        ui.add_space(14.0);
        match self.onboarding_step {
            0 => self.modes(ui),
            1 => {
                card(ui, |ui| self.slots(ui));
                card(ui, |ui| self.learning_settings(ui));
            }
            _ => {
                card(ui, |ui| self.zone(ui));
                card(ui, |ui| {
                    ui.checkbox(
                        &mut self.config.autostart,
                        t(
                            "Запускать при входе в Windows",
                            "Start when I sign in to Windows",
                        ),
                    );
                    ui.checkbox(
                        &mut self.config.respect_taskbar_pins,
                        t(
                            "Не дублировать приложения с панели задач",
                            "Skip apps already pinned to the taskbar",
                        ),
                    );
                    ui.small(t(
                        "Данные остаются на этом компьютере. Сохраняются дневные счётчики приложений; текст окон и нажатия клавиш не записываются.",
                        "Data stays on this computer. Daily app-usage totals are stored; window text and keystrokes are not recorded."
                    ));
                    collapsing_header(
                        ui,
                        t("Куда устанавливается программа", "Installation location"),
                    )
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(self.store.home.display().to_string())
                                .wrap()
                                .selectable(true),
                        );
                    });
                });
            }
        }
    }
    fn current_health(&self) -> Health {
        let value = health(
            self.background_alive,
            self.heartbeat,
            now(),
            &self.status,
            &self.saved_config,
            &self.learning,
        );
        if (!self.autostart_warning.is_empty()
            || !self.tray_warning.is_empty()
            || !self.settings_shortcut_warning.is_empty())
            && self.background_alive
            && !matches!(value, Health::Starting | Health::Unresponsive)
        {
            Health::Attention
        } else {
            value
        }
    }
    fn next_review(&self) -> String {
        if !self.saved_config.automatic {
            return t("По вашей команде", "When you choose Apply").into();
        }
        if !self.background_alive {
            return t(
                "После запуска фоновой работы",
                "After background work starts",
            )
            .into();
        }
        if !self.learning.ready(&self.saved_config) {
            return t(
                "После завершения наблюдения",
                "After observation is complete",
            )
            .into();
        }
        if self.state.last_review == 0 {
            return t(
                "Когда появится подходящий состав",
                "When eligible apps are available",
            )
            .into();
        }
        let due = self
            .state
            .last_review
            .saturating_add(self.saved_config.interval_days.saturating_mul(86400));
        if due <= now() {
            t("При ближайшем бездействии", "At the next idle opportunity").into()
        } else {
            when(due)
        }
    }
    fn name_for(&self, id: &str) -> String {
        self.apps
            .iter()
            .find(|app| app.id == id)
            .map(|app| app.name.clone())
            .unwrap_or_else(|| t("Недоступное приложение", "Unavailable app").into())
    }
    fn overview(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new(t("Ваш рабочий стол", "Your desktop")).size(28.0));
        if ui.available_width() >= 700.0 {
            ui.label(t(
                "Нужные приложения рядом. Привычный порядок сохранён.",
                "Keep useful apps close and your familiar layout intact.",
            ));
        }
        ui.add_space(12.0);
        card(ui, |ui| {
            let current = self.current_health();
            ui.colored_label(
                health_color(current),
                RichText::new(health_label(current)).size(22.0).strong(),
            );
            match current {
                Health::Offline => {
                    if let Some(error) = stopped_worker_error(
                        self.background_alive,
                        self.background_disabled,
                        &self.status,
                    ) {
                        ui.label(t(
                            "Фоновая работа завершилась с ошибкой. Настройки, история и ярлыки сохранены.",
                            "Background work ended with an error. Your settings, history and shortcuts are kept.",
                        ));
                        ui.label(i18n::runtime(error));
                        if ui.button(t("Посмотреть историю", "View history")).clicked()
                        {
                            self.tab = 3;
                            self.history_errors_only = true;
                        }
                    } else {
                        ui.label(if self.background_disabled {
                            t("Сбор статистики, автоматические изменения и автозапуск отключены. Ярлыки и данные сохранены. Можно запустить ручное управление или выбрать другой режим в настройках.",
                              "Statistics, automatic changes and startup are turned off. Your shortcuts and data are kept. Start manual control or choose another mode in Settings.")
                        } else {
                            t(
                                "Наблюдение и автоматические изменения сейчас не выполняются. Настройки и накопленные данные сохранены.",
                                "Observation and automatic changes are stopped. Your settings and usage history are kept.",
                            )
                        });
                    }
                    if primary(
                        ui,
                        self.job.is_none(),
                        if self.background_disabled {
                            t("Запустить ручное управление", "Start manual control")
                        } else {
                            t("Возобновить работу", "Resume background work")
                        },
                    )
                    .clicked()
                    {
                        self.restart_background();
                    }
                    if self.background_disabled
                        && ui
                            .button(t("Выбрать режим в настройках", "Choose a mode in Settings"))
                            .clicked()
                    {
                        self.tab = 2;
                        self.settings_section = 0;
                    }
                }
                Health::Unresponsive => {
                    if self.heartbeat.saturating_sub(now()) > 150 {
                        ui.label(t(
                            "Время последнего ответа позже текущего времени Windows. Часы могли быть переведены назад. Пока не придёт новый ответ, работа фонового процесса не подтверждена.",
                            "The last response is later than the current Windows time. The clock may have moved back. Background activity cannot be confirmed until a fresh response arrives."
                        ));
                    } else {
                        ui.label(t(
                            "Программа ещё запущена, но давно не обновляла состояние. Текущая операция может занимать больше времени.",
                            "The process is still running, but has not reported recently. The current operation may be taking longer."
                        ));
                    }
                    ui.small(format!(
                        "{}: {}",
                        t("Последний ответ", "Last response"),
                        when(self.heartbeat)
                    ));
                }
                Health::Attention => {
                    if !self.status.is_empty() {
                        ui.label(i18n::runtime(&self.status));
                    }
                    if !self.autostart_warning.is_empty()
                        && !self.status.contains(&self.autostart_warning)
                    {
                        ui.label(i18n::runtime(&self.autostart_warning));
                    }
                    if !self.tray_warning.is_empty() && !self.status.contains(&self.tray_warning) {
                        ui.label(i18n::runtime(&self.tray_warning));
                    }
                    if ui.button(t("Посмотреть историю", "View history")).clicked()
                    {
                        self.tab = 3;
                        self.history_errors_only = true;
                    }
                }
                Health::ObservationPaused => {
                    ui.label(t(
                        "Для завершения наблюдения включите сбор статистики в настройках или отключите ожидание перед размещением.",
                        "To finish observation, enable statistics in Settings or turn off the observation requirement."
                    ));
                    if ui.button(t("Открыть настройки", "Open settings")).clicked()
                    {
                        self.tab = 2;
                    }
                }
                Health::Suggestions => {
                    ui.label(t("Наблюдение продолжается. Состав ярлыков меняется по кнопке «Применить сейчас».",
                        "Observation continues. Choose Apply now to update your shortcuts."));
                }
                Health::Manual => {
                    ui.label(t("Новая статистика не собирается. Вы выбираете состав и момент применения.",
                        "No new usage statistics are collected. You choose the apps and when to apply changes."));
                }
                Health::Automatic => {
                    ui.label(format!(
                        "{}: {}",
                        t("Следующий пересмотр", "Next review"),
                        self.next_review()
                    ));
                }
                _ => {}
            }
            if !self.settings_shortcut_warning.is_empty()
                && !self.status.contains(&self.settings_shortcut_warning)
            {
                ui.colored_label(WARNING, i18n::runtime(&self.settings_shortcut_warning));
            }
            self.learning_progress(ui);
        });
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading(t("Сейчас под рукой", "Within reach"));
                ui.colored_label(
                    ACCENT,
                    RichText::new(format!(
                        "{} / {}",
                        self.state.owned.len(),
                        self.saved_config.slots
                    ))
                    .size(24.0)
                    .strong(),
                );
            });
            if self.state.owned.is_empty() {
                ui.label(if !self.learning.ready(&self.saved_config) {
                    t("Первые ярлыки появятся после наблюдения. Можно заранее выбрать приложения, которые всегда должны быть на столе.",
                      "Your first shortcuts will appear after observation. You can already choose apps that should always be on the desktop.")
                } else {
                    t("Пока нет быстрых ярлыков. Выберите приложения или дождитесь подходящих рекомендаций.",
                      "There are no quick shortcuts yet. Choose apps or wait for eligible recommendations.")
                });
                if ui.button(t("Выбрать приложения", "Choose apps")).clicked() {
                    self.tab = 1;
                }
            } else {
                ui.scope(|ui| {
                    // These rows contain text only, so they need no button-sized hit area.
                    ui.spacing_mut().interact_size.y = 22.0;
                    ui.spacing_mut().item_spacing.y = 6.0;
                    for owned in &self.state.owned {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(self.name_for(&owned.app));
                            ui.small(if owned.manual {
                                t("Закреплено вами", "Pinned by you")
                            } else {
                                t("Выбрано автоматически", "Automatic selection")
                            });
                        });
                    }
                });
                ui.small(t(
                    "Переместите быстрый ярлык, чтобы закрепить его. Удалите, чтобы исключить из автоматического возврата.",
                    "Move a quick shortcut to pin it. Delete one to prevent it from being added back."
                ));
            }
        });
        card(ui, |ui| {
            ui.heading(t("Следующее изменение", "Next change"));
            if self.dirty {
                ui.colored_label(WARNING, t(
                    "Ниже — предложение по сохранённым настройкам. Сохраните или отмените правки перед применением.",
                    "This proposal uses your saved settings. Save or discard your edits before applying."
                ));
            }
            if let Some(preview) = &self.preview {
                if preview.added.is_empty() && preview.removed.is_empty() {
                    ui.label(t(
                        "Текущий состав уже соответствует правилам и рейтингу.",
                        "The current selection already matches your rules and ranking.",
                    ));
                } else {
                    if !preview.added.is_empty() {
                        ui.colored_label(ACCENT, t("Добавить", "Add"));
                        for id in &preview.added {
                            ui.label(self.name_for(id));
                        }
                    }
                    if !preview.removed.is_empty() {
                        ui.colored_label(WARNING, t("Убрать быстрые копии", "Remove quick copies"));
                        for id in &preview.removed {
                            ui.label(self.name_for(id));
                        }
                    }
                    ui.small(t(
                        "Исходные ярлыки сохранятся.",
                        "Original shortcuts will be kept.",
                    ));
                }
            } else {
                ui.colored_label(WARNING, i18n::runtime(&self.preview_error));
            }
            let ready = self.learning.ready(&self.saved_config);
            let auto_arrange = self.layout.as_ref().is_some_and(|l| l.flags & 1 != 0);
            let geometry_changed = self.layout.as_ref().is_some_and(|l| {
                !self.state.topology.is_empty() && self.state.topology != l.topology
            });
            let can_apply = !self.busy()
                && !self.dirty
                && ready
                && self.preview.is_some()
                && !self.interrupted
                && !auto_arrange
                && !geometry_changed
                && self.background_alive;
            ui.horizontal_wrapped(|ui| {
                if primary(ui, can_apply, t("Применить сейчас", "Apply now"))
                    .on_hover_text(t("Разовое обновление по сохранённым правилам. Настройка автоматики не изменится.",
                        "Update once using your saved rules. This does not change the automation setting.")).clicked() {
                    self.queue("apply");
                }
                if ui.add_enabled(self.can_undo && !self.busy(), egui::Button::new(t("Отменить последнее изменение", "Undo last change")))
                    .on_hover_text(t("Вернуть предыдущий состав. Автоматические изменения будут приостановлены.",
                        "Restore the previous selection. Automatic changes will be paused.")).clicked() {
                    self.queue("rollback");
                }
            });
            if !ready {
                ui.small(t("Применение станет доступно после наблюдения. Условие можно изменить в настройках.",
                    "Apply becomes available after observation. You can change the requirement in Settings."));
            }
            if self.interrupted {
                ui.colored_label(WARNING, t("Предыдущее изменение не завершено. Сначала восстановите его кнопкой отмены.",
                    "A previous change was interrupted. Use Undo to recover it first."));
            }
            if auto_arrange {
                ui.colored_label(WARNING, t("Чтобы размещать ярлыки, отключите автоматическое упорядочивание в меню «Вид» рабочего стола и обновите схему в настройках.",
                    "To place shortcuts, turn off Auto arrange icons in the desktop View menu and refresh the layout in Settings."));
            }
            if geometry_changed {
                ui.colored_label(WARNING, t("Изменились экран или размер значков. Проверьте размещение и примите новую геометрию.",
                    "The display or icon size changed. Check the layout and accept the new geometry."));
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(t("Принять текущее размещение", "Accept current layout")),
                    )
                    .clicked()
                {
                    self.queue("accept_geometry");
                }
            }
            ui.small(format!(
                "{}: {}",
                t("Последний пересмотр", "Last review"),
                when(self.state.last_review)
            ));
        });
        collapsing_header(ui, t("Почему выбраны эти приложения", "Why these apps were chosen")).show(ui, |ui| {
            let contenders: Vec<_> = self.ranking.iter().filter(|r| r.eligible && !self.state.owned.iter().any(|o| o.app == r.id)).take(5).collect();
            if contenders.is_empty() {
                ui.label(t(
                    "Пока нет других подходящих кандидатов. В разделе «Приложения» можно увидеть причину для каждого ярлыка.",
                    "There are no other eligible candidates yet. Open Apps to see the reason for each shortcut."
                ));
            }
            for candidate in contenders {
                ui.strong(&candidate.name);
                ui.label(i18n::runtime(&candidate.reason));
            }
            if ui.button(t("Все приложения и правила", "All apps and rules")).clicked() {
                self.tab = 1;
            }
        });
    }
    fn applications(&mut self, ui: &mut egui::Ui) {
        let previous_filter = (self.filter.clone(), self.app_filter, self.app_sort);
        let compact = ui.available_width() < 700.0;
        if !compact {
            ui.heading(RichText::new(t("Приложения", "Apps")).size(28.0));
            ui.label(t(
                "Выберите, что всегда должно быть под рукой, а что не нужно добавлять.",
                "Choose what should always be nearby and what to leave out.",
            ));
            ui.add_space(10.0);
        }
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let search = ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .id_salt("app-search")
                        .hint_text(t(
                            "Поиск по имени, папке или пути…",
                            "Search by name, folder or path…",
                        ))
                        .desired_width(ui.available_width().min(430.0)),
                );
                if self.focus_search {
                    search.request_focus();
                    search.scroll_to_me(Some(egui::Align::TOP));
                    self.focus_search = false;
                }
                if search.has_focus()
                    && ui.input_mut(|input| {
                        input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                    })
                {
                    self.filter.clear();
                }
                if !self.filter.is_empty() && ui.button(t("Сбросить", "Clear")).clicked() {
                    self.filter.clear();
                    search.request_focus();
                }
            });
            ui.horizontal_wrapped(|ui| {
                let add_label = if compact {
                    t("Добавить…", "Add…")
                } else {
                    t("Добавить ярлыки…", "Add shortcuts…")
                };
                if primary(ui, !self.busy(), add_label)
                    .on_hover_text(t("Добавить ярлыки · Ctrl+O", "Add shortcuts · Ctrl+O"))
                    .clicked()
                {
                    self.open_import([]);
                }
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(if compact {
                            t("Обновить", "Refresh")
                        } else {
                            t("Найти установленные приложения", "Find installed apps")
                        }),
                    )
                    .on_hover_text(t(
                        "Найти ярлыки на рабочем столе и в меню «Пуск». Ваши правила сохранятся. F5 — обновить список.",
                        "Find shortcuts on the desktop and in the Start menu. Your rules are kept. F5 refreshes the list.",
                    ))
                    .clicked()
                {
                    self.queue("rescan");
                }
            });
            let filters_active = self.app_filter != 0 || self.app_sort != 0;
            let search_active = !self.filter.trim().is_empty();
            let filter_heading = if self.app_filter != 0 {
                format!(
                    "{}: {}",
                    t("Фильтр", "Filter"),
                    app_filter_name(self.app_filter)
                )
            } else if self.app_sort != 0 {
                format!(
                    "{}: {}",
                    t("Порядок", "Order"),
                    app_sort_name(self.app_sort, search_active)
                )
            } else {
                t("Фильтр и порядок", "Filter and order").into()
            };
            let show_filters = |ui: &mut egui::Ui| {
                let vertical = ui.available_width() < 540.0;
                let selectors = |ui: &mut egui::Ui| {
                    egui::ComboBox::from_id_salt("app-filter")
                        .width(ui.available_width().min(250.0))
                        .wrap()
                        .selected_text(app_filter_name(self.app_filter))
                        .show_ui(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            for (value, label) in [
                                (0, t("Все приложения", "All apps")),
                                (1, t("Подходят для размещения", "Eligible for placement")),
                                (2, t("Всегда на столе", "Always on desktop")),
                                (3, t("Не добавлять", "Never add")),
                                (4, t("Недоступные", "Unavailable")),
                            ] {
                                ui.selectable_value(&mut self.app_filter, value, label);
                            }
                            if !self.highlighted_apps.is_empty() {
                                ui.selectable_value(
                                    &mut self.app_filter,
                                    5,
                                    t("Последний импорт", "Last import"),
                                );
                            }
                        });
                    egui::ComboBox::from_id_salt("app-sort")
                        .width(ui.available_width().min(250.0))
                        .wrap()
                        .selected_text(app_sort_name(self.app_sort, search_active))
                        .show_ui(ui, |ui| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                            ui.selectable_value(
                                &mut self.app_sort,
                                0,
                                app_sort_name(0, search_active),
                            );
                            if search_active || self.app_sort == 3 {
                                ui.selectable_value(
                                    &mut self.app_sort,
                                    3,
                                    t("По алфавиту", "Alphabetically"),
                                );
                            }
                            ui.selectable_value(
                                &mut self.app_sort,
                                1,
                                t("По рейтингу", "By ranking"),
                            );
                            ui.selectable_value(
                                &mut self.app_sort,
                                2,
                                t("По времени использования", "By usage time"),
                            );
                        });
                };
                if vertical {
                    ui.vertical(selectors);
                } else {
                    ui.horizontal(selectors);
                }
            };
            if compact {
                collapsing_header(ui, &filter_heading)
                    .id_salt("catalog-filters")
                    .default_open(filters_active)
                    .show(ui, show_filters);
            } else {
                let mut show_filters = show_filters;
                show_filters(ui);
                ui.small(t(
                    "Можно перетащить ярлыки в окно. Поиск: Ctrl+F.",
                    "You can also drag shortcuts into this window. Search: Ctrl+F.",
                ));
            }
        });
        if previous_filter != (self.filter.clone(), self.app_filter, self.app_sort) {
            self.app_page = 0;
            self.page_scroll_reset = true;
        }
        let always_count = self
            .apps
            .iter()
            .filter(|app| app.valid && app.policy == Policy::Always)
            .count();
        if always_count > self.saved_config.slots {
            card(ui, |ui| {
                ui.colored_label(
                    WARNING,
                    format!(
                        "{}: {} · {}: {}",
                        t("Всегда на столе", "Always on desktop"),
                        always_count,
                        t("Доступно мест", "Available slots"),
                        self.saved_config.slots
                    ),
                );
                ui.label(t(
                    "Для всех закреплённых приложений не хватает мест. Увеличьте число ярлыков или измените правило у части приложений; затем можно применить состав.",
                    "There are not enough slots for all pinned apps. Add more slots or change some app rules before applying the selection.",
                ));
                if ui
                    .button(t("Изменить число мест", "Change slot count"))
                    .clicked()
                {
                    self.tab = 2;
                    self.settings_section = 1;
                }
            });
        }
        if !self.catalog_warning.is_empty() {
            card(ui, |ui| {
                ui.colored_label(
                    WARNING,
                    t(
                        "Список обновлён не полностью",
                        "Some shortcuts could not be scanned",
                    ),
                );
                ui.label(t("Доступные приложения сохранены. Подробности помогут понять, что нужно проверить.",
                    "Available apps have been kept. See the details to find out what needs checking."));
                collapsing_header(ui, t("Подробности", "Details")).show(ui, |ui| {
                    ui.add(
                        egui::Label::new(i18n::runtime(&self.catalog_warning))
                            .wrap()
                            .selectable(true),
                    );
                });
            });
        }
        self.catalog.update(
            &self.apps,
            &self.ranking,
            &self.filter,
            self.app_filter,
            self.app_sort,
            &self.highlighted_apps,
        );
        let match_count = self.catalog.indices.len();
        ui.small(format!(
            "{}: {} / {}",
            t("Найдено", "Matches"),
            match_count,
            self.apps.len()
        ));
        self.page_scroll_reset |= ui
            .push_id("catalog-pages-top", |ui| {
                page_controls(ui, &mut self.app_page, match_count)
            })
            .inner;
        if self.catalog.indices.is_empty() {
            card(ui, |ui| {
                ui.heading(if self.apps.is_empty() {
                    t("Список пока пуст", "No apps yet")
                } else {
                    t("Ничего не найдено", "No matches")
                });
                ui.label(if self.apps.is_empty() {
                    t("Нажмите «Найти установленные приложения» или добавьте нужный ярлык .lnk / .url.",
                      "Choose Find installed apps, or add a .lnk / .url shortcut.")
                } else {
                    t("Попробуйте другое имя или сбросьте фильтры.", "Try another name or clear the filters.")
                });
                if !self.apps.is_empty()
                    && ui
                        .button(t("Показать все приложения", "Show all apps"))
                        .clicked()
                {
                    self.filter.clear();
                    self.app_filter = 0;
                    self.app_page = 0;
                }
            });
            return;
        }
        let mut selected_policy = None;
        let mut details = None;
        let busy = self.busy();
        for &index in self
            .catalog
            .indices
            .iter()
            .skip(self.app_page * APPS_PER_PAGE)
            .take(APPS_PER_PAGE)
        {
            let app = &self.apps[index];
            let pending = self.pending_policy.as_ref().filter(|(id, _)| id == &app.id);
            ui.push_id(&app.id, |ui| {
                egui::Frame::group(ui.style())
                    .fill(SURFACE)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        let title = |ui: &mut egui::Ui| {
                            ui.add(
                                egui::Label::new(RichText::new(&app.name).size(20.0).strong())
                                    .wrap(),
                            );
                            ui.horizontal_wrapped(|ui| {
                                if self.state.owned.iter().any(|owned| owned.app == app.id) {
                                    ui.colored_label(
                                        ACCENT,
                                        t("На рабочем столе", "On the desktop"),
                                    );
                                }
                                if !app.valid {
                                    ui.colored_label(WARNING, t("Недоступно", "Unavailable"));
                                } else if app.pinned {
                                    ui.small(t("На панели задач", "On the taskbar"));
                                } else if self.highlighted_apps.contains(&app.id) {
                                    ui.colored_label(ACCENT, t("Выбрано", "Selected"));
                                } else {
                                    let category = match app.category.as_str() {
                                        "Manual" => t("Добавлено вами", "Added by you"),
                                        "Desktop" => t("Рабочий стол", "Desktop"),
                                        "Start menu" => t("Меню «Пуск»", "Start menu"),
                                        other => other,
                                    };
                                    if !category.is_empty() {
                                        ui.small(category);
                                    }
                                }
                            });
                        };
                        let mut controls = |ui: &mut egui::Ui| {
                            let mut policy = pending
                                .map_or_else(|| app.policy.clone(), |(_, policy)| policy.clone());
                            let before = policy.clone();
                            ui.add_enabled_ui(!busy, |ui| {
                                egui::ComboBox::from_id_salt("policy")
                                    .width(ui.available_width().min(260.0))
                                    .selected_text(policy_name(&policy))
                                    .show_ui(ui, |ui| {
                                        for value in [Policy::Auto, Policy::Always, Policy::Never] {
                                            let label = policy_name(&value);
                                            ui.selectable_value(&mut policy, value, label);
                                        }
                                    });
                            });
                            if policy != before {
                                selected_policy = Some((app.id.clone(), policy));
                            }
                            if ui.button(t("Подробнее", "Details")).clicked() {
                                details = Some(app.id.clone());
                            }
                        };
                        if ui.available_width() >= 720.0 {
                            ui.horizontal(|ui| {
                                let title_width = ui.available_width() - 430.0;
                                ui.allocate_ui_with_layout(
                                    egui::vec2(title_width, 0.0),
                                    egui::Layout::top_down(egui::Align::Min),
                                    |ui| {
                                        ui.set_min_width(title_width);
                                        title(ui);
                                    },
                                );
                                controls(ui);
                            });
                        } else {
                            title(ui);
                            if ui.available_width() >= 410.0 {
                                ui.horizontal(|ui| controls(ui));
                            } else {
                                controls(ui);
                            }
                        }
                        if pending.is_some() {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.colored_label(
                                    ACCENT,
                                    t("Сохраняем это правило…", "Saving this rule…"),
                                );
                            });
                        }
                    });
                ui.add_space(6.0);
            });
        }
        self.page_scroll_reset |= ui
            .push_id("catalog-pages-bottom", |ui| {
                page_controls(ui, &mut self.app_page, match_count)
            })
            .inner;
        if let Some((id, policy)) = selected_policy {
            self.change_policy(id, policy);
        }
        if let Some(id) = details {
            self.details_app = Some(id);
        }
    }
    fn app_details(&mut self, ctx: &egui::Context) {
        let Some(id) = self.details_app.clone() else {
            return;
        };
        let Some(app) = self.apps.iter().find(|app| app.id == id).cloned() else {
            self.details_app = None;
            return;
        };
        let ranked = self.ranking.iter().find(|ranked| ranked.id == id).cloned();
        let pending = self
            .pending_policy
            .as_ref()
            .filter(|(app_id, _)| app_id == &id);
        let mut policy = pending.map_or_else(|| app.policy.clone(), |(_, policy)| policy.clone());
        let before = policy.clone();
        let busy = self.busy();
        let mut error = None;
        let response = dialog_frame(
            ctx,
            &format!("app-details-{id}"),
            640.0,
            |ui| {
                let close = ui
                    .horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            primary(ui, true, t("Закрыть", "Close")).clicked()
                        })
                        .inner
                    })
                    .inner;
                ui.separator();
                close
            },
            |ui| {
                ui.heading(&app.name);
                ui.add_space(8.0);
                ui.label(t("Правило размещения", "Placement rule"));
                ui.add_enabled_ui(!busy, |ui| {
                    egui::ComboBox::from_id_salt("details-policy")
                        .width(ui.available_width().min(320.0))
                        .selected_text(policy_name(&policy))
                        .show_ui(ui, |ui| {
                            for value in [Policy::Auto, Policy::Always, Policy::Never] {
                                let label = policy_name(&value);
                                ui.selectable_value(&mut policy, value, label);
                            }
                        });
                });
                if pending.is_some() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(t("Сохраняем это правило…", "Saving this rule…"));
                    });
                }
                ui.small(t(
                "Правило сохраняется сразу. Следующий состав ярлыков можно проверить и применить на главной.",
                "The rule is saved immediately. Review and apply the next shortcut selection on Home.",
            ));
                if let Some(ranked) = &ranked {
                    ui.label(i18n::runtime(&ranked.reason));
                    ui.separator();
                    ui.heading(t("Использование", "Usage"));
                    ui.label(format!(
                        "{}: {:.1} · {}: {} · {}: {}",
                        t("Рейтинг", "Score"),
                        ranked.score,
                        t("Дней использования", "Active days"),
                        ranked.days,
                        t("Активных минут", "Active minutes"),
                        ranked.seconds / 60
                    ));
                    ui.small(format!(
                        "{}: {}",
                        t(
                            "Запросов запуска через быстрый ярлык",
                            "Launches through a quick shortcut"
                        ),
                        ranked.launches
                    ));
                }
                ui.separator();
                ui.heading(t("Исходный ярлык", "Original shortcut"));
                ui.add(egui::Label::new(&app.source).wrap().selectable(true));
                ui.horizontal_wrapped(|ui| {
                    if ui.button(t("Показать в папке", "Show in folder")).clicked()
                        && let Err(e) = native::show_in_folder(Path::new(&app.source))
                    {
                        error = Some(format!("{e:#}"));
                    }
                    if ui.button(t("Скопировать путь", "Copy path")).clicked() {
                        ctx.copy_text(app.source.clone());
                    }
                });
                if !app.target.is_empty() {
                    ui.label(t("Куда ведёт ярлык", "Shortcut target"));
                    ui.add(egui::Label::new(&app.target).wrap().selectable(true));
                }
                if !app.note.is_empty() {
                    ui.label(i18n::runtime(&app.note));
                }
                if let Some(notice) = &self.notice
                    && notice.kind == NoticeKind::Error
                {
                    ui.colored_label(ERROR, i18n::runtime(&notice.text));
                }
            },
        );
        if policy != before {
            self.change_policy(id, policy);
        }
        if response.inner || response.should_close() {
            self.details_app = None;
        }
        if let Some(error) = error {
            self.fail(error);
        }
    }
    fn preferences(&mut self, ui: &mut egui::Ui) {
        let sections = [
            t("Режим работы", "Mode"),
            t("Ярлыки и место", "Shortcuts & space"),
            t("Наблюдение", "Observation"),
            t("Тонкая настройка", "Fine tuning"),
            t("Данные", "Data"),
        ];
        if ui.available_width() < 700.0 {
            let show_title = ui.available_width() >= 520.0;
            ui.horizontal_wrapped(|ui| {
                if show_title {
                    ui.heading(RichText::new(t("Настройки", "Settings")).size(28.0));
                }
                egui::ComboBox::from_id_salt("settings-section")
                    .selected_text(sections[self.settings_section.min(sections.len() - 1)])
                    .width(250.0_f32.min(ui.available_width()))
                    .show_ui(ui, |ui| {
                        for (index, label) in sections.iter().enumerate() {
                            ui.selectable_value(&mut self.settings_section, index, *label);
                        }
                    });
            });
            ui.add_space(8.0);
        } else {
            ui.heading(RichText::new(t("Настройки", "Settings")).size(28.0));
            ui.label(t(
                "Настройте программу под себя. Изменения вступают в силу после сохранения.",
                "Make the app work your way. Changes take effect after you save.",
            ));
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                for (index, label) in sections.iter().enumerate() {
                    ui.selectable_value(&mut self.settings_section, index, *label);
                }
            });
            ui.add_space(14.0);
        }
        match self.settings_section {
            0 => {
                self.modes(ui);
                card(ui, |ui| {
                    ui.heading(t("Когда работать", "When to work"));
                    self.dirty |= ui
                        .checkbox(
                            &mut self.config.autostart,
                            t(
                                "Запускать при входе в Windows",
                                "Start when I sign in to Windows",
                            ),
                        )
                        .changed();
                    ui.label(t(
                        "После настройки окно можно закрыть. Программа доступна через значок в области уведомлений.",
                        "After setup, you can close this window. The app stays available from the notification area."
                    ));
                    if self.config.automatic {
                        ui.separator();
                        ui.label(t(
                            "Как часто пересматривать состав",
                            "How often to review the selection",
                        ));
                        let previous = self.config.interval_days;
                        ui.horizontal_wrapped(|ui| {
                            for (days, label) in [
                                (3, t("Раз в 3 дня", "Every 3 days")),
                                (7, t("Раз в неделю", "Weekly")),
                                (14, t("Раз в 2 недели", "Every 2 weeks")),
                                (30, t("Раз в 30 дней", "Every 30 days")),
                            ] {
                                ui.selectable_value(&mut self.config.interval_days, days, label);
                            }
                        });
                        if previous != self.config.interval_days {
                            self.refresh_dirty();
                        }
                        collapsing_header(ui, t("Другой интервал", "Custom interval")).show(
                            ui,
                            |ui| {
                                let mut days = self.config.interval_days;
                                if number(
                                    ui,
                                    t("Дней между пересмотрами", "Days between reviews"),
                                    &mut days,
                                    1,
                                    90,
                                ) {
                                    self.config.interval_days = days;
                                    self.refresh_dirty();
                                }
                            },
                        );
                        ui.small(t(
                            "Изменения выполняются в безопасный момент бездействия. Кнопка на главной позволяет применить их раньше.",
                            "Changes happen at a safe idle moment. The button on Home lets you apply them sooner."
                        ));
                    }
                });
            }
            1 => {
                card(ui, |ui| {
                    self.slots(ui);
                    ui.add_space(8.0);
                    self.dirty |= ui
                        .checkbox(
                            &mut self.config.respect_taskbar_pins,
                            t(
                                "Не дублировать приложения с панели задач",
                                "Skip apps already pinned to the taskbar",
                            ),
                        )
                        .changed();
                    ui.small(t(
                        "Правило «Всегда на столе» важнее этого ограничения.",
                        "The Always on desktop rule takes priority over this setting.",
                    ));
                });
                card(ui, |ui| {
                    ui.heading(t("Место для новых ярлыков", "Space for new shortcuts"));
                    self.zone(ui);
                });
                if self.layout.as_ref().is_some_and(|l| {
                    !self.state.topology.is_empty() && l.topology != self.state.topology
                }) {
                    card(ui, |ui| {
                        ui.colored_label(
                            WARNING,
                            t(
                                "Экран или размер значков изменился",
                                "The display or icon size changed",
                            ),
                        );
                        ui.label(t(
                            "Проверьте текущие позиции. Принятие новой геометрии сохранит их и разрешит следующие изменения.",
                            "Check the current positions. Accepting the new geometry keeps them and allows future changes."
                        ));
                        if primary(
                            ui,
                            !self.busy(),
                            t("Принять текущее размещение", "Accept current layout"),
                        )
                        .clicked()
                        {
                            self.queue("accept_geometry");
                        }
                    });
                }
            }
            2 => {
                card(ui, |ui| {
                    ui.heading(t("Перед первым размещением", "Before the first placement"));
                    self.learning_settings(ui);
                    if self.learning.completed_at > 0 {
                        ui.colored_label(ACCENT, t(
                            "Первоначальное наблюдение уже завершено. Изменение порогов не запускает его заново.",
                            "Initial observation is already complete. Changing these thresholds does not restart it."
                        ));
                    }
                });
                card(ui, |ui| {
                    ui.heading(t("Сбор статистики", "Usage statistics"));
                    self.dirty |= ui
                        .checkbox(
                            &mut self.config.collect,
                            t(
                                "Собирать локальную статистику приложений",
                                "Collect local app-usage statistics",
                            ),
                        )
                        .changed();
                    ui.label(t(
                        "Нужна для автоматического рейтинга. При выключении сохраняются уже накопленные данные и правила.",
                        "Statistics power the automatic ranking. Turning this off keeps your existing history and rules."
                    ));
                    if self.config.automatic && !self.config.collect {
                        ui.colored_label(WARNING, t(
                            "Автоматика включена, но новые наблюдения не поступают. Выбор будет основан на сохранённых данных.",
                            "Automation is on, but no new observations are collected. Selection will use saved data."
                        ));
                    }
                });
            }
            3 => {
                card(ui, |ui| {
                    ui.heading(t(
                        "Насколько часто менять привычный состав",
                        "Keep the selection stable",
                    ));
                    let mut changes = self.config.max_changes as i64;
                    if number(
                        ui,
                        t(
                            "Обычный предел замен за один пересмотр",
                            "Normal replacement limit per review",
                        ),
                        &mut changes,
                        1,
                        self.config.slots as i64,
                    ) {
                        self.config.max_changes = changes as usize;
                        self.refresh_dirty();
                    }
                    ui.small(t(
                        "Ручные правила и уменьшение числа мест могут потребовать больше изменений.",
                        "Manual rules and a reduction in slots may require more changes."
                    ));
                    let mut days = self.config.min_days as i64;
                    if number(
                        ui,
                        t(
                            "Минимум разных дней использования приложения",
                            "Minimum distinct days of app use",
                        ),
                        &mut days,
                        1,
                        14,
                    ) {
                        self.config.min_days = days as usize;
                        self.refresh_dirty();
                    }
                    ui.small(t(
                        "Защищает от случайных разовых запусков.",
                        "Avoids promoting apps after a one-off use.",
                    ));
                    ui.label(t(
                        "Насколько сильнее должен быть новый кандидат",
                        "How much stronger a new candidate must be",
                    ));
                    self.dirty |= ui
                        .add(
                            egui::Slider::new(&mut self.config.advantage, 1.1..=3.0)
                                .suffix("×")
                                .fixed_decimals(1),
                        )
                        .changed();
                    ui.small(t(
                        "Большее значение означает меньше перестановок.",
                        "A higher value means fewer replacements.",
                    ));
                });
                card(ui, |ui| {
                    ui.heading(t("Память об использовании", "Usage history weighting"));
                    ui.label(t(
                        "За сколько дней вес старых наблюдений уменьшается вдвое",
                        "Days before older observations lose half their weight",
                    ));
                    self.dirty |= ui
                        .add(
                            egui::Slider::new(&mut self.config.half_life_days, 3.0..=90.0)
                                .suffix(t(" дней", " days"))
                                .fixed_decimals(1),
                        )
                        .changed();
                    ui.small(t(
                        "Меньшее значение быстрее учитывает новые привычки.",
                        "A lower value responds faster to new habits.",
                    ));
                });
                card(ui, |ui| {
                    ui.heading(t(
                        "Когда перестать считать активность",
                        "When to stop counting activity",
                    ));
                    let mut seconds = i64::from(self.config.idle_seconds);
                    if number(
                        ui,
                        t(
                            "Секунд без взаимодействия с компьютером",
                            "Seconds without interacting with the computer",
                        ),
                        &mut seconds,
                        30,
                        900,
                    ) {
                        self.config.idle_seconds = seconds as u32;
                        self.refresh_dirty();
                    }
                    ui.small(t(
                        "Время бездействия не прибавляется к использованию приложения. Это отдельная настройка от момента размещения ярлыков.",
                        "Idle time does not count as app usage. This is separate from the idle condition used for placing shortcuts."
                    ));
                });
            }
            _ => {
                card(ui, |ui| {
                    ui.heading(t("Ваши данные", "Your data"));
                    ui.label(t(
                        "Дневная статистика приложений хранится 90 дней на этом компьютере. Текст окон, содержимое документов и нажатия клавиш не записываются.",
                        "Daily app-usage statistics are kept on this computer for 90 days. Window text, document contents and keystrokes are not recorded."
                    ));
                    ui.label(t("Папка программы и данных", "App and data folder"));
                    ui.add(
                        egui::Label::new(self.store.home.display().to_string())
                            .wrap()
                            .selectable(true),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("Открыть папку", "Open folder")).clicked()
                            && let Err(e) = native::open_file(&self.store.home)
                        {
                            self.fail(format!("{e:#}"));
                        }
                        if ui.button(t("Скопировать путь", "Copy path")).clicked() {
                            ui.ctx().copy_text(self.store.home.display().to_string());
                        }
                    });
                });
                card(ui, |ui| {
                    ui.heading(t(
                        "Начать статистику заново",
                        "Start fresh usage statistics",
                    ));
                    ui.label(t("Правила и текущие ярлыки сохранятся. Очистку статистики отменить невозможно.",
                        "Your rules and current shortcuts are kept. Clearing usage statistics cannot be undone."));
                    if ui
                        .add_enabled(
                            !self.busy(),
                            egui::Button::new(t("Очистить статистику…", "Clear usage statistics…")),
                        )
                        .clicked()
                    {
                        self.confirm_clear = true;
                    }
                });
                card(ui, |ui| {
                    ui.heading(t("Отключить программу", "Turn off Desktop Pilot"));
                    ui.label(t(
                        "Остановить фоновую работу и убрать автозапуск. Сбор статистики и автоматические изменения отключатся. Текущие ярлыки и данные сохранятся.",
                        "Stop background work and turn off startup, statistics and automatic changes. Your current shortcuts and data will be kept.",
                    ));
                    if ui
                        .add_enabled(
                            self.job.is_none() && !self.dirty,
                            egui::Button::new(t(
                                "Отключить Desktop Pilot…",
                                "Disable Desktop Pilot…",
                            )),
                        )
                        .clicked()
                    {
                        self.confirm_disable = true;
                    }
                    if self.dirty {
                        ui.small(t(
                            "Сначала сохраните или отмените текущие правки.",
                            "Save or discard your current edits first.",
                        ));
                    }
                });
                card(ui, |ui| {
                    ui.heading("Desktop Pilot");
                    ui.label(format!(
                        "{} {}",
                        t("Версия", "Version"),
                        env!("CARGO_PKG_VERSION")
                    ));
                    ui.hyperlink_to(
                        t("Руководство", "User guide"),
                        if self.config.language == "ru" {
                            "https://github.com/TechWisard/desktop-pilot/blob/main/docs/guide.ru.md"
                        } else {
                            "https://github.com/TechWisard/desktop-pilot/blob/main/docs/guide.md"
                        },
                    );
                    ui.hyperlink_to(
                        t("Обновления", "Releases"),
                        "https://github.com/TechWisard/desktop-pilot/releases",
                    );
                });
            }
        }
    }
    fn history(&mut self, ui: &mut egui::Ui) {
        ui.heading(RichText::new(t("История", "History")).size(28.0));
        ui.label(t(
            "Изменения ярлыков и результаты ваших действий.",
            "Shortcut changes and the results of your actions.",
        ));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.history_errors_only,
                t("Только ошибки и предупреждения", "Only errors and warnings"),
            );
            if ui.button(t("Обновить", "Refresh")).clicked()
                && let Err(e) = self.reload()
            {
                self.fail(format!("{e:#}"));
            }
        });
        ui.small(t(
            "Последние 100 событий. Время указано по настройкам Windows.",
            "Most recent 100 events. Times follow your Windows time zone.",
        ));
        ui.add_space(10.0);
        if !self.events_error.is_empty() {
            ui.colored_label(ERROR, i18n::runtime(&self.events_error));
            return;
        }
        let events: Vec<_> = self
            .events
            .iter()
            .filter(|(_, kind, _)| {
                !self.history_errors_only || matches!(kind.as_str(), "error" | "warning")
            })
            .collect();
        if events.is_empty() {
            card(ui, |ui| {
                ui.heading(if self.history_errors_only {
                    t("Ошибок и предупреждений нет", "No errors or warnings")
                } else {
                    t("История пока пуста", "No history yet")
                });
                ui.label(t(
                    "Новые события появятся здесь после действий программы.",
                    "New events will appear here as the app works.",
                ));
            });
        }
        for (at, kind, message) in events {
            card(ui, |ui| {
                let label = match kind.as_str() {
                    "setup" | "settings" => t("Настройка", "Setup"),
                    "layout" => t("Размещение", "Placement"),
                    "manual" => t("Ручное изменение", "Manual change"),
                    "rollback" => t("Отмена изменения", "Undo"),
                    "done" => t("Готово", "Completed"),
                    "error" => t("Ошибка", "Error"),
                    "warning" => t("Предупреждение", "Warning"),
                    _ => kind,
                };
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(
                        match kind.as_str() {
                            "error" => ERROR,
                            "warning" => WARNING,
                            _ => ACCENT,
                        },
                        label,
                    );
                    ui.small(when(*at));
                });
                ui.add(
                    egui::Label::new(i18n::runtime(message))
                        .wrap()
                        .selectable(true),
                );
            });
        }
    }
    fn footer(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(4.0);
        if self.onboarding {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "{} {} / 3",
                    t("Шаг", "Step"),
                    self.onboarding_step + 1
                ));
                if self.onboarding_step > 0
                    && ui
                        .add_enabled(self.job.is_none(), egui::Button::new(t("Назад", "Back")))
                        .clicked()
                {
                    self.onboarding_step -= 1;
                }
                if self.onboarding_step < 2 {
                    if primary(ui, self.job.is_none(), t("Продолжить", "Continue")).clicked()
                    {
                        self.onboarding_step += 1;
                    }
                } else if primary(
                    ui,
                    !self.busy(),
                    t("Начать работу", "Start using Desktop Pilot"),
                )
                .clicked()
                {
                    self.finish_setup();
                }
            });
        } else if self.dirty {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(
                    WARNING,
                    t("Есть несохранённые изменения", "Unsaved changes"),
                );
                if primary(ui, !self.busy(), t("Сохранить", "Save"))
                    .on_hover_text("Ctrl+S")
                    .clicked()
                {
                    self.save();
                }
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(t("Отменить правки", "Discard edits")),
                    )
                    .clicked()
                {
                    self.discard();
                }
            });
        }
        if let Some(job) = &self.job {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(i18n::runtime(job.label));
            });
            if job.started.elapsed() > Duration::from_secs(15) {
                ui.small(t("Операция ещё выполняется. Настройки и данные сохраняются; дождитесь результата.",
                    "The operation is still running. Your settings and data are kept; please wait for the result."));
            }
        } else if !self.pending_commands.is_empty() || self.active_commands > 0 {
            ui.horizontal_wrapped(|ui| {
                ui.spinner();
                ui.label(t("Выполняется команда…", "Running your command…"));
                if let Some(command) = self.pending_commands.first() {
                    let label = match command.action.as_str() {
                        "apply" => t("Применение", "Apply"),
                        "rollback" => t("Отмена", "Undo"),
                        "rescan" => t("Поиск приложений", "Find apps"),
                        "clear_stats" => t("Очистка статистики", "Clear statistics"),
                        "accept_geometry" => t("Принятие размещения", "Accept layout"),
                        _ => t("Сохранение правила", "Save rule"),
                    };
                    ui.small(label);
                    if command.submitted.elapsed() > Duration::from_secs(30) {
                        ui.colored_label(
                            WARNING,
                            t(
                                "Это дольше обычного. Проверьте состояние на главной.",
                                "This is taking longer than usual. Check the status on Home.",
                            ),
                        );
                    }
                }
            });
        }
        if let Some(notice) = &self.notice {
            let color = match notice.kind {
                NoticeKind::Info => MUTED,
                NoticeKind::Success => ACCENT,
                NoticeKind::Error => ERROR,
            };
            let message = i18n::runtime(&notice.text);
            let limit = if ctx.screen_rect().width() < 700.0 {
                55
            } else {
                120
            };
            let first_line = message.lines().next().unwrap_or_default();
            let shortened = message.contains('\n') || first_line.chars().count() > limit;
            let summary = if shortened {
                format!("{}…", first_line.chars().take(limit).collect::<String>())
            } else {
                first_line.to_owned()
            };
            let mut dismiss = false;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(egui::Button::new("×").min_size(egui::vec2(38.0, 38.0)))
                    .on_hover_text(t("Закрыть сообщение", "Dismiss message"))
                    .clicked()
                {
                    dismiss = true;
                }
                if (notice.kind == NoticeKind::Error || shortened)
                    && ui.button(t("Подробнее", "Details")).clicked()
                {
                    self.notice_details = true;
                }
                ui.colored_label(color, &summary);
            });
            if dismiss {
                self.notice = None;
                self.notice_details = false;
            }
        }
        if self.job.is_none()
            && self.pending_commands.is_empty()
            && !self.dirty
            && self.notice.is_none()
        {
            ui.small(t(
                "Работает на этом компьютере · Исходные ярлыки сохраняются",
                "Runs on this computer · Original shortcuts are kept",
            ));
        }
        ui.add_space(4.0);
    }
    fn import_dialog(&mut self, ctx: &egui::Context) {
        let response = dialog(
            ctx,
            "import-shortcuts",
            560.0,
            t("Добавить ярлыки", "Add shortcuts"),
            |ui| {
                ui.label(t(
                    "Выберите ярлыки .lnk или .url. Полные пути можно вставить по одному в строке.",
                    "Choose .lnk or .url shortcuts. You can paste full paths, one per line.",
                ));
                ui.add_enabled_ui(self.job.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (from_start, label) in [
                        (false, t("Выбрать файлы…", "Browse files…")),
                        (true, t("Из меню «Пуск»…", "From Start menu…")),
                    ] {
                        if ui.button(label).clicked() {
                            let mut picker = rfd::FileDialog::new()
                                .set_title(t("Выберите один или несколько ярлыков", "Choose one or more shortcuts"))
                                .add_filter(t("Ярлыки", "Shortcuts"), &["lnk", "url"]);
                            if let Ok(directory) = if from_start { native::programs() } else { native::desktop() } {
                                picker = picker.set_directory(directory);
                            }
                            if let Some(paths) = picker.pick_files() {
                                self.open_import(paths);
                            }
                        }
                    }
                });
                ui.label(t("Полные пути к ярлыкам", "Full shortcut paths"));
                let editor = ui.add(egui::TextEdit::multiline(&mut self.import_paths)
                    .id_salt("import-paths")
                    .hint_text(r"C:\Users\…\Desktop\App.lnk")
                    .desired_width(f32::INFINITY)
                    .desired_rows(2));
                if self.focus_import {
                    editor.request_focus();
                    self.focus_import = false;
                }
                if editor.changed() {
                    self.import_error.clear();
                }
                ui.horizontal_wrapped(|ui| {
                    if primary(ui, !self.busy() && !self.import_paths.trim().is_empty(), t("Добавить в список", "Add to the list")).clicked() {
                        match shortcut_paths(&self.import_paths) {
                            Ok(paths) => {
                                self.import_error.clear();
                                self.add_shortcuts(paths);
                            }
                            Err(error) => self.import_error = format!("{error:#}"),
                        }
                    }
                    if ui.button(t("Отмена", "Cancel")).clicked() {
                        self.import_open = false;
                    }
                });
                collapsing_header(ui, t("Какие файлы можно добавить?", "Which files can I add?")).show(ui, |ui| {
                    ui.small(t(
                        "Если у программы нет ярлыка, создайте его в Windows и выберите здесь. Можно выбрать несколько файлов или перетащить их в окно. Добавление не запускает приложение и не перемещает исходный файл.",
                        "If an app has no shortcut, create one in Windows and choose it here. You can choose several files or drag them into the window. Adding does not launch the app or move the original file.",
                    ));
                });
            });
                if self.job.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(t(
                            "Проверяем и добавляем ярлыки…",
                            "Checking and adding shortcuts…",
                        ));
                    });
                }
                if !self.import_error.is_empty() {
                    ui.colored_label(
                        WARNING,
                        t("Проверьте выбранные файлы", "Check the selected files"),
                    );
                    ui.add(
                        egui::Label::new(i18n::runtime(&self.import_error))
                            .wrap()
                            .selectable(true),
                    );
                }
            },
        );
        if response.should_close() && self.job.is_none() {
            self.import_open = false;
        }
    }
    fn close_setup(&mut self, ctx: &egui::Context, keep: bool) {
        let result = self.store.transaction(|| {
            self.store
                .set("setup_draft", &keep.then_some(&self.config))?;
            self.store
                .set("setup_step", &if keep { self.onboarding_step } else { 0 })
        });
        match result {
            Ok(()) => {
                self.confirm_setup_close = false;
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => self.fail(format!("{error:#}")),
        }
    }
    fn confirmations(&mut self, ctx: &egui::Context) {
        if self.details_app.is_some() {
            self.app_details(ctx);
        }
        if self.notice_details {
            let message = self
                .notice
                .as_ref()
                .map(|notice| i18n::runtime(&notice.text))
                .unwrap_or_default();
            let response = dialog(
                ctx,
                "notice-details",
                560.0,
                t("Сообщение программы", "App message"),
                |ui| {
                    ui.add(egui::Label::new(&message).wrap().selectable(true));
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(t("Скопировать сообщение", "Copy message"))
                            .clicked()
                        {
                            ctx.copy_text(message.clone());
                        }
                        if primary(ui, true, t("Закрыть", "Close")).clicked() {
                            self.notice_details = false;
                        }
                    });
                },
            );
            if response.should_close() {
                self.notice_details = false;
            }
        }
        if self.confirm_clear {
            let response = dialog(
                ctx,
                "clear-confirmation",
                480.0,
                t(
                    "Очистить статистику использования?",
                    "Clear usage statistics?",
                ),
                |ui| {
                    ui.label(t(
                    "Рейтинг начнёт накапливаться заново. Если наблюдение ещё не завершено, его прогресс сбросится. Ваши правила и ярлыки сохранятся.",
                    "The ranking will start building up again. Unfinished observation will reset. Your rules and shortcuts will be kept."
                ));
                    ui.colored_label(
                        WARNING,
                        t("Очистку нельзя отменить.", "Clearing cannot be undone."),
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(t("Сохранить статистику", "Keep statistics"))
                            .clicked()
                        {
                            self.confirm_clear = false;
                        }
                        if ui
                            .add_enabled(
                                !self.busy(),
                                egui::Button::new(t("Очистить", "Clear"))
                                    .fill(Color32::from_rgb(112, 43, 52))
                                    .min_size(egui::vec2(0.0, 42.0)),
                            )
                            .clicked()
                        {
                            self.confirm_clear = false;
                            self.queue("clear_stats");
                        }
                    });
                },
            );
            if response.should_close() {
                self.confirm_clear = false;
            }
        }
        if self.confirm_close {
            let response = dialog(
                ctx,
                "close-confirmation",
                520.0,
                t(
                    "Сохранить изменения перед закрытием?",
                    "Save changes before closing?",
                ),
                |ui| {
                    ui.label(t(
                        "В настройках есть несохранённые правки.",
                        "Your settings contain unsaved edits.",
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if primary(ui, !self.busy(), t("Сохранить и закрыть", "Save and close"))
                            .clicked()
                        {
                            self.confirm_close = false;
                            self.close_after_save = true;
                            self.save();
                        }
                        if ui
                            .add_enabled(
                                self.job.is_none(),
                                egui::Button::new(t(
                                    "Закрыть без сохранения",
                                    "Close without saving",
                                )),
                            )
                            .clicked()
                        {
                            self.confirm_close = false;
                            self.allow_close = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui
                            .button(t("Продолжить настройку", "Keep editing"))
                            .clicked()
                        {
                            self.confirm_close = false;
                        }
                    });
                },
            );
            if response.should_close() {
                self.confirm_close = false;
            }
        }
        if self.confirm_setup_close {
            let response = dialog(
                ctx,
                "setup-close-confirmation",
                500.0,
                t("Продолжить настройку позже?", "Continue setup later?"),
                |ui| {
                    ui.label(t(
                    "Можно сохранить выбранные параметры и вернуться к этому шагу при следующем запуске. Пока настройка не завершена, программа не работает в фоне и не меняет ярлыки.",
                    "Save your choices and return to this step next time. Until setup is complete, the app does not run in the background or change shortcuts.",
                ));
                    ui.horizontal_wrapped(|ui| {
                        if primary(
                            ui,
                            self.job.is_none(),
                            t("Сохранить и выйти", "Save and exit"),
                        )
                        .clicked()
                        {
                            self.close_setup(ctx, true);
                        }
                        if ui
                            .button(t("Продолжить настройку", "Keep setting up"))
                            .clicked()
                        {
                            self.confirm_setup_close = false;
                        }
                        if ui
                            .button(t("Выйти без сохранения", "Exit without saving"))
                            .clicked()
                        {
                            self.close_setup(ctx, false);
                        }
                    });
                },
            );
            if response.should_close() {
                self.confirm_setup_close = false;
            }
        }
        if self.confirm_disable {
            let response = dialog(
                ctx,
                "disable-confirmation",
                500.0,
                t("Отключить Desktop Pilot?", "Turn off Desktop Pilot?"),
                |ui| {
                    ui.label(t(
                    "Фоновая работа остановится. Сбор статистики, автоматические изменения и автозапуск будут отключены. Текущие ярлыки, правила и накопленные данные сохранятся.",
                    "Background work will stop. Statistics, automatic changes and startup will be turned off. Your current shortcuts, rules and usage history will be kept.",
                ));
                    ui.label(t(
                        "Вернуться к работе можно, открыв Desktop Pilot и выбрав нужный режим.",
                        "To use it again, open Desktop Pilot and choose the mode you want.",
                    ));
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .button(t("Оставить включённым", "Keep running"))
                            .clicked()
                        {
                            self.confirm_disable = false;
                        }
                        if primary(ui, self.job.is_none(), t("Отключить", "Turn off")).clicked()
                        {
                            self.confirm_disable = false;
                            self.disable_background();
                        }
                    });
                },
            );
            if response.should_close() {
                self.confirm_disable = false;
            }
        }
        if self.import_open {
            self.import_dialog(ctx);
        }
    }
    fn screenshots(&mut self, ctx: &egui::Context) {
        if let Some(directory) = &self.capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    let name = if let Some(view) = &self.capture_view {
                        format!("dialog-{view}.png")
                    } else if self.onboarding {
                        format!("setup-{}.png", self.onboarding_step)
                    } else if self.tab == 2 {
                        format!("settings-2-{}.png", self.settings_section)
                    } else {
                        format!("settings-{}.png", self.tab)
                    };
                    if let Err(e) = image::save_buffer(
                        directory.join(name),
                        &bytes,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        self.notice = Some(Notice {
                            text: e.to_string(),
                            kind: NoticeKind::Error,
                        });
                    }
                    self.capture_pending = false;
                    if self.capture_view.is_some()
                        || (self.onboarding && self.onboarding_step == 2)
                        || (!self.onboarding && self.tab == 3)
                    {
                        self.capture_pending = true;
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        if self.onboarding {
                            self.onboarding_step += 1;
                        } else if self.tab == 2 && self.settings_section < 4 {
                            self.settings_section += 1;
                        } else {
                            self.tab += 1;
                        }
                        self.capture_at = Instant::now() + Duration::from_millis(500);
                    }
                }
            }
            if Instant::now() >= self.capture_at && !self.capture_pending {
                self.capture_pending = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}

impl eframe::App for Settings {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        i18n::set(&self.config.language);
        if let Some(scale) = requested_scale(ctx, self.ui_scale) {
            self.set_scale(ctx, scale);
        }
        self.poll_jobs(ctx);
        self.screenshots(ctx);
        ctx.request_repaint_after(Duration::from_secs(2));
        if self.last.elapsed() > Duration::from_secs(2) && self.job.is_none() {
            self.last = Instant::now();
            if let Err(e) = self.reload() {
                self.fail(format!("{e:#}"));
            }
        }
        self.refresh_dirty();
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close {
            if self.job.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.inform(
                    "Please wait for the current operation to finish before closing.",
                    NoticeKind::Info,
                );
            } else if self.onboarding {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_setup_close = true;
            } else if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
                self.confirm_clear = false;
                self.confirm_disable = false;
                self.import_open = false;
                self.notice_details = false;
                self.details_app = None;
            }
        }
        if !self.modal_open()
            && !self.onboarding
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
        {
            self.save();
        }
        if !self.modal_open() && !self.onboarding && self.job.is_none() {
            if ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
                self.tab = 1;
                self.focus_search = true;
                self.page_scroll_reset = true;
            }
            if !self.busy()
                && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::O))
            {
                self.open_import([]);
            }
            if self.tab == 1 {
                if !self.busy()
                    && ctx
                        .input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::F5))
                {
                    self.queue("rescan");
                }
                let previous = self.app_page;
                let pages = self.catalog.indices.len().div_ceil(APPS_PER_PAGE).max(1);
                if ctx.input_mut(|input| {
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::PageUp)
                }) {
                    self.app_page = self.app_page.saturating_sub(1);
                }
                if ctx.input_mut(|input| {
                    input.consume_key(egui::Modifiers::COMMAND, egui::Key::PageDown)
                }) {
                    self.app_page = (self.app_page + 1).min(pages - 1);
                }
                self.page_scroll_reset |= previous != self.app_page;
            }
        }
        let dropped: Vec<_> = ctx.input(|input| {
            input
                .raw
                .dropped_files
                .iter()
                .filter_map(|file| file.path.clone())
                .collect()
        });
        if !dropped.is_empty() {
            if !self.onboarding && !self.busy() && (!self.modal_open() || self.import_open) {
                self.open_import(dropped);
            } else {
                self.inform(
                    "Finish the current step or operation, then drop the shortcuts again.",
                    NoticeKind::Info,
                );
            }
        }
        let interactive = !self.modal_open() && self.job.is_none();
        let navigation_enabled = !self.modal_open();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(19, 28, 39))
                    .inner_margin(egui::Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.available_width() >= 460.0 {
                        ui.heading(RichText::new("Desktop Pilot").size(27.0).strong());
                        ui.add_space(12.0);
                    }
                    ui.add_enabled_ui(interactive, |ui| self.language_and_scale(ui, ctx));
                });
            });
        let show_footer = self.onboarding
            || self.dirty
            || self.job.is_some()
            || !self.pending_commands.is_empty()
            || self.active_commands > 0
            || self.notice.is_some()
            || ctx.screen_rect().height() >= 600.0;
        if show_footer {
            egui::TopBottomPanel::bottom("footer")
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(19, 28, 39))
                        .inner_margin(egui::Margin::symmetric(18, 8)),
                )
                .show(ctx, |ui| {
                    // Progress and error details stay readable while an operation is running.
                    ui.add_enabled_ui(!self.modal_open(), |ui| {
                        let height = (ctx.screen_rect().height() * 0.32).clamp(90.0, 210.0);
                        ui.set_max_height(height);
                        egui::ScrollArea::vertical()
                            .id_salt("footer-scroll")
                            .max_height(height)
                            .show(ui, |ui| self.footer(ui, ctx));
                    });
                });
        }
        let wide = ctx.screen_rect().width() >= 920.0;
        if !self.onboarding && wide {
            egui::SidePanel::left("navigation")
                .resizable(false)
                .exact_width(196.0)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from_rgb(19, 28, 39))
                        .inner_margin(14.0),
                )
                .show(ctx, |ui| {
                    ui.add_space(12.0);
                    ui.add_enabled_ui(navigation_enabled, |ui| self.navigation(ui, true));
                    ui.add_space(24.0);
                    ui.small(t("СЕЙЧАС", "CURRENTLY"));
                    let current = self.current_health();
                    ui.colored_label(health_color(current), health_label(current));
                    ui.add_space(8.0);
                    ui.small(format!("v{}", env!("CARGO_PKG_VERSION")));
                });
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(22, 31, 43))
                    .inner_margin(20.0),
            )
            .show(ctx, |ui| {
                if !self.onboarding && !wide {
                    ui.add_enabled_ui(navigation_enabled, |ui| {
                        ui.horizontal_wrapped(|ui| self.navigation(ui, false));
                    });
                    ui.separator();
                }
                let section = if self.onboarding {
                    self.onboarding_step
                } else if self.tab == 2 {
                    self.settings_section
                } else if self.tab == 1 {
                    self.app_page
                } else {
                    0
                };
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                ui.spacing_mut().scroll.bar_width = 10.0;
                let mut scroll = egui::ScrollArea::vertical()
                    .id_salt(("page-scroll", self.onboarding, self.tab, section))
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.capture_scroll_bottom);
                if std::mem::take(&mut self.page_scroll_reset) {
                    scroll = scroll.vertical_scroll_offset(0.0);
                }
                scroll.show(ui, |ui| {
                    constrain_page_width(ui);
                    ui.add_enabled_ui(interactive, |ui| {
                        if self.onboarding {
                            self.welcome(ui);
                        } else {
                            match self.tab {
                                0 => self.overview(ui),
                                1 => self.applications(ui),
                                2 => self.preferences(ui),
                                _ => self.history(ui),
                            }
                        }
                    });
                    ui.add_space(14.0);
                });
            });
        self.confirmations(ctx);
    }
}

pub fn run(home: PathBuf, capture: Option<PathBuf>) -> Result<()> {
    let mut app = Settings::new(home)?;
    if let Some(path) = &capture {
        std::fs::create_dir_all(path)?;
    }
    app.capture = capture;
    if app.capture.is_some() {
        app.capture_scroll_bottom = app.store.get::<bool>("test_scroll_bottom")?;
        let view = app.store.get::<String>("test_view")?;
        match view.as_str() {
            "apps" | "catalog-last" | "catalog-search" => {
                app.tab = 1;
                if view == "catalog-last" {
                    app.app_page = app.apps.len().saturating_sub(1) / APPS_PER_PAGE;
                } else if view == "catalog-search" {
                    app.filter = app.store.get("test_search")?;
                }
            }
            "app-details" | "policy-pending" => {
                app.tab = 1;
                let requested: String = app.store.get("test_app_id")?;
                let id = app
                    .apps
                    .iter()
                    .find(|app| app.id == requested)
                    .or_else(|| app.apps.first())
                    .map(|app| app.id.clone());
                if view == "app-details" {
                    app.details_app = id;
                } else if let Some(id) = id {
                    app.pending_policy = Some((id, Policy::Always));
                    app.active_commands = 1;
                }
            }
            "import" => {
                app.tab = 1;
                app.import_open = true;
                app.import_paths = app.store.get("test_import_paths")?;
            }
            "disable" => app.confirm_disable = true,
            "close" => app.confirm_close = true,
            "setup-close" => app.confirm_setup_close = true,
            "clear" => app.confirm_clear = true,
            "notice" => {
                app.fail(app.store.get::<String>("test_notice")?);
                app.notice_details = true;
            }
            "feedback" => {
                app.fail(app.store.get::<String>("test_notice")?);
                app.config.slots = if app.config.slots == 8 {
                    7
                } else {
                    app.config.slots + 1
                };
                app.config.max_changes = app.config.max_changes.min(app.config.slots);
                app.tab = 2;
            }
            _ => {}
        }
        if app.modal_open()
            || matches!(
                view.as_str(),
                "feedback" | "apps" | "catalog-last" | "catalog-search" | "policy-pending"
            )
        {
            app.capture_view = Some(view);
        }
    }
    if app.capture.is_some()
        && !app.store.get::<String>("test_desktop")?.is_empty()
        && let Some(layout) = app.store.get::<Option<native::Layout>>("test_layout")?
    {
        // An explicit smoke-test profile can supply a diagram without querying Explorer.
        app.layout_receiver = None;
        app.find_after_layout = false;
        app.layout_error.clear();
        app.layout = Some(layout);
    }
    // The running worker may belong to the previous release. The installer
    // checks the executable first and leaves a matching worker untouched.
    if app.capture.is_none() && !app.onboarding {
        app.prepare_background();
    }
    let scale = app.ui_scale;
    let test_viewport = if app.capture.is_some() {
        app.store
            .get::<Option<[f32; 2]>>("test_viewport")?
            .filter(|size| {
                size[0].is_finite()
                    && size[1].is_finite()
                    && (360.0..=3840.0).contains(&size[0])
                    && (320.0..=2160.0).contains(&size[1])
            })
    } else {
        None
    };
    let icon = image::load_from_memory(include_bytes!("../assets/desktop-pilot.png"))?.into_rgba8();
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size(test_viewport.unwrap_or([1120.0, 820.0]))
        .with_min_inner_size(if test_viewport.is_some() {
            [360.0, 320.0]
        } else {
            [720.0, 540.0]
        })
        .with_icon(std::sync::Arc::new(egui::IconData {
            width: icon.width(),
            height: icon.height(),
            rgba: icon.into_raw(),
        }));
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    eframe::run_native(
        "Desktop Pilot",
        options,
        Box::new(move |cc| {
            configure_style(&cc.egui_ctx);
            cc.egui_ctx.set_zoom_factor(scale);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Settings window: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_light_theme_does_not_replace_the_product_style() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        for system_theme in [egui::Theme::Light, egui::Theme::Dark, egui::Theme::Light] {
            let _ = ctx.run(
                egui::RawInput {
                    system_theme: Some(system_theme),
                    ..Default::default()
                },
                |ctx| {
                    assert_eq!(ctx.theme(), egui::Theme::Dark);
                    let style = ctx.style();
                    assert!(style.visuals.dark_mode);
                    assert_eq!(
                        style.visuals.override_text_color,
                        Some(Color32::from_rgb(234, 239, 245))
                    );
                    assert_eq!(style.text_styles[&egui::TextStyle::Body].size, 18.0);
                    assert!(style.spacing.interact_size.y >= 38.0);
                },
            );
        }
        for theme in [egui::Theme::Dark, egui::Theme::Light] {
            assert_eq!(
                ctx.style_of(theme).text_styles[&egui::TextStyle::Body].size,
                18.0
            );
        }
    }

    #[test]
    fn cards_remain_inside_narrow_and_wide_viewports() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        for width in [420.0, 650.0, 900.0, 1120.0] {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))),
                ..Default::default()
            }, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx, |ui| {
                    let right = ui.max_rect().right();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        constrain_page_width(ui);
                        card(ui, |ui| {
                            ui.label("Длинное пояснение к настройке должно переноситься внутри карточки и оставаться полностью доступным даже в узком окне.");
                            let _ = ui.button("Понятное действие");
                        });
                        assert!(ui.min_rect().right() <= right + 1.0, "card exceeds {width}-point viewport: {:?}", ui.min_rect());
                    });
                });
            });
        }
    }

    #[test]
    fn stopped_worker_failure_is_visible_but_stale_and_intentional_stops_are_not() {
        let rus = "Фоновый учёт остановлен: database is locked\nAutostart warning";
        let eng = "Background tracking stopped: failed to save usage";
        assert_eq!(
            stopped_worker_error(false, false, rus),
            Some("Фоновый учёт остановлен: database is locked")
        );
        assert_eq!(stopped_worker_error(false, false, eng), Some(eng));
        assert_eq!(stopped_worker_error(true, false, rus), None);
        assert_eq!(stopped_worker_error(false, true, rus), None);
        assert_eq!(
            stopped_worker_error(false, false, "Фоновый учёт работает"),
            None
        );
        assert_eq!(
            stopped_worker_error(false, false, "Changes paused: source missing"),
            None
        );
        assert_eq!(stopped_worker_error(false, false, ""), None);
    }

    #[test]
    fn saved_state_overrides_a_stale_status_message() {
        let mut config = Config {
            automatic: true,
            collect: true,
            learning_enabled: false,
            ..Config::default()
        };
        let progress = Progress::default();
        assert_eq!(
            health(false, 100, 110, "Фоновый учёт работает", &config, &progress),
            Health::Offline
        );
        assert_eq!(
            health(true, 100, 251, "Фоновый учёт работает", &config, &progress),
            Health::Unresponsive
        );
        assert_eq!(
            health(true, 100, 101, "Changes paused: failed", &config, &progress),
            Health::Attention
        );
        assert_eq!(
            health(true, 100, 101, "", &config, &progress),
            Health::Automatic
        );
        config.automatic = false;
        assert_eq!(
            health(true, 100, 101, "", &config, &progress),
            Health::Suggestions
        );
        config.collect = false;
        assert_eq!(
            health(true, 100, 101, "", &config, &progress),
            Health::Manual
        );
    }

    #[test]
    fn an_observation_pause_is_not_reported_as_active_learning() {
        let config = Config {
            automatic: true,
            collect: false,
            ..Config::default()
        };
        assert_eq!(
            health(true, 100, 101, "", &config, &Progress::default()),
            Health::ObservationPaused
        );
    }

    #[test]
    fn saving_unrelated_edits_preserves_a_background_pause() {
        let base = Config {
            automatic: true,
            ..Config::default()
        };
        let draft = Config {
            slots: 6,
            ..base.clone()
        };
        let latest = Config {
            automatic: false,
            ..base.clone()
        };
        let result = merge_config(&base, &draft, &latest).unwrap();
        assert_eq!(result.slots, 6);
        assert!(!result.automatic);
    }

    #[test]
    fn conflicting_settings_are_not_silently_overwritten() {
        let base = Config::default();
        let draft = Config {
            slots: 6,
            ..base.clone()
        };
        let latest = Config {
            slots: 8,
            ..base.clone()
        };
        assert!(merge_config(&base, &draft, &latest).is_err());
        assert!(merge_config(&base, &draft, &draft).is_ok());
    }

    #[test]
    fn changes_that_become_invalid_after_merge_are_rejected() {
        let base = Config {
            slots: 8,
            max_changes: 1,
            ..Config::default()
        };
        let draft = Config {
            max_changes: 8,
            ..base.clone()
        };
        let latest = Config {
            slots: 2,
            ..base.clone()
        };
        assert!(merge_config(&base, &draft, &latest).is_err());
    }

    #[test]
    fn running_commands_are_not_mistaken_for_finished_commands() {
        assert!(command_in_progress("pending"));
        assert!(command_in_progress("running"));
        assert!(!command_in_progress("done"));
        assert!(!command_in_progress("error"));
    }

    #[test]
    fn manual_mode_can_apply_without_waiting_for_disabled_collection() {
        let mut config = Config::default();
        apply_mode(&mut config, OperatingMode::Manual);
        assert!(!config.automatic && !config.collect);
        assert!(Progress::default().ready(&config));
        assert_eq!(operating_mode(&config), OperatingMode::Manual);
    }

    #[test]
    fn switching_modes_preserves_the_users_placement_and_limits() {
        let mut config = Config {
            slots: 7,
            zone_col: 12,
            zone_row: 8,
            interval_days: 27,
            ..Config::default()
        };
        apply_mode(&mut config, OperatingMode::Observe);
        assert!(!config.automatic && config.collect);
        apply_mode(&mut config, OperatingMode::Adaptive);
        assert!(config.automatic && config.collect);
        assert_eq!(
            (
                config.slots,
                config.zone_col,
                config.zone_row,
                config.interval_days
            ),
            (7, 12, 8, 27)
        );
    }

    #[test]
    fn search_is_case_insensitive_tokenized_and_includes_paths() {
        let app = App {
            name: "Мой редактор".into(),
            source: r"C:\Работа\Tools\Editor.lnk".into(),
            ..App::default()
        };
        let apps = vec![app];
        let mut catalog = CatalogCache::default();
        catalog.rebuild(&apps, &[]);
        for (query, expected) in [("  РЕДАКТОР  tools ", 1), ("", 1), ("редактор unknown", 0)]
        {
            catalog.update(&apps, &[], query, 0, 0, &HashSet::new());
            assert_eq!(catalog.indices.len(), expected);
        }
    }

    #[test]
    fn invalid_ui_scale_cannot_make_the_window_unusable() {
        for scale in [f32::NAN, f32::INFINITY, -1.0, 0.0, 20.0] {
            assert_eq!(validated_scale(scale), 1.0);
        }
        assert_eq!(validated_scale(1.5), 1.5);
        assert_eq!(validated_scale(1.75), 1.75);
        assert_eq!(validated_scale(2.0), 2.0);
    }

    #[test]
    fn keyboard_zoom_matches_the_selector_and_survives_reopening() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-keyboard-scale-{}-{}",
            std::process::id(),
            now()
        ));
        let ctx = egui::Context::default();
        configure_style(&ctx);
        {
            let mut app = Settings::new(home.clone()).unwrap();
            for (key, expected) in [
                (egui::Key::Equals, 1.25),
                (egui::Key::Plus, 1.5),
                (egui::Key::Plus, 1.75),
                (egui::Key::Plus, 2.0),
                (egui::Key::Plus, 2.0),
                (egui::Key::Minus, 1.75),
                (egui::Key::Num0, 1.0),
                (egui::Key::Minus, 1.0),
                (egui::Key::Plus, 1.25),
            ] {
                let modifiers = egui::Modifiers::COMMAND;
                let _ = ctx.run(
                    egui::RawInput {
                        modifiers,
                        events: vec![egui::Event::Key {
                            key,
                            physical_key: Some(key),
                            pressed: true,
                            repeat: false,
                            modifiers,
                        }],
                        ..Default::default()
                    },
                    |ctx| {
                        if let Some(scale) = requested_scale(ctx, app.ui_scale) {
                            app.set_scale(ctx, scale);
                        }
                    },
                );
                let _ = ctx.run(egui::RawInput::default(), |_| {});
                assert_eq!(app.ui_scale, expected);
                assert_eq!(app.store.get::<f32>("ui_scale").unwrap(), expected);
                assert_eq!(ctx.zoom_factor(), expected);
            }
        }
        assert_eq!(Settings::new(home.clone()).unwrap().ui_scale, 1.25);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn catalog_search_finds_distinct_launch_profiles_and_user_notes() {
        let apps = vec![
            App {
                id: "personal".into(),
                name: "Browser".into(),
                arguments: "--profile-directory=Personal".into(),
                cwd: r"C:\Personal".into(),
                ..Default::default()
            },
            App {
                id: "research".into(),
                name: "Browser".into(),
                arguments: "--profile-directory=Research".into(),
                cwd: r"D:\Projects\Quarterly".into(),
                note: "Audit workspace".into(),
                ..Default::default()
            },
        ];
        let mut cache = CatalogCache::default();
        cache.rebuild(&apps, &[]);
        let recent = HashSet::new();
        for term in ["Research", "QUARTERLY", "audit workspace"] {
            cache.update(&apps, &[], term, 0, 0, &recent);
            assert_eq!(cache.indices, [1], "{term} must find the intended profile");
        }
        cache.update(&apps, &[], "Personal", 0, 0, &recent);
        assert_eq!(cache.indices, [0]);
    }

    #[test]
    fn a_large_catalog_keeps_every_result_and_rebuilds_changed_filters_and_scores() {
        let mut apps: Vec<App> = (0..3000)
            .map(|index| App {
                id: format!("app-{index:04}"),
                name: format!("Приложение {index:04}"),
                category: "Start menu".into(),
                source: format!(r"C:\Работа\Tools\App{index}.lnk"),
                target: format!(r"C:\Apps\App{index}.exe"),
                policy: if index % 3 == 0 {
                    Policy::Always
                } else {
                    Policy::Auto
                },
                valid: index % 7 != 0,
                ..App::default()
            })
            .collect();
        let mut ranking: Vec<Ranked> = apps
            .iter()
            .enumerate()
            .map(|(index, app)| Ranked {
                id: app.id.clone(),
                name: app.name.clone(),
                score: index as f64,
                days: 1,
                launches: 0,
                seconds: (3000 - index) as u32,
                eligible: app.valid,
                reason: String::new(),
            })
            .collect();
        let mut cache = CatalogCache::default();
        cache.rebuild(&apps, &ranking);
        let recent = HashSet::from(["app-0007".into(), "app-2999".into()]);
        cache.update(&apps, &ranking, "", 0, 0, &recent);
        let pages: Vec<_> = cache.indices.chunks(APPS_PER_PAGE).collect();
        assert_eq!(pages.len(), 125);
        assert_eq!(pages.last().unwrap().last(), Some(&2999));
        assert_eq!(
            pages
                .iter()
                .flat_map(|page| page.iter())
                .copied()
                .collect::<HashSet<_>>()
                .len(),
            3000
        );
        cache.update(&apps, &ranking, "ПУСК приложение 2999", 0, 0, &recent);
        assert_eq!(cache.indices, [2999]);
        cache.update(&apps, &ranking, "", 5, 0, &recent);
        assert_eq!(cache.indices, [7, 2999]);
        cache.update(&apps, &ranking, "", 0, 1, &recent);
        assert_eq!(cache.indices[0], 2999);
        ranking[7].score = 9999.0;
        apps[7].policy = Policy::Never;
        cache.rebuild(&apps, &ranking);
        cache.update(&apps, &ranking, "", 0, 1, &recent);
        assert_eq!(cache.indices[0], 7);
        cache.update(&apps, &ranking, "", 3, 0, &recent);
        assert_eq!(cache.indices, [7]);
        cache.update(&apps, &ranking, "", 0, 2, &recent);
        assert_eq!(cache.indices[0], 0);
        cache.update(&apps, &ranking, "", 4, 0, &recent);
        assert!(cache.indices.iter().all(|&index| !apps[index].valid));
    }

    #[test]
    fn a_pending_rule_stays_pending_until_the_database_reports_its_result() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-rule-result-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let mut app = Settings::new(home.clone()).unwrap();
            let entry = App {
                id: "test-app".into(),
                name: "App".into(),
                target: r"C:\App.exe".into(),
                source: r"C:\App.lnk".into(),
                valid: true,
                ..App::default()
            };
            app.store.put_app(&entry).unwrap();
            for (requested, succeeded) in [(Policy::Always, true), (Policy::Never, false)] {
                let action = format!("policy:{}:{requested:?}", entry.id);
                app.store.command(&action).unwrap();
                let command_id = app.store.db.last_insert_rowid();
                app.pending_commands.push(PendingCommand {
                    id: command_id,
                    action,
                    submitted: Instant::now(),
                });
                app.pending_policy = Some((entry.id.clone(), requested.clone()));
                app.store
                    .db
                    .execute(
                        "UPDATE commands SET status='running' WHERE id=?1",
                        [command_id],
                    )
                    .unwrap();
                app.reload().unwrap();
                assert_eq!(
                    app.pending_policy,
                    Some((entry.id.clone(), requested.clone()))
                );
                assert_ne!(app.apps[0].policy, requested);
                if succeeded {
                    app.store.policy(&entry.id, requested).unwrap();
                }
                app.store
                    .db
                    .execute(
                        "UPDATE commands SET status=?1,message=?2 WHERE id=?3",
                        rusqlite::params![
                            if succeeded { "done" } else { "error" },
                            "Result",
                            command_id
                        ],
                    )
                    .unwrap();
                app.reload().unwrap();
                assert!(app.pending_policy.is_none());
                assert!(app.pending_commands.is_empty());
                assert_eq!(app.apps[0].policy, Policy::Always);
                assert_eq!(
                    app.notice.as_ref().unwrap().kind == NoticeKind::Success,
                    succeeded
                );
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_failed_queue_and_failed_reload_do_not_leave_a_rule_looking_pending() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-rule-error-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let mut app = Settings::new(home.clone()).unwrap();
            app.pending_policy = Some(("test-app".into(), Policy::Always));
            let (sender, receiver) = mpsc::channel();
            sender
                .send(Err("Background startup failed".into()))
                .unwrap();
            app.job = Some(Job {
                label: "Sending the command…",
                started: Instant::now(),
                receiver,
            });
            app.store.set("config", &Config::default()).unwrap();
            app.store
                .db
                .execute("UPDATE kv SET v='invalid json' WHERE k='config'", [])
                .unwrap();
            app.poll_jobs(&egui::Context::default());
            assert!(app.job.is_none());
            assert!(app.pending_policy.is_none());
            assert!(app.pending_commands.is_empty());
            assert!(
                app.store.db.is_autocommit(),
                "failed read must release its snapshot"
            );
            app.store.set("config", &Config::default()).unwrap();
            app.reload().unwrap();
            assert!(
                app.store.db.is_autocommit(),
                "successful read must release its snapshot"
            );
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn settings_controls_remain_reachable_at_200_percent_in_a_small_window() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-settings-width-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let mut app = Settings::new(home.clone()).unwrap();
            let ctx = egui::Context::default();
            configure_style(&ctx);
            for width in [360.0, 480.0] {
                for section in 0..8 {
                    app.settings_section = section.min(4);
                    app.onboarding_step = section.saturating_sub(5);
                    for _ in 0..3 {
                        let _ = ctx.run(egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0))),
                            ..Default::default()
                        }, |ctx| {
                            egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(20)).show(ctx, |ui| {
                                let right = ui.max_rect().right();
                                egui::ScrollArea::vertical().show(ui, |ui| {
                                    if section < 5 {
                                        app.preferences(ui);
                                    } else {
                                        app.welcome(ui);
                                    }
                                    assert!(ui.min_rect().right() <= right + 1.0,
                                        "settings/setup section {section} exceeds {width}-point viewport: {:?}", ui.min_rect());
                                });
                            });
                        });
                    }
                }
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn catalog_controls_stay_inside_small_and_4k_scaled_viewports() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-catalog-width-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let mut app = Settings::new(home.clone()).unwrap();
            app.apps = (0..48)
                .map(|index| App {
                    id: format!("app-{index}"),
                    name: format!("Редактор с длинным названием {index}"),
                    category: "Start menu".into(),
                    valid: true,
                    ..App::default()
                })
                .collect();
            app.catalog.rebuild(&app.apps, &[]);
            let ctx = egui::Context::default();
            configure_style(&ctx);
            for width in [360.0, 480.0, 870.0, 1920.0, 2560.0, 3072.0] {
                for _ in 0..3 {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 800.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default()
                                .frame(egui::Frame::new().inner_margin(20))
                                .show(ctx, |ui| {
                                    let right = ui.max_rect().right();
                                    egui::ScrollArea::vertical().show(ui, |ui| {
                                        constrain_page_width(ui);
                                        app.applications(ui);
                                        assert!(
                                            ui.min_rect().right() <= right + 1.0,
                                            "catalog exceeds {width}-point viewport: {:?}",
                                            ui.min_rect()
                                        );
                                    });
                                });
                        },
                    );
                    if width >= 870.0 {
                        let text_y = |labels: &[&str]| {
                            output.shapes.iter().find_map(|shape| {
                                if let egui::epaint::Shape::Text(text) = &shape.shape
                                    && labels.contains(&text.galley.job.text.as_str())
                                {
                                    Some(text.pos.y)
                                } else {
                                    None
                                }
                            })
                        };
                        assert!(
                            text_y(&["All apps", "Все приложения"]).unwrap() < 300.0,
                            "filter toolbar consumed the remaining viewport height"
                        );
                        let title = text_y(&["Редактор с длинным названием 0"]).unwrap();
                        let category = text_y(&["Start menu", "Меню «Пуск»"]).unwrap();
                        assert!(
                            category >= title + 24.0,
                            "secondary app information must be below its title"
                        );
                    }
                }
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn pasted_shortcut_paths_accept_windows_quotes_unc_and_duplicates() {
        let paths = shortcut_paths("  \"C:\\Мои приложения\\Editor.LNK\"\r\n\\\\server\\Share\\Site.url\nC:\\МОИ ПРИЛОЖЕНИЯ\\editor.lnk\n").unwrap();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from(r"C:\Мои приложения\Editor.LNK"));
        assert_eq!(paths[1], PathBuf::from(r"\\server\Share\Site.url"));
        for invalid in [
            "",
            "app.lnk",
            r"C:relative.lnk",
            r"C:\App.exe",
            r"C:\App.lnk\nC:\document.txt",
        ] {
            assert!(
                shortcut_paths(invalid).is_err(),
                "unexpected import: {invalid}"
            );
        }
    }

    #[test]
    fn importing_again_keeps_rules_and_history_but_refreshes_the_target() {
        let existing = App {
            id: "app".into(),
            name: "Old name".into(),
            target: "old.exe".into(),
            policy: Policy::Never,
            seed: 42.0,
            seed_at: 1234,
            pinned: true,
            observed_roots: vec!["root".into()],
            category: "Work".into(),
            note: "User note".into(),
            ..App::default()
        };
        let mut fresh = App {
            id: existing.id.clone(),
            name: "New name".into(),
            target: "new.exe".into(),
            valid: true,
            ..App::default()
        };
        preserve_app_preferences(&mut fresh, Some(&existing));
        assert_eq!(fresh.policy, Policy::Never);
        assert_eq!((fresh.seed, fresh.seed_at), (42.0, 1234));
        assert!(fresh.pinned && fresh.valid);
        assert_eq!(fresh.observed_roots, ["root"]);
        assert_eq!(fresh.category, "Work");
        assert_eq!(fresh.note, "User note");
        assert_eq!(
            (fresh.name.as_str(), fresh.target.as_str()),
            ("New name", "new.exe")
        );
    }

    #[test]
    fn importing_legacy_url_ids_keeps_one_catalog_entry_and_its_usage_history() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-legacy-import-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let store = Store::open(&home).unwrap();
            for (id, target) in [
                ("legacy-http", "HTTPS://Example.COM/CaseSensitive"),
                (
                    "legacy-protocol",
                    "OBSIDIAN://open?vault=Research&file=Topic",
                ),
            ] {
                let app = App {
                    id: id.into(),
                    name: id.into(),
                    target: target.into(),
                    source: r"C:\Old\Original.url".into(),
                    policy: Policy::Never,
                    seed: 23.0,
                    valid: true,
                    ..App::default()
                };
                store.put_app(&app).unwrap();
                store.observe(id, 90, true, true).unwrap();
            }
            let existing: HashMap<_, _> = store
                .apps()
                .unwrap()
                .into_iter()
                .map(|app| (app.id.clone(), app))
                .collect();
            for (id, target) in [
                ("legacy-http", "https://example.com/CaseSensitive"),
                (
                    "legacy-protocol",
                    "obsidian://open?vault=Research&file=Topic",
                ),
            ] {
                let mut imported = App {
                    name: "Updated shortcut".into(),
                    target: target.into(),
                    source: r"C:\New\Original.url".into(),
                    valid: true,
                    ..App::default()
                };
                imported.assign_id();
                assert_ne!(imported.id, id);
                prepare_imported_app(&mut imported, &existing).unwrap();
                store.put_app(&imported).unwrap();
                assert_eq!(imported.id, id);
                assert_eq!(imported.policy, Policy::Never);
                assert_eq!(imported.seed, 23.0);
            }
            assert_eq!(store.apps().unwrap().len(), 2);
            let days = store.days().unwrap();
            assert_eq!(days.len(), 2);
            assert!(days.iter().all(|day| {
                existing.contains_key(&day.app) && day.seconds == 90 && day.sessions == 1
            }));

            let mut ambiguous = existing.clone();
            let mut duplicate = existing["legacy-http"].clone();
            duplicate.id = "another-legacy-id".into();
            ambiguous.insert(duplicate.id.clone(), duplicate);
            let mut imported = existing["legacy-http"].clone();
            imported.assign_id();
            assert!(prepare_imported_app(&mut imported, &ambiguous).is_err());
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn explicit_startup_or_tracking_changes_resume_a_disabled_profile() {
        let base = Config {
            collect: false,
            automatic: false,
            autostart: false,
            ..Config::default()
        };
        assert!(!should_resume_background(true, &base, &base));
        let mut draft = base.clone();
        draft.slots = 7;
        assert!(!should_resume_background(true, &base, &draft));
        for (collect, automatic, autostart) in [
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let draft = Config {
                collect,
                automatic,
                autostart,
                ..base.clone()
            };
            assert!(should_resume_background(true, &base, &draft));
            assert!(!should_resume_background(false, &base, &draft));
        }
    }

    #[test]
    fn app_details_can_be_closed_with_the_mouse_without_scrolling() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-details-close-{}-{}",
            std::process::id(),
            now()
        ));
        {
            let mut app = Settings::new(home.clone()).unwrap();
            app.apps = vec![App {
                id: "long-details".into(),
                name: "A shortcut with a long application name".into(),
                source: format!(r"C:\{}\Original.lnk", "Very long folder name\\".repeat(30)),
                note: "More shortcut information. ".repeat(100),
                valid: true,
                ..App::default()
            }];
            let ctx = egui::Context::default();
            configure_style(&ctx);
            for size in [egui::vec2(360.0, 270.0), egui::vec2(2560.0, 1440.0)] {
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                app.details_app = Some("long-details".into());
                let mut close = None;
                for _ in 0..3 {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ctx| app.app_details(ctx),
                    );
                    close = output.shapes.iter().find_map(|shape| {
                        if let egui::epaint::Shape::Text(text) = &shape.shape
                            && ["Close", "Закрыть"].contains(&text.galley.text())
                        {
                            let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                            assert!(screen.contains_rect(bounds));
                            assert!(shape.clip_rect.contains_rect(bounds));
                            Some(bounds.center())
                        } else {
                            None
                        }
                    });
                }
                let close = close.expect("Close must be visible above the long scrollable content");
                for pressed in [true, false] {
                    let _ = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            events: vec![
                                egui::Event::PointerMoved(close),
                                egui::Event::PointerButton {
                                    pos: close,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: egui::Modifiers::NONE,
                                },
                            ],
                            ..Default::default()
                        },
                        |ctx| app.app_details(ctx),
                    );
                }
                assert!(
                    app.details_app.is_none(),
                    "visible Close must dismiss Details"
                );
            }
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn long_dialogs_fit_a_minimum_window_at_large_text_scale() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        for _ in 0..3 {
            let _ = ctx.run(egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 360.0))),
                ..Default::default()
            }, |ctx| {
                let modal = dialog(ctx, "small-dialog", 560.0, "Сохранить изменения перед закрытием?", |ui| {
                    ui.add(egui::Label::new("Подробное сообщение с длинным путём: C:\\Users\\Пользователь\\Документы\\DesktopPilot\\usage.sqlite\n".repeat(12)).wrap());
                    let _ = ui.button("Продолжить настройку");
                });
                assert!(modal.response.rect.width() <= 480.0);
                assert!(modal.response.rect.height() <= 360.0);
            });
        }
    }

    #[test]
    fn unfinished_setup_can_resume_or_discard_its_saved_choices() {
        let home = std::env::temp_dir().join(format!(
            "test-ui-setup-draft-{}-{}",
            std::process::id(),
            now()
        ));
        let ctx = egui::Context::default();
        {
            let mut app = Settings::new(home.clone()).unwrap();
            apply_mode(&mut app.config, OperatingMode::Manual);
            app.config.slots = 7;
            app.config.zone_col = 9;
            app.onboarding_step = 2;
            app.close_setup(&ctx, true);
            assert!(app.allow_close);
            assert!(!engine::setup_complete(&app.store).unwrap());
        }
        {
            let mut resumed = Settings::new(home.clone()).unwrap();
            assert_eq!(operating_mode(&resumed.config), OperatingMode::Manual);
            assert_eq!(
                (
                    resumed.config.slots,
                    resumed.config.zone_col,
                    resumed.onboarding_step
                ),
                (7, 9, 2)
            );
            resumed.close_setup(&ctx, false);
            assert!(resumed.allow_close);
        }
        {
            let fresh = Settings::new(home.clone()).unwrap();
            assert_eq!(operating_mode(&fresh.config), OperatingMode::Adaptive);
            assert_eq!(fresh.onboarding_step, 0);
            assert_eq!(fresh.config.slots, Config::default().slots);
        }
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_dialog_can_grow_after_shorter_content_without_hiding_its_actions() {
        let ctx = egui::Context::default();
        configure_style(&ctx);
        for rows in [0, 4, 4] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(480.0, 360.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let mut action = egui::Rect::NOTHING;
                    let response =
                        dialog(ctx, "growing-dialog", 420.0, "Настройки", |ui| {
                            for _ in 0..rows {
                                ui.label("Сохранённые параметры доступны.");
                            }
                            action = ui.button("Продолжить настройку").rect;
                        });
                    assert!(
                        action.bottom() <= response.response.rect.bottom(),
                        "dialog clipped its action after growing: {action:?} / {:?}",
                        response.response.rect
                    );
                },
            );
        }
    }
}

#[cfg(test)]
mod import_ownership_tests;

#[cfg(test)]
mod clock_health_tests;

#[cfg(test)]
mod catalog_snapshot_tests;

#[cfg(test)]
mod catalog_relevance_tests;

#[cfg(test)]
mod import_scaling_tests;

mod import_catalog;
use import_catalog::ImportCatalog;

#[cfg(test)]
mod import_last_filter_tests;
