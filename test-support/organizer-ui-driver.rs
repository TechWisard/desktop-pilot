//! Acceptance-only instrumentation, copied into an isolated source tree.
//! Drives the real controls with Windows mouse input, not direct action calls.
use super::*;
use std::collections::HashMap;
use windows::{
    Win32::{
        Foundation::POINT,
        Graphics::Gdi::ClientToScreen,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
                MOUSEEVENTF_WHEEL, MOUSEINPUT, SendInput,
            },
            WindowsAndMessaging::{
                FindWindowW, GetForegroundWindow, GetWindowThreadProcessId, SetCursorPos,
                SetForegroundWindow,
            },
        },
    },
    core::w,
};

fn trace_id() -> egui::Id { egui::Id::new("organizer-runtime-controls") }
fn state_id() -> egui::Id { egui::Id::new("organizer-runtime-driver") }
fn enabled_id() -> egui::Id { egui::Id::new("organizer-runtime-enabled") }

fn remember(ui: &egui::Ui, label: &str, response: &egui::Response) {
    if !response.enabled() || !ui.clip_rect().contains(response.rect.center()) { return; }
    ui.ctx().data_mut(|data| {
        if data.get_temp::<bool>(enabled_id()) != Some(true) { return; }
        let mut controls = data.get_temp::<HashMap<String, egui::Rect>>(trace_id()).unwrap_or_default();
        controls.insert(label.to_owned(), response.rect);
        data.insert_temp(trace_id(), controls);
    });
}

pub(super) fn primary(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    let response = super::primary(ui, enabled, label);
    remember(ui, label, &response);
    response
}

pub(super) fn secondary(ui: &mut egui::Ui, enabled: bool, label: &str) -> egui::Response {
    let response = ui.add_enabled(enabled, egui::Button::new(label));
    remember(ui, label, &response);
    response
}

pub(in crate::ui) fn is_interactive_capture(app: &Settings) -> bool {
    app.capture.is_some() && app.capture_view.as_deref() == Some("organization-interactive")
}

pub(in crate::ui) fn configure_capture(app: &mut Settings, view: &str) -> Result<()> {
    if view != "organization-interactive" { return Ok(()); }
    ensure!(app.capture.is_some(), "Acceptance requires explicit screenshot mode");
    let home = std::fs::canonicalize(&app.store.home)?;
    ensure!(home.file_name().is_some_and(|name| name.to_string_lossy().starts_with("test-")),
        "Acceptance cannot use an ordinary user profile");
    let desktop: String = app.store.get("test_desktop")?;
    ensure!(!desktop.is_empty() && std::fs::canonicalize(desktop)?.starts_with(&home),
        "Interactive acceptance is restricted to fixture files inside its profile");
    ensure!(app.store.get::<bool>("background_disabled")?, "Fixture background must be disabled");
    app.capture_view = Some(view.to_owned());
    app.organization_open = true;
    app.onboarding = false;
    app.organization_files = app.store.get("test_organization_files")?;
    Ok(())
}

#[derive(Clone)]
struct Driver {
    stage: u8,
    started: Instant,
    next: Instant,
    pending: Option<String>,
    captures: Vec<String>,
    scrolls: u32,
    clicks: u32,
    error: Option<String>,
}
impl Default for Driver {
    fn default() -> Self {
        Self { stage: 0, started: Instant::now(), next: Instant::now() + Duration::from_millis(700),
            pending: None, captures: Vec::new(), scrolls: 0, clicks: 0, error: None }
    }
}

fn mouse(ctx: &egui::Context, point: egui::Pos2, wheel: bool) -> Result<()> {
    unsafe {
        let window = FindWindowW(None, w!("Desktop Pilot"))?;
        let mut pid = 0;
        GetWindowThreadProcessId(window, Some(&mut pid));
        ensure!(pid == std::process::id(), "Refusing input into another process");
        ensure!(GetForegroundWindow() == window || SetForegroundWindow(window).as_bool(),
            "The acceptance window is not foreground");
        let mut origin = POINT::default();
        ensure!(ClientToScreen(window, &mut origin).as_bool(), "Cannot locate the actual client area");
        let scale = ctx.pixels_per_point();
        SetCursorPos(origin.x + (point.x * scale).round() as i32,
            origin.y + (point.y * scale).round() as i32)?;
        let event = |flags, data| INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 {
            mi: MOUSEINPUT { dwFlags: flags, mouseData: data, ..Default::default() }
        }};
        let events = if wheel { vec![event(MOUSEEVENTF_WHEEL, (-360i32) as u32)] }
            else { vec![event(MOUSEEVENTF_LEFTDOWN, 0), event(MOUSEEVENTF_LEFTUP, 0)] };
        ensure!(SendInput(&events, std::mem::size_of::<INPUT>() as i32) == events.len() as u32,
            "Windows rejected acceptance mouse input");
    }
    Ok(())
}

fn activate(driver: &mut Driver, ctx: &egui::Context,
    controls: &HashMap<String, egui::Rect>, english: &str, russian: &str) -> Result<bool> {
    if let Some(rect) = controls.get(english).or_else(|| controls.get(russian)) {
        mouse(ctx, rect.center(), false)?;
        driver.clicks += 1;
        driver.scrolls = 0;
        driver.next = Instant::now() + Duration::from_millis(500);
        return Ok(true);
    }
    ensure!(driver.scrolls < 12, "Control is not reachable by scrolling: {english}");
    mouse(ctx, ctx.screen_rect().center(), true)?;
    driver.scrolls += 1;
    driver.next = Instant::now() + Duration::from_millis(400);
    Ok(false)
}

fn capture(driver: &mut Driver, ctx: &egui::Context, name: &str) {
    driver.pending = Some(name.to_owned());
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
}

fn close(app: &mut Settings, ctx: &egui::Context, driver: &Driver) -> Result<()> {
    let directory = app.capture.as_ref().context("Capture directory missing")?;
    std::fs::write(directory.join("interaction.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "success": driver.error.is_none(), "error": driver.error,
        "language": app.config.language, "scale": app.ui_scale,
        "screenshots": driver.captures, "native_mouse_clicks": driver.clicks,
        "seconds": driver.started.elapsed().as_secs_f64(),
        "version": env!("CARGO_PKG_VERSION"),
        "organization_pending": organize::status(&app.store)?.pending,
        "background_disabled": app.store.get::<bool>("background_disabled")?
    }))?)?;
    app.allow_close = true;
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    Ok(())
}

fn step(app: &mut Settings, ctx: &egui::Context, driver: &mut Driver,
    controls: &HashMap<String, egui::Rect>) -> Result<()> {
    for event in ctx.input(|input| input.events.clone()) {
        if let egui::Event::Screenshot { image, .. } = event
            && let Some(name) = driver.pending.take() {
            let bytes: Vec<_> = image.pixels.iter().flat_map(|pixel| pixel.to_array()).collect();
            image::save_buffer(app.capture.as_ref().unwrap().join(format!("{name}.png")),
                &bytes, image.width() as u32, image.height() as u32, image::ColorType::Rgba8)?;
            driver.captures.push(name);
            driver.next = Instant::now() + Duration::from_millis(350);
            driver.stage = match driver.stage { 2 => 3, 5 => 6, 8 => 9, 250 => 251, other => other };
        }
    }
    if driver.stage == 9 || driver.stage == 251 { return close(app, ctx, driver); }
    ensure!(driver.started.elapsed() < Duration::from_secs(90), "Interactive acceptance timed out");
    if driver.pending.is_some() || Instant::now() < driver.next { return Ok(()); }
    if app.job.is_some() { return Ok(()); }
    if matches!(driver.stage, 1 | 4 | 7)
        && let Some(notice) = &app.notice
        && notice.kind == NoticeKind::Error { anyhow::bail!("Actual UI operation failed: {}", notice.text); }
    match driver.stage {
        0 => { if activate(driver, ctx, controls, "Analyze the desktop", "Проанализировать рабочий стол")? {
            driver.stage = 1;
        }}
        1 => { if let Some(plan) = &app.organization_plan {
            ensure!(plan.summary().0 >= 2, "The real preview found no movable fixture group");
            driver.stage = 2; capture(driver, ctx, "preview");
        }}
        3 => { if activate(driver, ctx, controls, "Apply selected moves", "Применить выбранное")? {
            driver.stage = 4;
        }}
        4 => { if app.organization_plan.is_none() && organize::status(&app.store)?.undo.is_some() {
            driver.stage = 5; capture(driver, ctx, "applied");
        }}
        6 => { if activate(driver, ctx, controls, "Undo last organization", "Отменить последнюю уборку")? {
            driver.stage = 7;
        }}
        7 => { let status = organize::status(&app.store)?;
            if !status.pending && status.undo.is_none() {
                driver.stage = 8; capture(driver, ctx, "restored");
            }
        }
        _ => {}
    }
    Ok(())
}

pub(in crate::ui) fn drive_capture(app: &mut Settings, ctx: &egui::Context) {
    if !is_interactive_capture(app) { return; }
    let (mut driver, controls) = ctx.data_mut(|data| {
        data.insert_temp(enabled_id(), true);
        let driver = data.get_temp::<Driver>(state_id()).unwrap_or_default();
        let controls = data.get_temp::<HashMap<String, egui::Rect>>(trace_id()).unwrap_or_default();
        data.insert_temp(trace_id(), HashMap::<String, egui::Rect>::new());
        (driver, controls)
    });
    if let Err(error) = step(app, ctx, &mut driver, &controls) {
        driver.error = Some(format!("{error:#}"));
        if driver.stage == 250 || driver.started.elapsed() > Duration::from_secs(95) {
            let _ = close(app, ctx, &driver);
        } else {
            driver.stage = 250;
            capture(&mut driver, ctx, "failure");
        }
    }
    ctx.data_mut(|data| data.insert_temp(state_id(), driver));
    ctx.request_repaint_after(Duration::from_millis(50));
}
