use super::*;
use std::fs;

struct Cleanup(PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    app: Settings,
    desktop: PathBuf,
    _com: native::Com,
    _cleanup: Cleanup,
}
impl Fixture {
    fn new() -> Self {
        let home = std::env::temp_dir().join(format!(
            "test-organizer-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let desktop = home.join("Desktop");
        fs::create_dir_all(&desktop).unwrap();
        let store = Store::open(&home).unwrap();
        store
            .set("test_desktop", &desktop.to_string_lossy())
            .unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("background_disabled", &true).unwrap();
        store.set("config", &Config::default()).unwrap();
        let com = native::Com::new().unwrap();
        for name in ["Alpha", "Beta"] {
            let path = desktop.join(format!("{name}.url"));
            fs::write(
                &path,
                format!("[InternetShortcut]\r\nURL=https://example.invalid/{name}\r\n"),
            )
            .unwrap();
            store.put_app(&native::read_link(&path).unwrap()).unwrap();
        }
        drop(store);
        let mut app = Settings::new(home.clone()).unwrap();
        app.layout_receiver = None;
        app.find_after_layout = false;
        app.organization_open = true;
        Self {
            app,
            desktop,
            _com: com,
            _cleanup: Cleanup(home),
        }
    }

    fn finish_job(&mut self) {
        let ctx = egui::Context::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        while self.app.job.is_some() {
            self.app.poll_jobs(&ctx);
            assert!(
                Instant::now() < deadline,
                "organizer UI worker did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        self.app.layout_receiver = None;
        self.app.find_after_layout = false;
    }

    fn analyze(&mut self) {
        self.app.organization_analyze();
        self.finish_job();
        assert_eq!(
            self.app.organization_plan.as_ref().unwrap().summary(),
            (2, 1, 1)
        );
    }
}

fn rendered_text(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn collect_visible(
        shape: &egui::epaint::Shape,
        clip: egui::Rect,
        result: &mut Vec<(String, egui::Rect)>,
    ) {
        match shape {
            egui::epaint::Shape::Text(text) => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                if clip.intersects(rect) {
                    result.push((text.galley.job.text.clone(), rect));
                }
            }
            egui::epaint::Shape::Vec(children) => {
                for child in children {
                    collect_visible(child, clip, result);
                }
            }
            _ => {}
        }
    }

    let mut result = Vec::new();
    for shape in &output.shapes {
        collect_visible(&shape.shape, shape.clip_rect, &mut result);
    }
    result
}

fn render(
    ctx: &egui::Context,
    app: &mut Settings,
    viewport: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    // Areas use an initial sizing pass and a timed fade-in. Advance input time
    // as the real event loop does, retaining the parallel fixture correction.
    let time = ctx.input(|input| input.time) + 0.25;
    ctx.run(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            time: Some(time),
            events,
            ..Default::default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.label("Desktop Pilot test host");
            });
            let rect = app.organization_dialog(ctx);
            assert!(rect.width() <= ctx.screen_rect().width() + 1.0, "{rect:?}");
            assert!(
                rect.height() <= ctx.screen_rect().height() + 1.0,
                "{rect:?}"
            );
        },
    )
}

fn settle(ctx: &egui::Context, app: &mut Settings, viewport: egui::Vec2) {
    // A newly created egui Area has an intentionally invisible sizing pass.
    // Geometry is checked above during it. Every tested frame below must then
    // contain real painted feedback and controls, not merely an empty result.
    for _ in 0..3 {
        let _ = render(ctx, app, viewport, Vec::new());
    }
}

fn click(ctx: &egui::Context, app: &mut Settings, viewport: egui::Vec2, at: egui::Pos2) {
    for pressed in [true, false] {
        let _ = render(
            ctx,
            app,
            viewport,
            vec![
                egui::Event::PointerMoved(at),
                egui::Event::PointerButton {
                    pos: at,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

fn click_organization_button(
    ctx: &egui::Context,
    app: &mut Settings,
    viewport: egui::Vec2,
    label: &str,
) {
    settle(ctx, app, viewport);
    let output = render(ctx, app, viewport, Vec::new());
    let painted = rendered_text(&output);
    let at = painted
        .iter()
        .find(|(text, _)| text == label)
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| {
            panic!(
                "Missing visible organizer control '{label}': {:?}",
                painted.iter().map(|(text, _)| text).collect::<Vec<_>>()
            )
        });
    click(ctx, app, viewport, at);
}

#[test]
fn one_bulk_choice_changes_the_entire_real_plan_even_with_hidden_rows() {
    let mut f = Fixture::new();
    f.analyze();
    let ctx = egui::Context::default();
    configure_style(&ctx);
    i18n::set("en");
    f.app.config.language = "en".into();
    let viewport = egui::vec2(1200.0, 900.0);
    let original_sources: Vec<_> = ["Alpha.url", "Beta.url"]
        .iter()
        .map(|name| fs::read(f.desktop.join(name)).unwrap())
        .collect();

    // Search hides both rows, yet the global action must update both, not
    // just the visible page/filter. No filesystem mutation occurs on selection.
    f.app.organization_query = "unmatched".into();
    click_organization_button(&ctx, &mut f.app, viewport, "Deselect all");
    let plan = f.app.organization_plan.as_ref().unwrap();
    assert_eq!(plan.items.len(), 2);
    assert!(plan.items.iter().all(|item| !item.selected));
    assert_eq!(plan.summary(), (0, 0, 0));
    assert_eq!(f.app.organization_query, "unmatched");

    click_organization_button(&ctx, &mut f.app, viewport, "Select all");
    let plan = f.app.organization_plan.as_ref().unwrap();
    assert!(plan.items.iter().all(|item| item.selected));
    assert_eq!(plan.summary(), (2, 1, 1));

    // A partial filter must not allow a hidden selected row to remain when
    // the user asks to keep the whole plan on the desktop.
    f.app.organization_query = "Alpha".into();
    click_organization_button(&ctx, &mut f.app, viewport, "Deselect all");
    assert!(
        f.app
            .organization_plan
            .as_ref()
            .unwrap()
            .items
            .iter()
            .all(|item| !item.selected)
    );
    f.app.organization_apply();
    f.finish_job();
    for (name, contents) in ["Alpha.url", "Beta.url"]
        .iter()
        .zip(original_sources.iter())
    {
        assert_eq!(&fs::read(f.desktop.join(name)).unwrap(), contents);
    }
    assert!(!f.desktop.join("Sites").exists());
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());

    f.app.organization_analyze();
    f.finish_job();
    let repeated = f.app.organization_plan.as_ref().unwrap();
    assert!(repeated.items.is_empty());
    assert_eq!(repeated.skipped.len(), 2);
}

#[test]
fn all_six_thousand_preview_and_skipped_items_remain_accessible() {
    for count in [0usize, 1, 39, 40, 41, 60, 61, 5999, 6000] {
        let pages = count.div_ceil(ORGANIZATION_PAGE_SIZE).max(1);
        let mut visited = Vec::new();
        for requested in 0..pages {
            let (page, actual_pages, start, end) = organization_page_bounds(count, requested);
            assert_eq!(page, requested);
            assert_eq!(actual_pages, pages);
            visited.extend(start..end);
        }
        assert_eq!(visited, (0..count).collect::<Vec<_>>());
        assert_eq!(organization_page_bounds(count, usize::MAX).0, pages - 1);
    }
}

#[test]
fn actual_settings_jobs_analyze_apply_and_undo_without_resuming_a_disabled_profile() {
    let mut f = Fixture::new();
    let before = f.app.store.apps().unwrap();
    let config = f.app.store.get::<Config>("config").unwrap();
    f.analyze();
    assert!(f.desktop.join("Alpha.url").is_file());
    f.app.organization_apply();
    f.finish_job();
    assert!(f.app.notice.as_ref().unwrap().kind == NoticeKind::Success);
    assert!(f.app.organization_plan.is_none());
    assert!(f.desktop.join("Sites/Alpha.url").is_file());
    assert!(!f.desktop.join("Alpha.url").exists());
    let id = organize::status(&f.app.store).unwrap().undo.unwrap();
    f.app.organization_undo(id);
    f.finish_job();
    assert!(f.app.notice.as_ref().unwrap().kind == NoticeKind::Success);
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
    assert_eq!(f.app.store.apps().unwrap(), before);
    assert_eq!(f.app.store.get::<Config>("config").unwrap(), config);
    assert!(f.app.store.get::<bool>("background_disabled").unwrap());
    assert!(!native::is_running("background", &f.app.store.home).unwrap());
}

#[test]
fn failed_apply_returns_visible_modal_feedback_and_preserves_every_object() {
    let mut f = Fixture::new();
    f.analyze();
    let changed = f.desktop.join("Beta.url");
    fs::write(&changed, b"changed after preview").unwrap();
    f.app.organization_apply();
    f.finish_job();
    assert!(f.app.notice.as_ref().unwrap().kind == NoticeKind::Error);
    assert!(f.app.organization_open);
    assert!(f.desktop.join("Alpha.url").is_file());
    assert_eq!(fs::read(&changed).unwrap(), b"changed after preview");
    assert!(!f.desktop.join("Sites").exists());
    assert!(!organize::status(&f.app.store).unwrap().pending);
    let (summary, color, details) = f.app.organization_feedback().unwrap();
    assert_eq!(color, ERROR);
    assert!(!summary.is_empty());
    assert!(details.contains("changed after preview"));
    let ctx = egui::Context::default();
    configure_style(&ctx);
    let viewport = egui::vec2(480.0, 360.0);
    settle(&ctx, &mut f.app, viewport);
    for _ in 0..3 {
        let output = render(&ctx, &mut f.app, viewport, Vec::new());
        let text = rendered_text(&output);
        assert!(
            text.iter().any(|(value, _)| {
                value.contains("Действие не завершено")
                    || value.contains("The action did not complete")
            }),
            "No visible operation feedback. Painted text: {:?}",
            text.iter().map(|(value, _)| value).collect::<Vec<_>>()
        );
    }
}

#[test]
fn organization_modal_keeps_close_feedback_and_progress_inside_scaled_viewports() {
    let mut f = Fixture::new();
    f.analyze();
    for language in ["ru", "en"] {
        for scale in [1.0, 2.0] {
            let ctx = egui::Context::default();
            configure_style(&ctx);
            ctx.set_zoom_factor(scale);
            let viewport = egui::vec2(960.0 / scale, 720.0 / scale);
            f.app.organization_open = true;
            f.app.config.language = language.into();
            i18n::set(language);
            f.app.fail("A source changed; no object was overwritten.");
            settle(&ctx, &mut f.app, viewport);
            for frame in 0..3 {
                let output = render(&ctx, &mut f.app, viewport, Vec::new());
                let text = rendered_text(&output);
                let feedback = text.iter().find(|(value, _)| {
                    value.contains("Действие не завершено")
                        || value.contains("The action did not complete")
                });
                assert!(
                    feedback.is_some(),
                    "language={language}, scale={scale}, frame={frame}, painted={:?}, shapes={}",
                    text.iter()
                        .map(|(value, _)| value.as_str())
                        .collect::<Vec<_>>(),
                    output.shapes.len()
                );
                assert!(ctx.screen_rect().contains_rect(feedback.unwrap().1));
                let (_, close) = text
                    .iter()
                    .find(|(value, _)| matches!(value.as_str(), "Close" | "Закрыть"))
                    .expect("Close must remain visible without scrolling");
                assert!(ctx.screen_rect().contains_rect(*close));
            }
            // A real pending Job must show progress in this modal, not only
            // in the blocked parent window. No fixture work is executed here.
            let (_sender, receiver) = mpsc::channel();
            f.app.notice = None;
            f.app.job = Some(Job {
                label: "Analyzing the desktop…",
                started: Instant::now(),
                receiver,
            });
            settle(&ctx, &mut f.app, viewport);
            for _ in 0..3 {
                let output = render(&ctx, &mut f.app, viewport, Vec::new());
                assert!(rendered_text(&output).iter().any(|(value, _)| {
                    value.contains("Анализируем рабочий стол")
                        || value.contains("Analyzing the desktop")
                }));
            }
            f.app.job = None;
            settle(&ctx, &mut f.app, viewport);
            let output = render(&ctx, &mut f.app, viewport, Vec::new());
            let text = rendered_text(&output);
            let (_, close) = text
                .iter()
                .find(|(value, _)| matches!(value.as_str(), "Close" | "Закрыть"))
                .expect("Close is painted before the pointer action");
            click(&ctx, &mut f.app, viewport, close.center());
            assert!(
                !f.app.organization_open,
                "The visible Close button must work"
            );
            assert!(f.desktop.join("Alpha.url").is_file());
            assert!(f.desktop.join("Beta.url").is_file());
        }
    }
}

#[test]
fn completed_operation_with_warnings_is_not_presented_as_unqualified_success() {
    let mut f = Fixture::new();
    f.app.inform(
        "Moved: 2\nFiles are safe; an occupied icon position was preserved.",
        NoticeKind::Success,
    );
    let (_, color, details) = f.app.organization_feedback().unwrap();
    assert_eq!(color, WARNING);
    assert!(details.contains("occupied icon position"));
}

#[test]
fn all_unchecked_suggestions_save_locally_without_a_folder_or_undo() {
    let mut f = Fixture::new();
    f.analyze();
    let plan = f.app.organization_plan.as_mut().unwrap();
    for item in &mut plan.items {
        item.selected = false;
    }
    assert_eq!(plan.summary(), (0, 0, 0));
    f.app.organization_apply();
    f.finish_job();
    assert!(f.app.notice.as_ref().unwrap().kind == NoticeKind::Success);
    let message = &f.app.notice.as_ref().unwrap().text;
    assert!(message.contains("Keep choices saved") || message.contains("Исключения сохранены"));
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());
    assert!(!organize::status(&f.app.store).unwrap().pending);
    assert!(f.app.store.get::<bool>("background_disabled").unwrap());
    f.app.organization_analyze();
    f.finish_job();
    assert!(f.app.organization_plan.as_ref().unwrap().items.is_empty());
}

#[test]
fn remembering_keep_choices_does_not_interrupt_a_running_background_mutex() {
    let mut f = Fixture::new();
    f.analyze();
    let original_alpha = fs::read(f.desktop.join("Alpha.url")).unwrap();
    let original_beta = fs::read(f.desktop.join("Beta.url")).unwrap();
    for item in &mut f.app.organization_plan.as_mut().unwrap().items {
        item.selected = false;
    }
    // Simulate the real background process holding its singleton mutex. A
    // keep-only Settings job must complete while this guard is still held,
    // without requesting a shutdown or waiting for a worker restart.
    let background = native::exclusive("background", &f.app.store.home).unwrap();
    f.app.organization_apply();
    f.finish_job();
    assert!(f.app.notice.as_ref().unwrap().kind == NoticeKind::Success);
    assert_eq!(
        fs::read(f.desktop.join("Alpha.url")).unwrap(),
        original_alpha
    );
    assert_eq!(fs::read(f.desktop.join("Beta.url")).unwrap(), original_beta);
    assert!(!f.desktop.join("Sites").exists());
    assert!(!organize::status(&f.app.store).unwrap().pending);
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());
    assert!(!f.app.store.get::<bool>("shutdown").unwrap());
    drop(background);
    // A successful remembered choice intentionally removes all suggestions;
    // Fixture::analyze requires two suggestions and is for first-run only.
    f.app.organization_analyze();
    f.finish_job();
    assert!(f.app.organization_plan.as_ref().unwrap().items.is_empty());
}

#[test]
fn keep_only_entry_point_rejects_a_plan_that_would_move_files() {
    let mut f = Fixture::new();
    f.analyze();
    let before = fs::read(f.desktop.join("Alpha.url")).unwrap();
    let plan = f.app.organization_plan.as_ref().unwrap();
    assert_eq!(plan.summary(), (2, 1, 1));
    let error = organize::remember_keep(&f.app.store, plan).unwrap_err();
    assert!(format!("{error:#}").contains("cannot move"));
    assert_eq!(fs::read(f.desktop.join("Alpha.url")).unwrap(), before);
    assert!(!organize::status(&f.app.store).unwrap().pending);
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());
}

#[test]
fn new_analysis_reveals_all_candidates_after_a_stale_filtered_preview() {
    let mut f = Fixture::new();
    f.analyze();
    let before = f.app.organization_plan.as_ref().unwrap().summary();
    assert_eq!(before, (2, 1, 1));

    // After viewing one project, the user returns later to re-analyze.
    // A stale search can hide every new selected move while Apply still acts
    // on the unfiltered plan. Starting a fresh preview must reveal it all.
    f.app.organization_query = "unrelated previous project".into();
    f.app.organization_page = usize::MAX;
    f.app.organization_analyze();
    f.finish_job();

    assert!(f.app.organization_query.is_empty());
    assert_eq!(f.app.organization_page, 0);
    let plan = f.app.organization_plan.as_ref().unwrap();
    assert_eq!(plan.summary(), before);
    assert_eq!(plan.items.len(), 2);
    assert!(plan.items.iter().all(|item| item.selected));
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
    assert!(organize::status(&f.app.store).unwrap().undo.is_none());
}

#[test]
fn unchanged_organizer_frames_reuse_production_projection_but_edits_invalidate_it() {
    let mut fixture = Fixture::new();
    fixture.analyze();
    let mut plan = fixture.app.organization_plan.as_ref().unwrap().clone();
    let mut config = fixture.app.config.clone();
    let mut cache = None;
    let mut calculations = 0;

    let expected = plan.projected_zone(&config).is_some();
    // egui repaints even when nothing is clicked. Each calculation below
    // executes the real projection, including filesystem identity checks;
    // 40 unchanged frames should not re-open those paths 40 times.
    for _ in 0..40 {
        assert_eq!(
            cached_organization_projection(&plan, &config, &mut cache, |plan, config| {
                calculations += 1;
                plan.projected_zone(config).is_some()
            }),
            expected
        );
    }
    assert_eq!(calculations, 1);

    // A changed selection must refresh the estimated free zone immediately,
    // not reuse the earlier proposal.
    plan.items[0].selected = false;
    let after_selection = plan.projected_zone(&config).is_some();
    assert_eq!(
        cached_organization_projection(&plan, &config, &mut cache, |plan, config| {
            calculations += 1;
            plan.projected_zone(config).is_some()
        }),
        after_selection
    );
    assert_eq!(calculations, 2);

    config.zone_row = (config.zone_row + 1).min(40);
    let after_configuration = plan.projected_zone(&config).is_some();
    assert_eq!(
        cached_organization_projection(&plan, &config, &mut cache, |plan, config| {
            calculations += 1;
            plan.projected_zone(config).is_some()
        }),
        after_configuration
    );
    assert_eq!(calculations, 3);

    // A fresh analysis invalidates the cache even when the next plan has
    // exactly the same selected flags and display configuration.
    fixture.app.organization_projection = cache;
    fixture.app.organization_analyze();
    assert!(fixture.app.organization_projection.is_none());
    fixture.finish_job();
    assert!(fixture.app.organization_projection.is_none());
    assert!(fixture.desktop.join("Alpha.url").is_file());
    assert!(fixture.desktop.join("Beta.url").is_file());
    assert!(!fixture.desktop.join("Sites").exists());
    assert!(!organize::status(&fixture.app.store).unwrap().pending);
}

#[test]
fn recovery_confirmation_cannot_authorize_a_different_pending_journal() {
    let mut f = Fixture::new();
    f.app.config.language = "en".into();
    i18n::set("en");
    let ctx = egui::Context::default();
    configure_style(&ctx);
    let viewport = egui::vec2(960.0, 720.0);

    // The first recovery was explicitly acknowledged in this open dialog.
    f.app
        .store
        .set("organization.active.v1", &Some("1001".to_owned()))
        .unwrap();
    settle(&ctx, &mut f.app, viewport);
    f.app.organization_accept_for = Some("1001".to_owned());
    let _ = render(&ctx, &mut f.app, viewport, Vec::new());
    assert_eq!(f.app.organization_accept_for.as_deref(), Some("1001"));

    // Another Settings process can resolve the first operation and create a
    // newer recovery. The old checkbox must not silently approve the new ID.
    f.app
        .store
        .set("organization.active.v1", &Some("1002".to_owned()))
        .unwrap();
    let output = render(&ctx, &mut f.app, viewport, Vec::new());
    assert!(f.app.organization_accept_for.is_none());
    let painted = rendered_text(&output);
    let (_, button) = painted
        .iter()
        .find(|(value, _)| value == "Accept current locations")
        .expect("Recovery button must be shown");
    click(&ctx, &mut f.app, viewport, button.center());
    assert!(
        f.app.job.is_none(),
        "stale consent must not start a recovery job"
    );
    assert_eq!(
        f.app
            .store
            .get::<Option<String>>("organization.active.v1")
            .unwrap()
            .as_deref(),
        Some("1002")
    );

    // Clearing recovery also clears consent. If the same identifier returns
    // later, it still requires a new user confirmation.
    f.app.organization_accept_for = Some("1002".to_owned());
    f.app
        .store
        .set("organization.active.v1", &Option::<String>::None)
        .unwrap();
    let _ = render(&ctx, &mut f.app, viewport, Vec::new());
    assert!(f.app.organization_accept_for.is_none());
    f.app
        .store
        .set("organization.active.v1", &Some("1002".to_owned()))
        .unwrap();
    let _ = render(&ctx, &mut f.app, viewport, Vec::new());
    assert!(f.app.organization_accept_for.is_none());
    assert!(f.desktop.join("Alpha.url").is_file());
    assert!(f.desktop.join("Beta.url").is_file());
    assert!(!f.desktop.join("Sites").exists());
}
