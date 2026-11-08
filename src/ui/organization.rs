use super::*;
use crate::organize;
use std::sync::atomic::AtomicBool;

const ORGANIZATION_PAGE_SIZE: usize = 40;

// Projection may verify filesystem identities and search many candidate grid
// positions. egui paints this modal on every frame: repeat the projection
// only when its read-only inputs change, never on a normal repaint.
// Apply separately rechecks live desktop files and positions under the lock.
pub(super) struct OrganizationProjectionCache {
    config: Config,
    selected: Vec<bool>,
    available: bool,
}

fn cached_organization_projection(
    plan: &organize::Plan,
    config: &Config,
    cache: &mut Option<OrganizationProjectionCache>,
    compute: impl FnOnce(&organize::Plan, &Config) -> bool,
) -> bool {
    let selected: Vec<_> = plan.items.iter().map(|item| item.selected).collect();
    if let Some(previous) = cache
        && previous.config == *config
        && previous.selected == selected
    {
        return previous.available;
    }
    let available = compute(plan, config);
    *cache = Some(OrganizationProjectionCache {
        config: config.clone(),
        selected,
        available,
    });
    available
}

fn organization_projection_available(
    plan: &organize::Plan,
    config: &Config,
    cache: &mut Option<OrganizationProjectionCache>,
) -> bool {
    cached_organization_projection(plan, config, cache, |plan, config| {
        plan.projected_zone(config).is_some()
    })
}

fn organization_page_bounds(total: usize, requested: usize) -> (usize, usize, usize, usize) {
    let pages = total.div_ceil(ORGANIZATION_PAGE_SIZE).max(1);
    let page = requested.min(pages - 1);
    let start = page * ORGANIZATION_PAGE_SIZE;
    let end = (start + ORGANIZATION_PAGE_SIZE).min(total);
    (page, pages, start, end)
}

fn organization_pages(ui: &mut egui::Ui, page: &mut usize, total: usize) -> (usize, usize) {
    let (current, pages, _, _) = organization_page_bounds(total, *page);
    *page = current;
    if pages > 1 {
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
                .add(
                    egui::DragValue::new(&mut requested)
                        .range(1..=pages)
                        .update_while_editing(false),
                )
                .changed()
            {
                *page = requested.saturating_sub(1).min(pages - 1);
            }
            ui.label(format!("/ {pages}"));
            if ui
                .add_enabled(*page + 1 < pages, egui::Button::new(t("Далее", "Next")))
                .clicked()
            {
                *page += 1;
            }
        });
    }
    let (_, _, start, end) = organization_page_bounds(total, *page);
    (start, end)
}

fn organization_reason(reason: &str) -> String {
    // Translate complete known reasons only. Never replace words inside a
    // user's filename, path, URL or an operating-system diagnostic.
    let russian = match reason {
        "Files are not selected for organization" => "Перенос обычных файлов не включён",
        "You chose to keep this object on the desktop" => {
            "Вы выбрали оставить этот объект на столе"
        }
        "Managed quick shortcut: left in place" => "Быстрый ярлык Desktop Pilot остаётся на месте",
        "Manual shortcut rule: left in place" => "Сохранено ваше ручное правило для ярлыка",
        "Another known shortcut depends on this object: left in place" => {
            "На этот объект ссылается другой ярлык"
        }
        "Shortcut launch arguments may refer to this object: left in place" => {
            "Этот файл может использоваться в параметрах запуска ярлыка"
        }
        "The desktop is used as an application's working directory: files left in place" => {
            "Рабочий стол используется приложением как рабочая папка; файлы оставлены на месте"
        }
        "Some shortcut dependencies could not be inspected: files left in place" => {
            "Не удалось проверить связи некоторых ярлыков; обычные файлы оставлены на месте"
        }
        "Protected, cloud-placeholder and filesystem-link objects are left in place" => {
            "Папка, защищённый объект, облачный файл или ссылка файловой системы"
        }
        "Relative working directory or icon: left in place" => {
            "Рабочая папка или значок заданы относительным путём"
        }
        "Shortcut without an explicit working directory: left in place" => {
            "У ярлыка не задана рабочая папка"
        }
        "Relative or unrecognized shortcut target: left in place" => {
            "Цель ярлыка относительная или не распознана"
        }
        "No standalone launch target: left in place" => {
            "Не удалось определить самостоятельную цель запуска"
        }
        "Executable, linked data or unknown file type: left in place" => {
            "Исполняемый файл, связанные данные или неизвестный тип"
        }
        "Associated file set: left in place" => "Файл входит в связанный набор",
        _ => return i18n::runtime(reason),
    };
    t(russian, reason).to_owned()
}

// A person's terms may describe different visible columns, e.g. an app
// name plus the destination folder. Require every term, regardless of order.
// This is a read-only filter: plans and saved rules never change.
// The UI normalizes the query once per frame; do not allocate a new query
// string for each of the thousands of possible rows in a large review.
fn organizer_matches_query(query: &str, first: &str, second: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let first = first.to_lowercase();
    let second = second.to_lowercase();
    query
        .split_whitespace()
        .all(|term| first.contains(term) || second.contains(term))
}

fn skipped_matches_query(item: &organize::Skipped, query: &str) -> bool {
    query.is_empty()
        || organizer_matches_query(query, &item.name, &organization_reason(&item.reason))
}

impl Settings {
    fn organization_analyze(&mut self) {
        if self.busy() {
            return;
        }
        // A new review is a new decision; don't retain consent for an
        // unrelated reset operation across the refreshed desktop plan.
        self.organization_reset_confirm = false;
        self.organization_plan = None;
        self.organization_projection = None;
        // Every successful request starts a fresh complete preview. A search
        // left over from the previous plan must not hide newly selected moves.
        self.organization_query.clear();
        self.organization_page = 0;
        let home = self.store.home.clone();
        let config = self.config.clone();
        let include_files = self.organization_files;
        self.start_job("Analyzing the desktop…", move || {
            let _com = native::Com::new()?;
            let store = Store::open(&home)?;
            let plan = organize::analyze(
                &store,
                organize::Options { include_files },
                &config,
                &AtomicBool::new(false),
            )?;
            Ok(JobResult::OrganizationPlan(plan))
        });
    }

    fn organization_apply(&mut self) {
        if self.busy() {
            return;
        }
        let Some(plan) = self.organization_plan.clone() else {
            return;
        };
        let home = self.store.home.clone();
        let keep_only = plan.summary().0 == 0;
        self.start_job(
            if keep_only {
                "Saving keep-on-desktop choices…"
            } else {
                "Organizing desktop objects…"
            },
            move || {
                let _com = native::Com::new()?;
                let (outcome, warning) = if keep_only {
                    // Only an exclusion was requested: the filesystem is not
                    // changed, so don't interrupt live usage observation.
                    let store = Store::open(&home)?;
                    (organize::remember_keep(&store, &plan)?, None)
                } else {
                    setup::with_organization_paused(&home, |store| {
                        organize::apply_stopped(store, &plan, &AtomicBool::new(false))
                    })?
                };
                Ok(JobResult::OrganizationChanged(outcome, warning))
            },
        );
    }

    fn organization_undo(&mut self, id: String) {
        if self.busy() {
            return;
        }
        let home = self.store.home.clone();
        self.start_job("Restoring desktop objects…", move || {
            let _com = native::Com::new()?;
            let (outcome, warning) =
                setup::with_organization_paused(&home, |store| organize::undo_stopped(store, &id))?;
            Ok(JobResult::OrganizationChanged(outcome, warning))
        });
    }

    fn organization_accept(&mut self, id: String) {
        if self.busy() {
            return;
        }
        let home = self.store.home.clone();
        self.start_job("Confirming current desktop locations…", move || {
            let _com = native::Com::new()?;
            let (_, warning) = setup::with_organization_paused(&home, |store| {
                organize::accept_current_stopped(store, &id)
            })?;
            Ok(JobResult::OrganizationAccepted(warning))
        });
    }

    fn organization_reset_exclusions(&mut self) {
        if self.busy() {
            return;
        }
        let home = self.store.home.clone();
        self.start_job("Resetting desktop organization exclusions…", move || {
            let store = Store::open(&home)?;
            organize::clear_exclusions(&store)?;
            Ok(JobResult::OrganizationReset)
        });
    }

    // Feedback belongs to the modal that initiated the operation, not solely
    // to the main window's footer hidden behind its interaction blocker.
    fn organization_feedback(&self) -> Option<(String, Color32, String)> {
        let notice = self.notice.as_ref()?;
        let details = i18n::runtime(&notice.text);
        let (summary, color) = if notice.kind == NoticeKind::Error {
            (
                t(
                    "Действие не завершено. Подробности ниже.",
                    "The action did not complete. Details are below.",
                )
                .to_owned(),
                ERROR,
            )
        } else if notice.kind == NoticeKind::Success && details.contains('\n') {
            (
                t(
                    "Готово с замечаниями. Проверьте подробности ниже.",
                    "Completed with warnings. Review the details below.",
                )
                .to_owned(),
                WARNING,
            )
        } else {
            let first = details.lines().next().unwrap_or_default();
            let summary = if first.chars().count() > 80 {
                format!("{}…", first.chars().take(80).collect::<String>())
            } else {
                first.to_owned()
            };
            let color = if notice.kind == NoticeKind::Success {
                ACCENT
            } else {
                MUTED
            };
            (summary, color)
        };
        Some((summary, color, details))
    }

    pub(super) fn organization_dialog(&mut self, ctx: &egui::Context) -> egui::Rect {
        let mut analyze = false;
        let mut apply = false;
        let mut undo = None;
        let mut accept = None;
        let mut reset = false;
        let mut close = false;
        let available = !self.busy();
        let status = organize::status(&self.store);
        // An interrupted organization hides exclusion reset controls. A check
        // from before recovery must not authorize a later reset after recovery.
        if !status.as_ref().is_ok_and(|state| !state.pending) {
            self.organization_reset_confirm = false;
        }
        // A confirmation applies to one exact recovery journal. Another
        // Settings window may resolve that journal and start a newer operation
        // while this dialog remains open. Never carry consent to a new ID.
        let pending_id = status
            .as_ref()
            .ok()
            .filter(|state| state.pending)
            .and_then(|state| state.undo.as_deref());
        if self.organization_accept_for.as_deref() != pending_id {
            self.organization_accept_for = None;
        }
        // Keep the confirmation reachable while the user reviews a long plan.
        // The modal body scrolls independently of its header.
        let preview_action = status
            .as_ref()
            .ok()
            .filter(|current| !current.pending)
            .and(self.organization_plan.as_ref())
            .filter(|plan| plan.options.include_files == self.organization_files)
            .map(|plan| {
                let (moves, _, _) = plan.summary();
                let rejected = plan.items.iter().any(|item| !item.selected);
                (moves, rejected)
            })
            // A plan with no move and no Keep decision has no action to take.
            // Avoid displaying an inert confirmation for a checked singleton.
            .filter(|(moves, rejected)| *moves > 0 || *rejected);
        let feedback = self.organization_feedback();
        let running = self.job.as_ref().map(|job| match job.label {
            "Analyzing the desktop…" => t("Анализируем рабочий стол…", "Analyzing the desktop…"),
            "Organizing desktop objects…" => t("Распределяем объекты…", "Organizing objects…"),
            "Saving keep-on-desktop choices…" => {
                t("Сохраняем исключения…", "Saving keep choices…")
            }
            "Restoring desktop objects…" => t("Восстанавливаем объекты…", "Restoring objects…"),
            _ => t("Выполняем действие…", "Working…"),
        });
        let response = dialog_frame(
            ctx,
            "desktop-organization",
            760.0,
            |ui| {
                // Put the exit control beside the title at every zoom level.
                // The single Apply/Remember action gets its own fixed row so
                // neither control can wrap unpredictably in a long preview.
                ui.horizontal(|ui| {
                    ui.heading(t("Навести порядок", "Organize desktop"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(available, egui::Button::new(t("Закрыть", "Close")))
                            .clicked()
                        {
                            close = true;
                        }
                    });
                });
                if let Some((moves, rejected)) = preview_action
                    && primary(
                        ui,
                        available && (moves > 0 || rejected),
                        if moves > 0 {
                            t("Применить выбранное", "Apply selected moves")
                        } else {
                            t("Запомнить исключения", "Remember keep choices")
                        },
                    )
                    .clicked()
                {
                    apply = true;
                }
                if let Some(label) = running {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(label);
                    });
                }
                if let Some((summary, color, _)) = &feedback {
                    ui.add(egui::Label::new(RichText::new(summary).color(*color)).wrap());
                }
            },
            |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                if let Some((_, color, details)) = &feedback {
                    collapsing_header(ui, t("Результат действия", "Action details")).show(
                        ui,
                        |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(details).color(*color))
                                    .wrap()
                                    .selectable(true),
                            );
                        },
                    );
                }
                ui.label(t(
                    "Сначала проверьте план перемещений в обычные папки. Существующие папки, защищённые объекты и ручные закрепления остаются на месте.",
                    "Review the plan before moving objects into ordinary folders. Existing folders, protected objects and manual pins stay in place.",
                ));
                ui.small(t(
                    "По умолчанию распределяются только распознанные ярлыки. Автоматической фоновой уборки нет.",
                    "Only recognized shortcuts are selected by default. There is no automatic background cleanup.",
                ));
                match status.as_ref() {
                    Err(error) => {
                        ui.colored_label(ERROR, i18n::runtime(&format!("{error:#}")));
                    }
                    Ok(current) if current.pending => {
                        ui.colored_label(WARNING, t(
                            "Предыдущая уборка была прервана. Восстановите объекты или явно примите их текущее расположение.",
                            "An earlier organization was interrupted. Restore the objects or explicitly accept their current locations.",
                        ));
                        if let Some(id) = &current.undo {
                            if primary(ui, available, t("Восстановить", "Restore")).clicked()
                            {
                                undo = Some(id.clone());
                            }
                            let mut confirmed =
                                self.organization_accept_for.as_deref() == Some(id.as_str());
                            ui.add_enabled(
                                available,
                                egui::Checkbox::new(
                                    &mut confirmed,
                                    t(
                                        "Я проверил файлы и хочу оставить их в текущем расположении.",
                                        "I checked the files and want to keep their current locations.",
                                    ),
                                ),
                            );
                            self.organization_accept_for = confirmed.then(|| id.clone());
                            if ui
                                .add_enabled(
                                    available && confirmed,
                                    egui::Button::new(t(
                                        "Оставить как есть",
                                        "Accept current locations",
                                    )),
                                )
                                .clicked()
                            {
                                accept = Some(id.clone());
                            }
                        }
                    }
                    Ok(current) => {
                        ui.add_enabled(
                            available,
                            egui::Checkbox::new(
                                &mut self.organization_files,
                                t(
                                    "Также распределить обычные файлы",
                                    "Also organize ordinary files",
                                ),
                            ),
                        );
                        if self.organization_files {
                            ui.colored_label(WARNING, t(
                                "Перенос изменит пути файлов. Связи внутри документов и других программ не всегда можно определить. Проверьте выбранные файлы; проекты, код и таблицы не переносятся.",
                                "Moving changes file paths. Links inside documents and other apps cannot always be detected. Review your selection; projects, code and spreadsheets are not moved.",
                            ));
                        }
                        if self
                            .organization_plan
                            .as_ref()
                            .is_some_and(|p| p.options.include_files != self.organization_files)
                        {
                            self.organization_plan = None;
                            self.organization_projection = None;
                            self.organization_page = 0;
                        }
                        if primary(
                            ui,
                            available,
                            t("Проанализировать рабочий стол", "Analyze the desktop"),
                        )
                        .clicked()
                        {
                            analyze = true;
                        }
                        if let Some(plan) = self.organization_plan.as_mut() {
                            let (moves, folders, freed) = plan.summary();
                            ui.heading(t("Предварительный план", "Preview"));
                            ui.label(format!(
                                "{}: {moves} · {}: {folders} · {}: {freed}",
                                t("Перемещений", "Moves"),
                                t("Новых папок", "New folders"),
                                t("Мест освободится", "Places freed"),
                            ));
                            ui.small(t(
                                "В оценке учтены новые папки. Перед переносом расположение и состав проверяются повторно.",
                                "New folders count towards the space estimate. Locations and selections are checked again before moving.",
                            ));
                            let selected_count =
                                plan.items.iter().filter(|item| item.selected).count();
                            if moves == 0 && selected_count > 0 {
                                // Checked singletons in new groups stay in place. Explain why
                                // a still-checked row does not count as a pending move.
                                ui.small(t(
                                    "Сейчас ничего не переместится: новая папка создаётся только для двух и более отмеченных объектов одной группы. Чтобы переместить их, отметьте ещё один объект этой группы.",
                                    "Nothing will move yet: a new folder needs at least two checked items in the same group. To move them, check another item in that group.",
                                ));
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!(
                                    "{}: {selected_count} / {}",
                                    t("Отмечено", "Selected"),
                                    plan.items.len()
                                ));
                                if ui
                                    .add_enabled(
                                        available && selected_count < plan.items.len(),
                                        egui::Button::new(t("Отметить все", "Select all"))
                                            .min_size(egui::vec2(130.0, 40.0)),
                                    )
                                    .clicked()
                                {
                                    for item in &mut plan.items {
                                        item.selected = true;
                                    }
                                }
                                if ui
                                    .add_enabled(
                                        available && selected_count > 0,
                                        egui::Button::new(t("Снять все отметки", "Deselect all"))
                                            .min_size(egui::vec2(160.0, 40.0)),
                                    )
                                    .clicked()
                                {
                                    for item in &mut plan.items {
                                        item.selected = false;
                                    }
                                }
                            });
                            ui.small(t(
                                "Команды действуют на весь план, включая скрытые поиском строки и другие страницы.",
                                "These controls affect the entire plan, including filtered-out rows and other pages.",
                            ));
                            // When every candidate stays in place, no layout will change.
                            // A quick-shortcut-space warning would imply a failed move
                            // and suggest unnecessary work during a Keep-only review.
                            if moves > 0
                                && !organization_projection_available(
                                    plan,
                                    &self.config,
                                    &mut self.organization_projection,
                                )
                            {
                                ui.colored_label(WARNING, t(
                                    "Подходящая область для быстрых ярлыков пока не подтверждена. После уборки обновите схему; при необходимости выберите меньше ярлыков.",
                                    "A suitable quick-shortcut area is not confirmed yet. Refresh the layout after organizing; reduce the shortcut count if necessary.",
                                ));
                            }
                            // Put recovery beside the search label rather than
                            // pushing long exclusion lists below the viewport.
                            // Search never changes a saved selection or policy.
                            ui.horizontal_wrapped(|ui| {
                                ui.label(t(
                                    "Поиск по имени, папке или причине исключения",
                                    "Search by name, folder or exclusion reason",
                                ));
                                if !self.organization_query.trim().is_empty()
                                    && ui
                                        .add_enabled(
                                            available,
                                            egui::Button::new(t("Очистить поиск", "Clear search")),
                                        )
                                        .clicked()
                                {
                                    self.organization_query.clear();
                                    self.organization_page = 0;
                                    ctx.data_mut(|data| {
                                        data.insert_temp(
                                            egui::Id::new("organize-skipped-page"),
                                            0usize,
                                        );
                                    });
                                }
                            });
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.organization_query)
                                        .desired_width(ui.available_width()),
                                )
                                .changed()
                            {
                                self.organization_page = 0;
                                ctx.data_mut(|data| {
                                    data.insert_temp(
                                        egui::Id::new("organize-skipped-page"),
                                        0usize,
                                    );
                                });
                            }
                            ui.small(t(
                                "Снимите галочку, чтобы оставить объект на столе. Можно запомнить исключения без перемещения других файлов.",
                                "Uncheck an object to leave it on the desktop. You can remember exclusions without moving other files.",
                            ));
                            let query = self.organization_query.trim().to_lowercase();
                            let filtered: Vec<_> = plan
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| {
                                    query.is_empty()
                                        || organizer_matches_query(
                                            &query,
                                            &item.name,
                                            &plan.destination(item),
                                        )
                                })
                                .map(|(index, _)| index)
                                .collect();
                            // Compute both result groups before showing feedback: a query
                            // may match only excluded objects, or neither group.
                            let skipped: Vec<_> = plan
                                .skipped
                                .iter()
                                .filter(|item| skipped_matches_query(item, &query))
                                .collect();
                            // Use one concise status line for search, not two:
                            // an extra row hid the excluded-items section at 100% DPI.
                            if !query.is_empty() {
                                if filtered.is_empty() {
                                    if skipped.is_empty() {
                                        ui.small(t(
                                            "Ничего не найдено в этом плане. Очистите поиск или измените слова.",
                                            "No items in this review match. Clear search or try different words.",
                                        ));
                                    } else {
                                        ui.small(t(
                                            "Среди предлагаемых объектов совпадений нет. Найденные оставленные на месте объекты показаны ниже.",
                                            "No proposed items match. Matching left-in-place objects are shown below.",
                                        ));
                                    }
                                } else {
                                    ui.small(t(
                                        "Поиск скрывает только строки списка. При применении будут обработаны все отмеченные объекты, включая скрытые фильтром и находящиеся на других страницах.",
                                        "Search only hides rows. Apply processes every checked item, including those hidden by this filter or on other pages.",
                                    ));
                                }
                            }
                            // Keep the original full-plan controls above, but let a person
                            // review one searched set across every page without silently
                            // changing other selected desktop objects.
                            if !query.is_empty() {
                                let selected_matches = filtered
                                    .iter()
                                    .filter(|&&index| plan.items[index].selected)
                                    .count();
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(format!(
                                        "{}: {selected_matches} / {}",
                                        t("Отмечено среди найденных", "Selected matches"),
                                        filtered.len(),
                                    ));
                                    if ui
                                        .add_enabled(
                                            available && selected_matches < filtered.len(),
                                            egui::Button::new(t(
                                                "Отметить найденные",
                                                "Select matches",
                                            ))
                                            .min_size(egui::vec2(150.0, 40.0)),
                                        )
                                        .clicked()
                                    {
                                        for &index in &filtered {
                                            plan.items[index].selected = true;
                                        }
                                    }
                                    if ui
                                        .add_enabled(
                                            available && selected_matches > 0,
                                            egui::Button::new(t(
                                                "Снять отметки с найденных",
                                                "Deselect matches",
                                            ))
                                            .min_size(egui::vec2(180.0, 40.0)),
                                        )
                                        .clicked()
                                    {
                                        for &index in &filtered {
                                            plan.items[index].selected = false;
                                        }
                                    }
                                });
                                ui.small(t(
                                    "Действует на все найденные строки, включая другие страницы. Остальные объекты не меняются.",
                                    "Affects every match across pages. Other objects stay unchanged.",
                                ));
                            }
                            let (start, end) =
                                organization_pages(ui, &mut self.organization_page, filtered.len());
                            let rows: Vec<_> = filtered[start..end]
                                .iter()
                                .map(|&index| {
                                    (
                                        index,
                                        plan.destination(&plan.items[index]),
                                        plan.will_move(&plan.items[index]),
                                    )
                                })
                                .collect();
                            for (index, destination, moving) in rows {
                                let item = &mut plan.items[index];
                                let label = if moving {
                                    // The default egui font on Windows 11 ARM did not paint the
                                    // arrow glyph. Use a widely supported breadcrumb marker.
                                    format!("{}  »  {destination}", item.name)
                                } else {
                                    // Keep the proposed folder visible even for unchecked rows
                                    // or checked singletons that cannot form a new folder.
                                    // The review search can match this field; its meaning
                                    // must not disappear when a checkbox changes.
                                    format!(
                                        "{} — {} · {}: {destination}",
                                        item.name,
                                        t("Остаётся", "Stays in place"),
                                        t("Папка в плане", "Planned folder"),
                                    )
                                };
                                ui.add_enabled(
                                    available,
                                    egui::Checkbox::new(&mut item.selected, label),
                                );
                            }
                            ui.small(format!(
                                "{}: {}–{} / {}",
                                t("Объекты в плане", "Plan items"),
                                if filtered.is_empty() { 0 } else { start + 1 },
                                end,
                                filtered.len(),
                            ));
                            if !skipped.is_empty() {
                                collapsing_header(
                                    ui,
                                    &format!(
                                        "{} ({})",
                                        t("Оставлены без перемещения", "Left in place"),
                                        skipped.len(),
                                    ),
                                )
                                // Keep filtered and unfiltered disclosure state separate.
                                // Even a search matching every excluded item should open
                                // the results once; later manual collapse remains respected.
                                .id_salt(if query.is_empty() {
                                    "organize-skipped-unfiltered"
                                } else {
                                    "organize-skipped-filtered"
                                })
                                .default_open(!query.is_empty())
                                .show(ui, |ui| {
                                    let id = egui::Id::new("organize-skipped-page");
                                    let mut page = ctx
                                        .data_mut(|data| data.get_temp::<usize>(id).unwrap_or(0));
                                    let (start, end) =
                                        organization_pages(ui, &mut page, skipped.len());
                                    ctx.data_mut(|data| data.insert_temp(id, page));
                                    for entry in &skipped[start..end] {
                                        ui.add(
                                            egui::Label::new(format!(
                                                "{} — {}",
                                                entry.name,
                                                organization_reason(&entry.reason),
                                            ))
                                            .wrap()
                                            .selectable(true),
                                        );
                                    }
                                    ui.small(format!("{}–{} / {}", start + 1, end, skipped.len()));
                                });
                            }
                        }
                        if let Some(id) = &current.undo {
                            ui.separator();
                            ui.label(t(
                                "Последнюю уборку можно отменить. Новые и изменённые файлы не перезаписываются.",
                                "Undo the last organization without overwriting new or changed files.",
                            ));
                            if ui
                                .add_enabled(
                                    available,
                                    egui::Button::new(t(
                                        "Отменить последнюю уборку",
                                        "Undo last organization",
                                    )),
                                )
                                .clicked()
                            {
                                undo = Some(id.clone());
                            }
                        }
                    }
                }
                if status.as_ref().is_ok_and(|state| !state.pending) {
                    ui.separator();
                    collapsing_header(ui, t("Ранее исключённые объекты", "Previously excluded objects"))
                        .show(ui, |ui| {
                            ui.label(t(
                                "Сброс не перемещает файлы, а позволяет предложить их снова при следующем анализе.",
                                "Reset moves no files. It only allows future plans to suggest them again.",
                            ));
                            ui.add_enabled(available, egui::Checkbox::new(
                                &mut self.organization_reset_confirm,
                                t("Сбросить все исключения", "Confirm resetting all exclusions"),
                            ));
                            if ui.add_enabled(
                                available && self.organization_reset_confirm,
                                egui::Button::new(t("Сбросить сохранённый выбор", "Reset remembered choices")),
                            ).clicked() {
                                reset = true;
                            }
                        });
                }
            },
        );
        if response.should_close() && available {
            close = true;
        }
        if close {
            self.organization_open = false;
            self.organization_accept_for = None;
            // Closing a review revokes consent to clear remembered exclusions.
            // A later visit must require a deliberate new confirmation.
            self.organization_reset_confirm = false;
        }
        if analyze {
            // A new analysis is a new review, even when a previously excluded
            // list happens to have the same number of pages. Do not return to
            // a later page chosen for an older desktop snapshot.
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("organize-skipped-page"), 0usize);
            });
            self.organization_analyze();
        }
        if apply {
            self.organization_apply();
        }
        if let Some(id) = undo {
            self.organization_undo(id);
        }
        if let Some(id) = accept {
            self.organization_accept(id);
        }
        if reset {
            self.organization_reset_exclusions();
        }
        response.response.rect
    }
}

#[cfg(test)]
mod tests;
