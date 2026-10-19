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
    collections::HashMap,
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};

const ACCENT: Color32 = Color32::from_rgb(86, 207, 187);
const MUTED: Color32 = Color32::from_rgb(163, 177, 193);
const WARNING: Color32 = Color32::from_rgb(246, 196, 110);
const ERROR: Color32 = Color32::from_rgb(255, 153, 153);
const SURFACE: Color32 = Color32::from_rgb(29, 38, 50);

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
enum JobResult {
    Setup(Config),
    Saved(Config),
    Queued(i64, String),
    Started,
    Added,
}
struct Job {
    label: &'static str,
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
    notice: Option<Notice>,
    status: String,
    catalog_warning: String,
    autostart_warning: String,
    background_alive: bool,
    heartbeat: i64,
    last: Instant,
    dirty: bool,
    onboarding: bool,
    onboarding_step: usize,
    confirm_clear: bool,
    confirm_close: bool,
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
    capture_scroll_bottom: bool,
    capture_at: Instant,
    capture_pending: bool,
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
    // Maintenance reports every minute. Allow two missed reports before warning.
    if at.saturating_sub(heartbeat) > 150 {
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
    if scale.is_finite()
        && [1.0_f32, 1.25, 1.5]
            .iter()
            .any(|v| (scale - v).abs() < 0.01)
    {
        scale
    } else {
        1.0
    }
}
fn matches_search(app: &App, query: &str) -> bool {
    let haystack = format!(
        "{} {} {} {}",
        app.name, app.category, app.source, app.target
    )
    .to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}
fn page_controls(ui: &mut egui::Ui, page: &mut usize, count: usize) {
    let pages = count.div_ceil(24).max(1);
    *page = (*page).min(pages - 1);
    if pages <= 1 {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(*page > 0, egui::Button::new(t("Назад", "Previous")))
            .clicked()
        {
            *page -= 1;
        }
        ui.label(format!(
            "{} {} / {}",
            t("Страница", "Page"),
            *page + 1,
            pages
        ));
        if ui
            .add_enabled(*page + 1 < pages, egui::Button::new(t("Вперёд", "Next")))
            .clicked()
        {
            *page += 1;
        }
    });
}
fn card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style())
        .fill(SURFACE)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            content(ui);
        });
    ui.add_space(8.0);
}
fn constrain_page_width(ui: &mut egui::Ui) {
    // set_max_width can expand a Ui. Keep the original viewport constraint,
    // including its scrollbar and the frame margins.
    ui.set_max_width(ui.available_width().min(1060.0));
}
fn configure_style(ctx: &egui::Context) {
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
            config.automatic = true;
            config.collect = true;
            config.autostart = true;
        }
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
            notice: None,
            status: String::new(),
            catalog_warning: String::new(),
            autostart_warning: String::new(),
            background_alive: false,
            heartbeat: 0,
            last: Instant::now(),
            dirty: onboarding,
            onboarding,
            onboarding_step: 0,
            confirm_clear: false,
            confirm_close: false,
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
            capture_scroll_bottom: false,
            capture_at: Instant::now() + Duration::from_secs(1),
            capture_pending: false,
        };
        app.reload()?;
        app.refresh_layout(onboarding);
        Ok(app)
    }
    fn inform(&mut self, text: impl Into<String>, kind: NoticeKind) {
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
    fn refresh_dirty(&mut self) {
        self.dirty = self.onboarding || !config_equal(&self.config, &self.edit_base);
    }
    fn reload(&mut self) -> Result<()> {
        let mut apps = self.store.apps()?;
        apps.sort_by_key(|a| a.name.to_lowercase());
        let state: State = self.store.get("state")?;
        let learning = self.store.get("learning")?;
        let saved_config = self.store.get("config")?;
        let ranking = rank(&apps, &self.store.days()?, &saved_config, now());
        let status = self.store.get("status")?;
        let catalog_warning = self.store.get("catalog_warning")?;
        let autostart_warning = self.store.get("autostart_warning")?;
        let heartbeat = self.store.get("heartbeat")?;
        let background_alive = native::is_running("background", &self.store.home)?;
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
        self.apps = apps;
        self.state = state;
        self.learning = learning;
        self.ranking = ranking;
        self.status = status;
        self.catalog_warning = catalog_warning;
        self.autostart_warning = autostart_warning;
        self.heartbeat = heartbeat;
        self.background_alive = background_alive;
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
        self.preview = match engine::preview(&self.store, !self.state.initialized) {
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
        self.job = Some(Job { label, receiver });
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
    fn restart_background(&mut self) {
        let home = self.store.home.clone();
        self.start_job("Starting background work…", move || {
            setup::start_background(&home)?;
            Ok(JobResult::Started)
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
            Ok(JobResult::Saved(merged))
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
            Ok(JobResult::Setup(config))
        });
    }
    fn add_shortcut(&mut self, path: PathBuf) {
        let home = self.store.home.clone();
        self.start_job("Adding the shortcut…", move || {
            let _com = native::Com::new()?;
            let mut app = native::read_link(&path)?;
            let store = Store::open(&home)?;
            store.transaction(|| {
                if let Some(existing) = store.apps()?.into_iter().find(|a| a.id == app.id) {
                    app.policy = existing.policy;
                    app.seed = existing.seed;
                    app.seed_at = existing.seed_at;
                    app.pinned = existing.pinned;
                    app.observed_roots = existing.observed_roots;
                    if app.note.is_empty() {
                        app.note = existing.note;
                    }
                    app.category = existing.category;
                } else {
                    app.category = "Manual".into();
                }
                store.put_app(&app)
            })?;
            Ok(JobResult::Added)
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
                        Ok(JobResult::Setup(config)) | Ok(JobResult::Saved(config)) => {
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
                        Ok(JobResult::Added) => self.inform(
                            "Shortcut added. Existing rules and history were kept.",
                            NoticeKind::Success,
                        ),
                        Err(e) => {
                            self.close_after_save = false;
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
        let before = self.ui_scale;
        egui::ComboBox::from_id_salt("ui-scale")
            .selected_text(format!("{:.0}%", self.ui_scale * 100.0))
            .width(88.0)
            .show_ui(ui, |ui| {
                for (value, label) in [(1.0, "100%"), (1.25, "125%"), (1.5, "150%")] {
                    ui.selectable_value(&mut self.ui_scale, value, label);
                }
            })
            .response
            .on_hover_text(t(
                "Размер текста и элементов интерфейса",
                "Text and interface size",
            ));
        if before != self.ui_scale {
            match self.store.set("ui_scale", &self.ui_scale) {
                Ok(()) => ctx.set_zoom_factor(self.ui_scale),
                Err(e) => {
                    self.ui_scale = before;
                    self.fail(format!("{e:#}"));
                }
            }
        }
    }
    fn navigation(&mut self, ui: &mut egui::Ui, vertical: bool) {
        let labels = [
            t("Главная", "Home"),
            t("Приложения", "Apps"),
            t("Настройки", "Settings"),
            t("История", "History"),
        ];
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
            .selected_text(label(before))
            .show_ui(ui, |ui| {
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
        egui::CollapsingHeader::new(t("Точное положение", "Exact position")).show(ui, |ui| {
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
        }
        response.on_hover_text(t(
            "Нажмите свободную ячейку, чтобы выбрать начало области",
            "Click a free cell to choose the start of the area",
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
                    egui::CollapsingHeader::new(t(
                        "Куда устанавливается программа",
                        "Installation location",
                    ))
                    .show(ui, |ui| {
                        ui.label(self.store.home.display().to_string());
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
        if !self.autostart_warning.is_empty()
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
        ui.label(t(
            "Нужные приложения рядом. Привычный порядок сохранён.",
            "Keep useful apps close and your familiar layout intact.",
        ));
        ui.add_space(12.0);
        card(ui, |ui| {
            let current = self.current_health();
            ui.colored_label(
                health_color(current),
                RichText::new(health_label(current)).size(22.0).strong(),
            );
            match current {
                Health::Offline => {
                    ui.label(t(
                        "Наблюдение и автоматические изменения сейчас не выполняются. Настройки и накопленные данные сохранены.",
                        "Observation and automatic changes are stopped. Your settings and usage history are kept."
                    ));
                    if primary(
                        ui,
                        self.job.is_none(),
                        t("Возобновить работу", "Resume background work"),
                    )
                    .clicked()
                    {
                        self.restart_background();
                    }
                }
                Health::Unresponsive => {
                    ui.label(t(
                        "Программа ещё запущена, но давно не обновляла состояние. Текущая операция может занимать больше времени.",
                        "The process is still running, but has not reported recently. The current operation may be taking longer."
                    ));
                    ui.small(format!(
                        "{}: {}",
                        t("Последний ответ", "Last response"),
                        when(self.heartbeat)
                    ));
                }
                Health::Attention => {
                    ui.label(i18n::runtime(&self.status));
                    if !self.autostart_warning.is_empty()
                        && !self.status.contains(&self.autostart_warning)
                    {
                        ui.label(i18n::runtime(&self.autostart_warning));
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
                Health::Manual => {
                    ui.label(if self.saved_config.collect {
                        t("Наблюдение продолжается. Состав ярлыков меняется по кнопке «Применить сейчас».",
                          "Observation continues. Choose Apply now to update your shortcuts.")
                    } else {
                        t("Новая статистика не собирается. Вы выбираете состав и момент применения.",
                          "No new usage statistics are collected. You choose the apps and when to apply changes.")
                    });
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
        egui::CollapsingHeader::new(t("Почему выбраны эти приложения", "Why these apps were chosen")).show(ui, |ui| {
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
        ui.heading(RichText::new(t("Приложения", "Apps")).size(28.0));
        ui.label(t(
            "Выберите, что всегда должно быть под рукой, а что не нужно добавлять.",
            "Choose what should always be nearby and what to leave out.",
        ));
        ui.add_space(10.0);
        card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .hint_text(t(
                            "Поиск по имени, папке или пути…",
                            "Search by name, folder or path…",
                        ))
                        .desired_width(ui.available_width().min(430.0)),
                );
                if !self.filter.is_empty() && ui.button(t("Сбросить", "Clear")).clicked() {
                    self.filter.clear();
                }
            });
            ui.horizontal_wrapped(|ui| {
                egui::ComboBox::from_id_salt("app-filter")
                    .selected_text(match self.app_filter {
                        1 => t("Подходят для размещения", "Eligible for placement"),
                        2 => t("Всегда на столе", "Always on desktop"),
                        3 => t("Не добавлять", "Never add"),
                        4 => t("Недоступные", "Unavailable"),
                        _ => t("Все приложения", "All apps"),
                    })
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (0, t("Все приложения", "All apps")),
                            (1, t("Подходят для размещения", "Eligible for placement")),
                            (2, t("Всегда на столе", "Always on desktop")),
                            (3, t("Не добавлять", "Never add")),
                            (4, t("Недоступные", "Unavailable")),
                        ] {
                            ui.selectable_value(&mut self.app_filter, value, label);
                        }
                    });
                egui::ComboBox::from_id_salt("app-sort")
                    .selected_text(match self.app_sort {
                        1 => t("По рейтингу", "By ranking"),
                        2 => t("По времени использования", "By usage time"),
                        _ => t("По алфавиту", "Alphabetically"),
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.app_sort,
                            0,
                            t("По алфавиту", "Alphabetically"),
                        );
                        ui.selectable_value(&mut self.app_sort, 1, t("По рейтингу", "By ranking"));
                        ui.selectable_value(
                            &mut self.app_sort,
                            2,
                            t("По времени использования", "By usage time"),
                        );
                    });
            });
            ui.horizontal_wrapped(|ui| {
                if primary(ui, !self.busy(), t("Добавить ярлык…", "Add shortcut…")).clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter(t("Ярлыки", "Shortcuts"), &["lnk", "url"])
                        .pick_file()
                {
                    self.add_shortcut(path);
                }
                if ui
                    .add_enabled(
                        !self.busy(),
                        egui::Button::new(t(
                            "Найти установленные приложения",
                            "Find installed apps",
                        )),
                    )
                    .clicked()
                {
                    self.queue("rescan");
                }
            });
        });
        if previous_filter != (self.filter.clone(), self.app_filter, self.app_sort) {
            self.app_page = 0;
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
                egui::CollapsingHeader::new(t("Подробности", "Details")).show(ui, |ui| {
                    ui.add(
                        egui::Label::new(i18n::runtime(&self.catalog_warning))
                            .wrap()
                            .selectable(true),
                    );
                });
            });
        }
        let ranking: HashMap<_, _> = self
            .ranking
            .iter()
            .map(|ranked| (ranked.id.as_str(), ranked))
            .collect();
        let mut indices: Vec<usize> = (0..self.apps.len())
            .filter(|&index| {
                let app = &self.apps[index];
                if !matches_search(app, &self.filter) {
                    return false;
                }
                match self.app_filter {
                    1 => ranking.get(app.id.as_str()).is_some_and(|r| r.eligible),
                    2 => app.policy == Policy::Always,
                    3 => app.policy == Policy::Never,
                    4 => !app.valid,
                    _ => true,
                }
            })
            .collect();
        if self.app_sort != 0 {
            indices.sort_by(|&a, &b| {
                let a = ranking.get(self.apps[a].id.as_str());
                let b = ranking.get(self.apps[b].id.as_str());
                if self.app_sort == 1 {
                    b.map_or(0.0, |r| r.score)
                        .total_cmp(&a.map_or(0.0, |r| r.score))
                } else {
                    b.map_or(0, |r| r.seconds).cmp(&a.map_or(0, |r| r.seconds))
                }
            });
        }
        ui.small(format!(
            "{}: {} / {}",
            t("Найдено", "Matches"),
            indices.len(),
            self.apps.len()
        ));
        let match_count = indices.len();
        page_controls(ui, &mut self.app_page, match_count);
        if indices.is_empty() {
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
        let mut selected_command = None;
        let busy = self.busy();
        for index in indices.into_iter().skip(self.app_page * 24).take(24) {
            let app = &self.apps[index];
            ui.push_id(&app.id, |ui| {
                card(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(RichText::new(&app.name).size(20.0));
                        if app.pinned {
                            ui.small(t("На панели задач", "On the taskbar"));
                        }
                        if !app.valid {
                            ui.colored_label(
                                WARNING,
                                t(
                                    "Ярлык или цель недоступны",
                                    "Shortcut or target unavailable",
                                ),
                            );
                        }
                    });
                    let category = match app.category.as_str() {
                        "Manual" => t("Добавлено вами", "Added by you"),
                        "Desktop" => t("Рабочий стол", "Desktop"),
                        "Start menu" => t("Меню «Пуск»", "Start menu"),
                        other => other,
                    };
                    if !category.is_empty() {
                        ui.small(category);
                    }
                    ui.add_enabled_ui(!busy, |ui| {
                        let mut policy = app.policy.clone();
                        egui::ComboBox::from_id_salt("policy")
                            .width(ui.available_width().min(320.0))
                            .selected_text(policy_name(&policy))
                            .show_ui(ui, |ui| {
                                for value in [Policy::Auto, Policy::Always, Policy::Never] {
                                    let label = policy_name(&value);
                                    ui.selectable_value(&mut policy, value, label);
                                }
                            });
                        if policy != app.policy {
                            selected_command = Some(format!("policy:{}:{policy:?}", app.id));
                        }
                    });
                    if let Some(ranked) = ranking.get(app.id.as_str()) {
                        ui.label(i18n::runtime(&ranked.reason));
                        egui::CollapsingHeader::new(t(
                            "Статистика и исходный ярлык",
                            "Usage and original shortcut",
                        ))
                        .show(ui, |ui| {
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
                            ui.label(t("Исходный ярлык", "Original shortcut"));
                            ui.add(egui::Label::new(&app.source).wrap().selectable(true));
                            if ui.button(t("Скопировать путь", "Copy path")).clicked()
                            {
                                ui.ctx().copy_text(app.source.clone());
                            }
                            if !app.note.is_empty() {
                                ui.label(i18n::runtime(&app.note));
                            }
                        });
                    }
                });
            });
        }
        page_controls(ui, &mut self.app_page, match_count);
        if let Some(command) = selected_command {
            self.queue(&command);
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
            ui.horizontal_wrapped(|ui| {
                ui.heading(RichText::new(t("Настройки", "Settings")).size(28.0));
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
                        egui::CollapsingHeader::new(t("Другой интервал", "Custom interval")).show(
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
            let mut dismiss = false;
            ui.horizontal_wrapped(|ui| {
                if ui
                    .small_button("×")
                    .on_hover_text(t("Закрыть сообщение", "Dismiss message"))
                    .clicked()
                {
                    dismiss = true;
                }
                if notice.kind == NoticeKind::Error {
                    ui.colored_label(
                        color,
                        t(
                            "Не удалось завершить действие",
                            "The action could not be completed",
                        ),
                    );
                    if ui.small_button(t("Копировать", "Copy")).clicked() {
                        ctx.copy_text(message.clone());
                    }
                } else if message.chars().count() <= 180 {
                    ui.colored_label(color, &message);
                }
            });
            if notice.kind == NoticeKind::Error || message.chars().count() > 180 {
                egui::ScrollArea::vertical()
                    .id_salt("notice-details")
                    .max_height(86.0)
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&message).color(color))
                                .wrap()
                                .selectable(true),
                        );
                    });
            }
            if dismiss {
                self.notice = None;
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
    fn confirmations(&mut self, ctx: &egui::Context) {
        if self.confirm_clear {
            egui::Window::new(t("Очистить статистику использования?", "Clear usage statistics?"))
                .id(egui::Id::new("clear-confirmation"))
                .collapsible(false).resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .max_width(480.0)
                .show(ctx, |ui| {
                    ui.label(t(
                        "Рейтинг начнёт накапливаться заново. Если наблюдение ещё не завершено, его прогресс сбросится. Ваши правила и ярлыки сохранятся.",
                        "The ranking will start building up again. Unfinished observation will reset. Your rules and shortcuts will be kept."
                    ));
                    ui.colored_label(WARNING, t("Очистку нельзя отменить.", "Clearing cannot be undone."));
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(t("Сохранить статистику", "Keep statistics")).clicked() {
                            self.confirm_clear = false;
                        }
                        if ui.add_enabled(!self.busy(), egui::Button::new(t("Очистить", "Clear"))
                            .fill(Color32::from_rgb(112, 43, 52)).min_size(egui::vec2(0.0, 42.0))).clicked() {
                            self.confirm_clear = false;
                            self.queue("clear_stats");
                        }
                    });
                });
        }
        if self.confirm_close {
            egui::Window::new(t(
                "Сохранить изменения перед закрытием?",
                "Save changes before closing?",
            ))
            .id(egui::Id::new("close-confirmation"))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .max_width(520.0)
            .show(ctx, |ui| {
                ui.label(t(
                    "В настройках есть несохранённые правки.",
                    "Your settings contain unsaved edits.",
                ));
                ui.add_space(10.0);
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
                            egui::Button::new(t("Закрыть без сохранения", "Close without saving")),
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
            });
        }
        if (self.confirm_clear || self.confirm_close)
            && ctx.input(|i| i.key_pressed(egui::Key::Escape))
        {
            self.confirm_clear = false;
            self.confirm_close = false;
        }
    }
    fn screenshots(&mut self, ctx: &egui::Context) {
        if let Some(directory) = &self.capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    let name = if self.onboarding {
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
                    if (self.onboarding && self.onboarding_step == 2)
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
            } else if self.dirty && !self.onboarding {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
            }
        }
        if !self.confirm_clear
            && !self.confirm_close
            && !self.onboarding
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S))
        {
            self.save();
        }
        let interactive = !self.confirm_clear && !self.confirm_close && self.job.is_none();
        egui::TopBottomPanel::top("header")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(19, 28, 39))
                    .inner_margin(egui::Margin::symmetric(20, 14)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(RichText::new("Desktop Pilot").size(27.0).strong());
                    ui.add_space(12.0);
                    ui.add_enabled_ui(interactive, |ui| self.language_and_scale(ui, ctx));
                });
            });
        egui::TopBottomPanel::bottom("footer")
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(19, 28, 39))
                    .inner_margin(egui::Margin::symmetric(18, 8)),
            )
            .show(ctx, |ui| {
                // Progress and error details stay readable while an operation is running.
                ui.add_enabled_ui(!self.confirm_clear && !self.confirm_close, |ui| {
                    self.footer(ui, ctx)
                });
            });
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
                    ui.add_enabled_ui(interactive, |ui| self.navigation(ui, true));
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
                    ui.add_enabled_ui(interactive, |ui| {
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
                egui::ScrollArea::vertical()
                    .id_salt(("page-scroll", self.onboarding, self.tab, section))
                    .auto_shrink([false, false])
                    .stick_to_bottom(self.capture_scroll_bottom)
                    .show(ui, |ui| {
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
    if app.capture.is_none() && !app.onboarding && !app.background_alive {
        app.restart_background();
    }
    let scale = app.ui_scale;
    let test_viewport = if app.capture.is_some() {
        app.store
            .get::<Option<[f32; 2]>>("test_viewport")?
            .filter(|size| {
                size[0].is_finite()
                    && size[1].is_finite()
                    && (360.0..=2560.0).contains(&size[0])
                    && (320.0..=1600.0).contains(&size[1])
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
        assert!(matches_search(&app, "  РЕДАКТОР  tools "));
        assert!(matches_search(&app, ""));
        assert!(!matches_search(&app, "редактор unknown"));
    }

    #[test]
    fn invalid_ui_scale_cannot_make_the_window_unusable() {
        for scale in [f32::NAN, f32::INFINITY, -1.0, 0.0, 20.0] {
            assert_eq!(validated_scale(scale), 1.0);
        }
        assert_eq!(validated_scale(1.5), 1.5);
    }
}
