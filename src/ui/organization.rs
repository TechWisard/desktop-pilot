use super::*;
use crate::organize;
use std::sync::atomic::AtomicBool;

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

    pub(super) fn organization_dialog(&mut self, ctx: &egui::Context) {
        let mut analyze = false;
        let mut apply = false;
        let mut undo = None;
        let mut accept = None;
        let mut reset = false;
        let mut close = false;
        let available = !self.busy();
        let status = organize::status(&self.store);
        let response = dialog(
            ctx,
            "desktop-organization",
            760.0,
            t("Навести порядок", "Organize desktop"),
            |ui| {
                ui.label(t(
                    "Программа предлагает обычные папки и сначала показывает все перемещения. Исходные файлы не удаляются, существующие папки и закреплённые значки не трогаются.",
                    "Preview moves into ordinary folders first. No originals are deleted; existing folders and manually pinned icons stay in place.",
                ));
                ui.small(t(
                    "По умолчанию распределяются только безопасно распознанные ярлыки. Изменения выполняются по вашей кнопке; автоматической фоновой уборки нет.",
                    "Only confidently recognized shortcuts are selected by default. Nothing is moved until you confirm; there is no background cleanup.",
                ));
                ui.add_space(8.0);
                match status.as_ref() {
                    Err(error) => {
                        ui.colored_label(ERROR, format!("{error:#}"));
                    }
                    Ok(current) if current.pending => {
                        ui.colored_label(WARNING, t(
                            "Предыдущая уборка была прервана. Прежде чем продолжать, восстановите объекты или явно примите их текущее расположение.",
                            "An earlier organization was interrupted. Restore the objects or explicitly accept their current locations before continuing.",
                        ));
                        if let Some(id) = &current.undo {
                            if primary(ui, available, t("Восстановить", "Restore")).clicked()
                            {
                                undo = Some(id.clone());
                            }
                            ui.checkbox(
                                &mut self.organization_accept_confirm,
                                t(
                                    "Я проверил расположение файлов и хочу оставить их там, где они сейчас.",
                                    "I checked the files and want to keep their current locations.",
                                ),
                            );
                            if ui
                                .add_enabled(
                                    available && self.organization_accept_confirm,
                                    egui::Button::new(t(
                                        "Оставить текущее расположение",
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
                        ui.checkbox(
                            &mut self.organization_files,
                            t(
                                "Также распределить обычные документы и медиафайлы (необязательно)",
                                "Also include ordinary documents and media files (optional)",
                            ),
                        ).on_hover_text(t(
                            "Исполняемые файлы, сценарии, таблицы со связями, проекты и неизвестные типы остаются на месте.",
                            "Executables, scripts, linked spreadsheets, projects and unknown file types stay in place.",
                        ));
                        if self
                            .organization_plan
                            .as_ref()
                            .is_some_and(|p| p.options.include_files != self.organization_files)
                        {
                            self.organization_plan = None;
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
                            ui.add_space(8.0);
                            ui.heading(t("Предварительный план", "Preview"));
                            ui.label(format!(
                                "{}: {moves} · {}: {folders} · {}: {freed}",
                                t("Перемещений", "Moves"),
                                t("Новых папок", "New folders"),
                                t("Освободится мест", "Places freed"),
                            ));
                            ui.small(t(
                                "Оценка места учитывает новые папки. Реальная раскладка повторно проверяется перед применением.",
                                "New folders are counted in the space estimate. The actual desktop is checked again before Apply.",
                            ));
                            if plan.projected_zone(&self.config).is_none() {
                                ui.colored_label(WARNING, t(
                                    "Для выбранного числа быстрых ярлыков подходящей свободной области пока не найдено. Можно выбрать меньше ярлыков или освободить место дополнительно.",
                                    "There may still be insufficient space for the selected quick shortcuts. Reduce their number or free more room.",
                                ));
                            }
                            ui.horizontal_wrapped(|ui| {
                                ui.label(t("Поиск по плану:", "Filter the plan:"));
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.organization_query)
                                        .desired_width(260.0),
                                );
                            });
                            ui.small(t(
                                "Снимите галочку, чтобы оставить объект на рабочем столе; выбор будет сохранён после применения.",
                                "Uncheck an object to keep it on the desktop; the choice is remembered after Apply.",
                            ));
                            let destinations: Vec<_> = plan
                                .items
                                .iter()
                                .map(|item| plan.destination(item))
                                .collect();
                            let moving: Vec<_> =
                                plan.items.iter().map(|item| plan.will_move(item)).collect();
                            let query = self.organization_query.trim().to_lowercase();
                            let filtered: Vec<usize> = plan
                                .items
                                .iter()
                                .enumerate()
                                .filter(|(_, item)| {
                                    query.is_empty() || item.name.to_lowercase().contains(&query)
                                })
                                .map(|(index, _)| index)
                                .collect();
                            egui::ScrollArea::vertical()
                                .id_salt("organize-preview-scroll")
                                .max_height(250.0)
                                .show(ui, |ui| {
                                    for &index in filtered.iter().take(120) {
                                        let item = &mut plan.items[index];
                                        ui.checkbox(
                                            &mut item.selected,
                                            if moving[index] {
                                                format!("{}  →  {}", item.name, destinations[index])
                                            } else {
                                                format!(
                                                    "{} — {}",
                                                    item.name,
                                                    t("Пока остаётся", "Stays for now")
                                                )
                                            },
                                        );
                                    }
                                });
                            if filtered.len() > 120 {
                                ui.small(format!(
                                    "{} {}",
                                    t("Показано 120 из", "Showing 120 of"),
                                    filtered.len(),
                                ));
                            }
                            if !plan.skipped.is_empty() {
                                egui::CollapsingHeader::new(format!(
                                    "{} ({})",
                                    t("Оставлены без перемещения", "Left in place"),
                                    plan.skipped.len(),
                                ))
                                .show(ui, |ui| {
                                    egui::ScrollArea::vertical()
                                        .max_height(150.0)
                                        .show(ui, |ui| {
                                            for entry in plan.skipped.iter().take(60) {
                                                ui.small(format!(
                                                    "{} — {}",
                                                    entry.name, entry.reason
                                                ));
                                            }
                                        });
                                });
                            }
                            let (moves, _, _) = plan.summary();
                            if primary(
                                ui,
                                available && moves > 0,
                                t("Применить выбранные изменения", "Apply selected changes"),
                            )
                            .clicked()
                            {
                                apply = true;
                            }
                        }
                        if let Some(id) = &current.undo {
                            ui.separator();
                            ui.label(t(
                                "Последнюю завершённую уборку можно отменить. Новые или изменённые файлы не перезаписываются.",
                                "The last completed organization can be undone. New or modified files are never overwritten.",
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
                    egui::CollapsingHeader::new(t("Ранее исключённые объекты", "Previously excluded objects"))
                        .show(ui, |ui| {
                            ui.label(t(
                                "Программа помнит, что вы оставили на месте. Сброс не перемещает файлы, но позволяет предложить их снова в следующем анализе.",
                                "Objects you chose to leave are remembered. Reset moves no files; it only allows future plans to suggest them again.",
                            ));
                            ui.checkbox(
                                &mut self.organization_reset_confirm,
                                t("Сбросить все исключения", "Confirm resetting all exclusions"),
                            );
                            if ui.add_enabled(
                                available && self.organization_reset_confirm,
                                egui::Button::new(t("Сбросить сохранённый выбор", "Reset remembered choices")),
                            ).clicked() {
                                reset = true;
                            }
                        });
                }
                ui.add_space(8.0);
                if ui
                    .add_enabled(available, egui::Button::new(t("Закрыть", "Close")))
                    .clicked()
                {
                    close = true;
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
    }
}
