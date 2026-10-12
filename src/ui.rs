use crate::{
    engine,
    i18n::{self, text as t},
    learning::{Mode, Progress},
    model::*,
    native, setup,
    store::Store,
};
use anyhow::Result;
use eframe::egui::{self, Color32, RichText};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
pub struct Settings {
    store: Store,
    config: Config,
    apps: Vec<App>,
    state: State,
    learning: Progress,
    ranking: Vec<Ranked>,
    tab: usize,
    filter: String,
    message: String,
    status: String,
    last: Instant,
    dirty: bool,
    onboarding: bool,
    confirm_clear: bool,
    pending_command: Option<i64>,
    capture: Option<PathBuf>,
    capture_at: Instant,
    capture_pending: bool,
}
fn when(timestamp: i64) -> String {
    if timestamp == 0 {
        return t("ещё не выполнялось", "not yet").into();
    }
    chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|d| {
            d.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}
fn policy_name(policy: &Policy) -> &'static str {
    match policy {
        Policy::Auto => t("Автоматически", "Automatic"),
        Policy::Always => t("Всегда на столе", "Always on desktop"),
        Policy::Never => t("Не добавлять", "Never add"),
    }
}
fn flag(ui: &egui::Ui, response: &egui::Response, language: &str) {
    let rect = egui::Rect::from_min_size(
        egui::pos2(response.rect.left() + 7.0, response.rect.center().y - 6.0),
        egui::vec2(20.0, 12.0),
    );
    let painter = ui.painter().with_clip_rect(rect);
    if language == "ru" {
        for (row, color) in [
            Color32::WHITE,
            Color32::from_rgb(35, 77, 179),
            Color32::from_rgb(214, 42, 54),
        ]
        .iter()
        .enumerate()
        {
            painter.rect_filled(
                egui::Rect::from_min_size(
                    rect.min + egui::vec2(0.0, row as f32 * 4.0),
                    egui::vec2(20.0, 4.0),
                ),
                0.0,
                *color,
            );
        }
    } else {
        painter.rect_filled(rect, 0.0, Color32::from_rgb(28, 48, 108));
        for color in [Color32::WHITE, Color32::from_rgb(205, 39, 55)] {
            let width = if color == Color32::WHITE { 3.2 } else { 1.2 };
            painter.line_segment(
                [rect.left_top(), rect.right_bottom()],
                egui::Stroke::new(width, color),
            );
            painter.line_segment(
                [rect.right_top(), rect.left_bottom()],
                egui::Stroke::new(width, color),
            );
        }
        for (width, color) in [(4.2, Color32::WHITE), (2.2, Color32::from_rgb(205, 39, 55))] {
            painter.line_segment(
                [
                    egui::pos2(rect.left(), rect.center().y),
                    egui::pos2(rect.right(), rect.center().y),
                ],
                egui::Stroke::new(width, color),
            );
            painter.line_segment(
                [
                    egui::pos2(rect.center().x, rect.top()),
                    egui::pos2(rect.center().x, rect.bottom()),
                ],
                egui::Stroke::new(width, color),
            );
        }
    }
}
impl Settings {
    fn new(home: PathBuf) -> Result<Self> {
        let store = Store::open(&home)?;
        let onboarding = !engine::setup_complete(&store)?;
        let mut config: Config = store.get("config")?;
        if onboarding {
            config.automatic = true;
            config.collect = true;
            config.autostart = true;
            if let Ok(layout) = native::layout() {
                if let Ok((col, row, width)) = engine::suggest_zone(&layout, config.slots) {
                    config.zone_col = col;
                    config.zone_row = row;
                    config.zone_columns = width;
                }
                if layout.flags & 1 != 0 {
                    config.automatic = false;
                }
            }
        }
        let mut app = Self {
            store,
            config,
            apps: vec![],
            state: State::default(),
            learning: Progress::default(),
            ranking: vec![],
            tab: 0,
            filter: String::new(),
            message: String::new(),
            status: String::new(),
            last: Instant::now(),
            dirty: onboarding,
            onboarding,
            confirm_clear: false,
            pending_command: None,
            capture: None,
            capture_at: Instant::now() + Duration::from_secs(1),
            capture_pending: false,
        };
        app.reload()?;
        Ok(app)
    }
    fn reload(&mut self) -> Result<()> {
        self.apps = self.store.apps()?;
        self.apps.sort_by_key(|a| a.name.to_lowercase());
        self.state = self.store.get("state")?;
        self.learning = self.store.get("learning")?;
        if !self.dirty {
            self.config = self.store.get("config")?;
        }
        self.ranking = rank(&self.apps, &self.store.days()?, &self.config, now());
        self.status = self.store.get("status")?;
        if let Some(id) = self.pending_command {
            let (status, message): (String, String) = self.store.db.query_row(
                "SELECT status,message FROM commands WHERE id=?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if status != "pending" {
                self.message = message;
                self.pending_command = None;
            }
        }
        Ok(())
    }
    fn queue(&mut self, command: &str) {
        match self.store.command(command) {
            Ok(()) => {
                self.pending_command = Some(self.store.db.last_insert_rowid());
                self.message = t(
                    "Команда передана фоновому приложению",
                    "Command sent to the background process",
                )
                .into();
            }
            Err(e) => self.message = e.to_string(),
        }
    }
    fn language(&mut self, ui: &mut egui::Ui) {
        let code = if self.config.language == "ru" {
            "RU"
        } else {
            "EN"
        };
        let button = ui.menu_button(
            RichText::new(format!("        {code}    ")).size(16.0),
            |ui| {
                for (language, label) in [("en", "English"), ("ru", "Русский")] {
                    let choice = ui.selectable_label(
                        self.config.language == language,
                        format!("        {label}"),
                    );
                    flag(ui, &choice, language);
                    if choice.clicked() {
                        // Persist just the language; do not commit other unsaved settings.
                        let saved = self.store.get::<Config>("config").and_then(|mut config| {
                            config.language = language.into();
                            self.store.set("config", &config)
                        });
                        match saved {
                            Ok(()) => self.config.language = language.into(),
                            Err(e) => self.message = e.to_string(),
                        }
                        ui.close_menu();
                    }
                }
            },
        );
        flag(ui, &button.response, &self.config.language);
        let center = egui::pos2(
            button.response.rect.right() - 10.0,
            button.response.rect.center().y,
        );
        let stroke = egui::Stroke::new(1.5, ui.visuals().text_color());
        ui.painter().line_segment(
            [
                center + egui::vec2(-3.0, -1.5),
                center + egui::vec2(0.0, 1.5),
            ],
            stroke,
        );
        ui.painter().line_segment(
            [
                center + egui::vec2(0.0, 1.5),
                center + egui::vec2(3.0, -1.5),
            ],
            stroke,
        );
        button.response.on_hover_text("Language / Язык");
        i18n::set(&self.config.language);
    }
    fn zone(&mut self, ui: &mut egui::Ui) {
        ui.label(t("Столбцы и строки отсчитываются от нуля на основном мониторе. Существующие значки сохраняют позиции.","Columns and rows start at zero on the primary monitor. Existing icons keep their positions."));
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.zone_col, 0..=40)
                    .text(t("начальный столбец", "first column")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.zone_row, 0..=40)
                    .text(t("начальная строка", "first row")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.zone_columns, 1..=8)
                    .text(t("столбцов в области", "columns in the area")),
            )
            .changed();
        if ui
            .button(t("Найти свободную область", "Find a free area"))
            .clicked()
        {
            match native::layout().and_then(|l| engine::suggest_zone(&l, self.config.slots)) {
                Ok((col, row, width)) => {
                    self.config.zone_col = col;
                    self.config.zone_row = row;
                    self.config.zone_columns = width;
                    self.dirty = true;
                }
                Err(e) => self.message = e.to_string(),
            }
        }
    }
    fn learning_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading(t(
            "Пассивное наблюдение перед размещением",
            "Observe before placing shortcuts",
        ));
        self.dirty |= ui
            .checkbox(
                &mut self.config.learning_enabled,
                t(
                    "Сначала накопить наблюдения",
                    "Start with passive observation",
                ),
            )
            .changed();
        ui.add_enabled_ui(self.config.learning_enabled, |ui| {
            let before = self.config.learning_mode;
            let label = |mode| match mode {
                Mode::Sessions => t("По числу сеансов", "Number of sessions"),
                Mode::Days => t("По активным дням", "Active days"),
                Mode::Either => t(
                    "Сеансы ИЛИ дни — любое условие",
                    "Sessions OR days — either condition",
                ),
                Mode::Both => t(
                    "Сеансы И дни — оба условия",
                    "Sessions AND days — both conditions",
                ),
            };
            egui::ComboBox::from_id_salt("learning-mode")
                .selected_text(label(before))
                .show_ui(ui, |ui| {
                    for mode in [Mode::Sessions, Mode::Days, Mode::Either, Mode::Both] {
                        ui.selectable_value(&mut self.config.learning_mode, mode, label(mode));
                    }
                });
            self.dirty |= before != self.config.learning_mode;
            if self.config.learning_mode != Mode::Days {
                self.dirty |= ui
                    .add(
                        egui::Slider::new(&mut self.config.learning_sessions, 1..=100)
                            .text(t("сеансов за компьютером", "computer sessions")),
                    )
                    .changed();
            }
            if self.config.learning_mode != Mode::Sessions {
                self.dirty |= ui
                    .add(
                        egui::Slider::new(&mut self.config.learning_days, 1..=90)
                            .text(t("активных дней", "active days")),
                    )
                    .changed();
            }
        });
        ui.small(t("Сеанс: минимум 5 минут учтённой активности; новый — после 30 минут без неё. Активный день: минимум 5 минут. Перезапуск программы не обнуляет прогресс.", "A session needs 5 minutes of tracked activity; 30 minutes without it starts a new session. An active day needs 5 minutes. Progress survives app restarts."));
        ui.small(t("После достижения порога ярлыки появятся автоматически при включённой автоматике, в безопасный момент бездействия. Пока идёт наблюдение, ярлыки не создаются.", "After the threshold, shortcuts appear automatically when automation is enabled and the desktop is idle. Observation itself never creates shortcuts."));
    }
    fn learning_progress(&self, ui: &mut egui::Ui) {
        if !self.config.learning_enabled {
            return;
        }
        if self.learning.ready(&self.config) {
            ui.label(t(
                "Первоначальное наблюдение завершено",
                "Initial observation complete",
            ));
            return;
        }
        ui.heading(if self.config.collect {
            t("Пассивное наблюдение", "Passive observation")
        } else {
            t("Наблюдение приостановлено", "Observation paused")
        });
        if self.config.learning_mode != Mode::Days {
            ui.label(format!(
                "{}: {} / {}",
                t("Сеансы за компьютером", "Computer sessions"),
                self.learning.sessions,
                self.config.learning_sessions
            ));
            ui.add(egui::ProgressBar::new(
                (self.learning.sessions as f32 / self.config.learning_sessions as f32).min(1.0),
            ));
        }
        if self.config.learning_mode != Mode::Sessions {
            ui.label(format!(
                "{}: {} / {}",
                t("Активные дни", "Active days"),
                self.learning.active_days(),
                self.config.learning_days
            ));
            ui.add(egui::ProgressBar::new(
                (self.learning.active_days() as f32 / self.config.learning_days as f32).min(1.0),
            ));
        }
        ui.label(t("Ярлыки пока не создаются. После выполнения условия будет выбран наиболее востребованный состав.", "No shortcuts are created yet. The most-used eligible apps will be selected after the observation requirement is met."));
        ui.add_space(10.0);
    }
    fn welcome(&mut self, ui: &mut egui::Ui) {
        ui.heading(t("Настройка первого запуска", "Welcome to Desktop Pilot"));
        ui.label(t("Программа создаёт дополнительные быстрые ярлыки. Оригиналы и ваша структура папок сохраняются.","Create a small set of adaptive quick shortcuts while keeping your original shortcuts and folder structure."));
        ui.add_space(10.0);
        ui.separator();
        self.learning_settings(ui);
        ui.checkbox(
            &mut self.config.automatic,
            t(
                "Автоматические изменения раз в неделю",
                "Automatic changes once a week",
            ),
        );
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
                "Не дублировать закрепления панели задач",
                "Exclude apps already pinned to the taskbar",
            ),
        );
        ui.add(
            egui::Slider::new(&mut self.config.slots, 1..=8)
                .text(t("быстрых мест", "quick-access slots")),
        );
        self.config.max_changes = self.config.max_changes.min(self.config.slots);
        ui.add_space(8.0);
        ui.heading(t("Область размещения", "Shortcut area"));
        self.zone(ui);
        ui.add_space(12.0);
        ui.label(t("Установка и данные: %LOCALAPPDATA%\\DesktopPilot. Только дневные счётчики приложений. Без сети, записи клавиш и текста окон.","Install location and data: %LOCALAPPDATA%\\DesktopPilot. Daily app-usage totals only. No network service, keystrokes or window text."));
        if ui
            .add_sized(
                [230.0, 36.0],
                egui::Button::new(t("Начать работу", "Start using Desktop Pilot")),
            )
            .clicked()
        {
            match setup::finish(&self.store, &self.config) {
                Ok(()) => {
                    self.onboarding = false;
                    self.dirty = false;
                    self.message = t(
                        "Готово. Выберите приложения или дождитесь статистики.",
                        "Ready. Choose apps or let the usage history build up.",
                    )
                    .into();
                    let _ = self.reload();
                }
                Err(e) => self.message = format!("{e:#}"),
            }
        }
    }
    fn overview(&mut self, ui: &mut egui::Ui) {
        self.learning_progress(ui);
        ui.heading(t("Сейчас на рабочем столе", "On your desktop"));
        ui.label(format!(
            "{} / {} {}",
            self.state.owned.len(),
            self.config.slots,
            t("быстрых мест", "quick-access slots")
        ));
        if self.state.owned.is_empty() {
            ui.label(t("Пока нет быстрых копий. Во время наблюдения можно настроить правила будущего выбора.","No quick copies yet. You can set rules for the future selection while observation runs."));
        }
        ui.add_space(10.0);
        for owned in &self.state.owned {
            let name = self
                .apps
                .iter()
                .find(|a| a.id == owned.app)
                .map(|a| a.name.as_str())
                .unwrap_or("Unavailable");
            egui::Frame::group(ui.style())
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(name);
                        ui.label(if owned.manual {
                            t("Ручное закрепление", "Manually pinned")
                        } else {
                            t("Автоматический выбор", "Automatic selection")
                        });
                    });
                    ui.small(format!(
                        "{}, {} · {} {}",
                        owned.position.x,
                        owned.position.y,
                        t("с", "since"),
                        when(owned.since)
                    ));
                });
            ui.add_space(4.0);
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.learning.ready(&self.config),
                    egui::Button::new(t("Применить сейчас", "Apply now")),
                )
                .clicked()
            {
                self.queue("apply");
            }
            if ui
                .button(t("Откатить последнее изменение", "Undo last change"))
                .clicked()
            {
                self.queue("rollback");
            }
        });
        ui.label(format!(
            "{}: {}",
            t("Последний пересмотр", "Last review"),
            when(self.state.last_review)
        ));
        let next = if self.state.last_review == 0 {
            t(
                "после накопления статистики",
                "after enough usage has been observed",
            )
            .into()
        } else {
            when(self.state.last_review + self.config.interval_days * 86400)
        };
        ui.label(format!(
            "{}: {}",
            t("Следующий пересмотр", "Next review"),
            next
        ));
        ui.label(t("Перемещённый вручную быстрый ярлык закрепляется. Удалённый не возвращается. Существующие папки остаются на месте.","Move a quick shortcut to pin it. Delete one to exclude it. Existing folders stay in place."));
        ui.separator();
        ui.heading(t("Следующие по рейтингу", "Next in the ranking"));
        for r in self
            .ranking
            .iter()
            .filter(|r| r.eligible && !self.state.owned.iter().any(|o| o.app == r.id))
            .take(5)
        {
            ui.label(format!(
                "{} · {:.1} {} · {} {}",
                r.name,
                r.score,
                t("балла", "points"),
                r.days,
                t("дней", "days")
            ));
        }
    }
    fn applications(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label(t("Поиск", "Search"));
            ui.text_edit_singleline(&mut self.filter);
            if ui.button(t("Обновить список", "Refresh catalog")).clicked() {
                self.queue("rescan");
            }
            if ui.button(t("Добавить ярлык…", "Add shortcut…")).clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter(t("Ярлыки", "Shortcuts"), &["lnk", "url"])
                    .pick_file()
            {
                self.message = match native::read_link(&path).and_then(|mut a| {
                    a.category = "Manual".into();
                    self.store.put_app(&a)
                }) {
                    Ok(()) => t(
                        "Ярлык добавлен; выберите правило",
                        "Shortcut added; choose a rule",
                    )
                    .into(),
                    Err(e) => e.to_string(),
                };
            }
        });
        ui.label(t("«Всегда на столе» важнее рейтинга и панели задач. Для приложений, открываемых только через файлы, контекстное меню или горячие клавиши, выберите «Не добавлять».","Always on desktop overrides ranking and taskbar filtering. Choose Never add for apps you only open through files, context menus or hotkeys."));
        let mut commands = Vec::new();
        let filter = self.filter.to_lowercase();
        for app in &self.apps {
            if !format!("{} {}", app.name, app.category)
                .to_lowercase()
                .contains(&filter)
            {
                continue;
            }
            ui.push_id(&app.id, |ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.strong(&app.name);
                        let mut policy = app.policy.clone();
                        egui::ComboBox::from_id_salt("policy")
                            .selected_text(policy_name(&policy))
                            .show_ui(ui, |ui| {
                                for value in [Policy::Auto, Policy::Always, Policy::Never] {
                                    let label = policy_name(&value);
                                    ui.selectable_value(&mut policy, value, label);
                                }
                            });
                        if policy != app.policy {
                            commands.push(format!("policy:{}:{policy:?}", app.id));
                        }
                        if app.pinned {
                            ui.label(t("На панели задач", "Pinned to taskbar"));
                        }
                        if !app.valid {
                            ui.colored_label(
                                Color32::LIGHT_RED,
                                t("Цель недоступна", "Target unavailable"),
                            );
                        }
                    });
                    ui.small(&app.category);
                    if let Some(r) = self.ranking.iter().find(|r| r.id == app.id) {
                        ui.small(format!(
                            "{:.1} {} · {} {} · {} {} · {} {}",
                            r.score,
                            t("балла", "points"),
                            r.days,
                            t("дней", "days"),
                            r.launches,
                            t("запросов запуска", "launch requests"),
                            r.seconds / 60,
                            t("мин активности", "active min")
                        ));
                        ui.small(i18n::runtime(&r.reason));
                    }
                    ui.collapsing(
                        t("Исходный ярлык и пояснение", "Original shortcut and notes"),
                        |ui| {
                            ui.monospace(&app.source);
                            ui.label(i18n::runtime(&app.note));
                        },
                    );
                });
            });
            ui.add_space(4.0);
        }
        for command in commands {
            self.queue(&command);
        }
    }
    fn preferences(&mut self, ui: &mut egui::Ui) {
        ui.separator();
        self.learning_settings(ui);
        ui.separator();
        ui.heading(t("Автоматика", "Automation"));
        self.dirty |= ui
            .checkbox(
                &mut self.config.automatic,
                t(
                    "Автоматически пересматривать ярлыки",
                    "Automatically review quick shortcuts",
                ),
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.config.collect,
                t(
                    "Собирать локальную статистику",
                    "Collect local usage statistics",
                ),
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.config.autostart,
                t(
                    "Запускать при входе в Windows",
                    "Start when I sign in to Windows",
                ),
            )
            .changed();
        self.dirty |= ui
            .checkbox(
                &mut self.config.respect_taskbar_pins,
                t("Не дублировать панель задач", "Exclude taskbar-pinned apps"),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.slots, 1..=8)
                    .text(t("мест на рабочем столе", "desktop slots")),
            )
            .changed();
        self.config.max_changes = self.config.max_changes.min(self.config.slots);
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.interval_days, 1..=30)
                    .text(t("дней между пересмотрами", "days between reviews")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.max_changes, 1..=self.config.slots)
                    .text(t("замен за пересмотр", "replacements per review")),
            )
            .changed();
        ui.separator();
        ui.heading(t("Устойчивость выбора", "Ranking stability"));
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.min_days, 1..=14)
                    .text(t("разных дней наблюдений", "distinct days of use")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.half_life_days, 3.0..=60.0).text(t(
                    "дней до уменьшения веса вдвое",
                    "history half-life in days",
                )),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.advantage, 1.1..=3.0)
                    .text(t("минимальное преимущество", "minimum score advantage")),
            )
            .changed();
        self.dirty |= ui
            .add(
                egui::Slider::new(&mut self.config.idle_seconds, 30..=600).text(t(
                    "секунд до остановки учёта",
                    "idle seconds before tracking pauses",
                )),
            )
            .changed();
        ui.separator();
        ui.heading(t("Размещение новых ярлыков", "New shortcut placement"));
        self.zone(ui);
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(t("Сохранить настройки", "Save settings"))
                .clicked()
            {
                match self
                    .config
                    .validate()
                    .and_then(|()| self.store.set("config", &self.config))
                {
                    Ok(()) => {
                        self.dirty = false;
                        self.message = t("Настройки сохранены", "Settings saved").into();
                    }
                    Err(e) => self.message = e.to_string(),
                }
            }
            if ui
                .button(t(
                    "Принять текущую геометрию",
                    "Accept current display geometry",
                ))
                .clicked()
            {
                self.queue("accept_geometry");
            }
        });
        ui.separator();
        ui.label(t("Данные хранятся 90 дней. Активное окно — признак использования, а не доказательство клика ярлыка. Неоднозначные профили одного EXE не объединяются.","Data is retained for 90 days. Foreground activity indicates use, not a shortcut click. Ambiguous profiles of one executable are not merged."));
        ui.checkbox(
            &mut self.confirm_clear,
            t(
                "Подтверждаю очистку статистики",
                "I want to clear the usage history",
            ),
        );
        if ui
            .add_enabled(
                self.confirm_clear,
                egui::Button::new(t("Очистить статистику", "Clear usage history")),
            )
            .clicked()
        {
            self.queue("clear_stats");
            self.confirm_clear = false;
        }
    }
    fn screenshots(&mut self, ctx: &egui::Context) {
        if let Some(directory) = &self.capture {
            for event in ctx.input(|i| i.events.clone()) {
                if let egui::Event::Screenshot { image, .. } = event {
                    let bytes: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Err(e) = image::save_buffer(
                        directory.join(format!("settings-{}.png", self.tab)),
                        &bytes,
                        image.width() as u32,
                        image.height() as u32,
                        image::ColorType::Rgba8,
                    ) {
                        self.message = e.to_string();
                    }
                    self.capture_pending = false;
                    if self.tab == 3 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.tab += 1;
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
        self.screenshots(ctx);
        ctx.request_repaint_after(Duration::from_secs(2));
        if self.last.elapsed() > Duration::from_secs(2) {
            self.last = Instant::now();
            if let Err(e) = self.reload() {
                self.message = e.to_string();
            }
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.heading(RichText::new("Desktop Pilot").size(25.0));
                ui.add_space(16.0);
                if !self.onboarding {
                    ui.colored_label(
                        if self.config.automatic {
                            Color32::from_rgb(91, 217, 173)
                        } else {
                            Color32::from_rgb(242, 191, 92)
                        },
                        if !self.learning.ready(&self.config) {
                            if self.config.collect {
                                t("Пассивное наблюдение", "Passive observation")
                            } else {
                                t("Наблюдение на паузе", "Observation paused")
                            }
                        } else if self.config.automatic {
                            t("Автоматика включена", "Automation enabled")
                        } else {
                            t("Автоматика на паузе", "Automation paused")
                        },
                    );
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.language(ui)
                });
            });
            ui.label(t(
                "Быстрый доступ с сохранением привычного порядка",
                "Quick access without losing your familiar layout",
            ));
            if self.dirty && !self.onboarding {
                ui.colored_label(
                    Color32::from_rgb(242, 191, 92),
                    t(
                        "Есть несохранённые настройки",
                        "Settings have unsaved changes",
                    ),
                );
            }
            if !self.onboarding {
                ui.horizontal(|ui| {
                    for (i, label) in [
                        t("Обзор", "Overview"),
                        t("Приложения", "Apps"),
                        t("Настройки", "Settings"),
                        t("История", "History"),
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut self.tab, i, *label);
                    }
                });
            }
            ui.add_space(6.0);
        });
        egui::TopBottomPanel::bottom("footer").show(ctx, |ui| {
            ui.small(t(
                "Локальные данные · Без записи текста и клавиш · Оригиналы сохраняются",
                "Local data · No window text or keystrokes · Originals kept",
            ));
            if !self.message.is_empty() {
                ui.colored_label(
                    Color32::from_rgb(242, 191, 92),
                    i18n::runtime(&self.message),
                );
            }
            if !self.onboarding {
                ui.small(i18n::runtime(&self.status));
            }
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(10.0);
                if self.onboarding {
                    self.welcome(ui);
                    return;
                }
                match self.tab {
                    0 => self.overview(ui),
                    1 => self.applications(ui),
                    2 => self.preferences(ui),
                    _ => {
                        ui.heading(t(
                            "Изменения и результаты команд",
                            "Changes and command results",
                        ));
                        if let Ok(events) = self.store.events() {
                            for (time, kind, message) in events {
                                ui.small(format!("{} · {}", when(time), kind));
                                ui.label(i18n::runtime(&message));
                                ui.separator();
                            }
                        }
                    }
                }
            });
        });
    }
}
pub fn run(home: PathBuf, capture: Option<PathBuf>) -> Result<()> {
    let mut app = Settings::new(home)?;
    if let Some(path) = &capture {
        std::fs::create_dir_all(path)?;
    }
    app.capture = capture;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 740.0])
            .with_min_inner_size([820.0, 580.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Desktop Pilot",
        options,
        Box::new(move |cc| {
            let mut style = (*cc.egui_ctx.style()).clone();
            style.visuals = egui::Visuals::dark();
            style.visuals.selection.bg_fill = Color32::from_rgb(28, 111, 119);
            style.visuals.override_text_color = Some(Color32::from_rgb(224, 228, 233));
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            for (kind, size) in [
                (egui::TextStyle::Body, 15.0),
                (egui::TextStyle::Button, 14.0),
                (egui::TextStyle::Small, 12.0),
            ] {
                style
                    .text_styles
                    .insert(kind, egui::FontId::proportional(size));
            }
            cc.egui_ctx.set_style(style);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Settings window: {e}"))
}
