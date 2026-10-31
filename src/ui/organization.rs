use super::*;
use crate::organize;
use std::sync::atomic::AtomicBool;

const ORGANIZATION_PAGE_SIZE: usize = 40;

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

impl Settings {
    fn organization_analyze(&mut self) {
        if self.busy() {
            return;
        }
        self.organization_plan = None;
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
        self.start_job("Organizing desktop objects…", move || {
            let _com = native::Com::new()?;
            let (outcome, warning) = setup::with_organization_paused(&home, |store| {
                organize::apply_stopped(store, &plan, &AtomicBool::new(false))
            })?;
            Ok(JobResult::OrganizationChanged(outcome, warning))
        });
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
        let feedback = self.organization_feedback();
        let running = self.job.as_ref().map(|job| match job.label {
            "Analyzing the desktop…" => t("Анализируем рабочий стол…", "Analyzing the desktop…"),
            "Organizing desktop objects…" => t("Распределяем объекты…", "Organizing objects…"),
            "Restoring desktop objects…" => t("Восстанавливаем объекты…", "Restoring objects…"),
            _ => t("Выполняем действие…", "Working…"),
        });
        let response = dialog_frame(
            ctx,
            "desktop-organization",
            760.0,
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.heading(t("Навести порядок", "Organize desktop"));
                    if ui
                        .add_enabled(available, egui::Button::new(t("Закрыть", "Close")))
                        .clicked()
                    {
                        close = true;
                    }
                });
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
                            ui.add_enabled(
                                available,
                                egui::Checkbox::new(
                                    &mut self.organization_accept_confirm,
                                    t(
                                        "Я проверил файлы и хочу оставить их в текущем расположении.",
                                        "I checked the files and want to keep their current locations.",
                                    ),
                                ),
                            );
                            if ui
                                .add_enabled(
                                    available && self.organization_accept_confirm,
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
                            if plan.projected_zone(&self.config).is_none() {
                                ui.colored_label(WARNING, t(
                                    "Подходящая область для быстрых ярлыков пока не подтверждена. После уборки обновите схему; при необходимости выберите меньше ярлыков.",
                                    "A suitable quick-shortcut area is not confirmed yet. Refresh the layout after organizing; reduce the shortcut count if necessary.",
                                ));
                            }
                            if primary(
                                ui,
                                available && moves > 0,
                                t("Применить выбранное", "Apply selected moves"),
                            )
                            .clicked()
                            {
                                apply = true;
                            }
                            ui.label(t("Поиск по всему плану", "Search the entire plan"));
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
                                "Снимите галочку, чтобы оставить объект на столе. Выбор запоминается после успешного применения.",
                                "Uncheck an object to leave it on the desktop. Choices are remembered after a successful Apply.",
                            ));
                            let query = self.organization_query.trim().to_lowercase();
                            let filtered: Vec<_> = plan
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| {
                                    query.is_empty() || item.name.to_lowercase().contains(&query)
                                })
                                .map(|(index, _)| index)
                                .collect();
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
                            egui::ScrollArea::vertical()
                                .id_salt("organize-preview-scroll")
                                .max_height(250.0)
                                .show(ui, |ui| {
                                    for (index, destination, moving) in rows {
                                        let item = &mut plan.items[index];
                                        let label = if moving {
                                            format!("{}  →  {destination}", item.name)
                                        } else {
                                            format!(
                                                "{} — {}",
                                                item.name,
                                                t("Остаётся", "Stays in place")
                                            )
                                        };
                                        ui.add_enabled(
                                            available,
                                            egui::Checkbox::new(&mut item.selected, label),
                                        );
                                    }
                                });
                            ui.small(format!(
                                "{}: {}–{} / {}",
                                t("Объекты в плане", "Plan items"),
                                if filtered.is_empty() { 0 } else { start + 1 },
                                end,
                                filtered.len(),
                            ));
                            let skipped: Vec<_> = plan
                                .skipped
                                .iter()
                                .filter(|item| {
                                    query.is_empty() || item.name.to_lowercase().contains(&query)
                                })
                                .collect();
                            if !skipped.is_empty() {
                                collapsing_header(
                                    ui,
                                    &format!(
                                        "{} ({})",
                                        t("Оставлены без перемещения", "Left in place"),
                                        skipped.len(),
                                    ),
                                )
                                .show(ui, |ui| {
                                    let id = egui::Id::new("organize-skipped-page");
                                    let mut page = ctx
                                        .data_mut(|data| data.get_temp::<usize>(id).unwrap_or(0));
                                    let (start, end) =
                                        organization_pages(ui, &mut page, skipped.len());
                                    ctx.data_mut(|data| data.insert_temp(id, page));
                                    egui::ScrollArea::vertical()
                                        .id_salt("organize-skipped-scroll")
                                        .max_height(180.0)
                                        .show(ui, |ui| {
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
                                        });
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
            self.organization_accept_confirm = false;
        }
        if analyze {
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
