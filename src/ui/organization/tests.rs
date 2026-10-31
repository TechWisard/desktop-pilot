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
    let time = ctx.input(|input| input.time) + 1.0 / 60.0;
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
            for _ in 0..3 {
                let output = render(&ctx, &mut f.app, viewport, Vec::new());
                let text = rendered_text(&output);
                let (_, feedback) = text
                    .iter()
                    .find(|(value, _)| {
                        value.contains("Действие не завершено")
                            || value.contains("The action did not complete")
                    })
                    .expect("Operation feedback must be painted after the sizing pass");
                assert!(ctx.screen_rect().contains_rect(*feedback));
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
