//! Read-only native screenshot views, restricted to an explicit test profile.
use super::*;
use crate::organize;
use std::sync::atomic::AtomicBool;

pub(super) fn organization(app: &mut Settings, view: &str) -> Result<()> {
    ensure!(
        app.capture.is_some(),
        "An explicit screenshot destination is required"
    );
    let fixture: String = app.store.get("test_desktop")?;
    ensure!(
        !fixture.is_empty(),
        "Organizer screenshots require a fixture desktop"
    );
    let home = std::fs::canonicalize(&app.store.home)?;
    let desktop = std::fs::canonicalize(&fixture)?;
    ensure!(
        desktop != home && desktop.starts_with(&home),
        "The screenshot desktop must be inside its isolated test profile"
    );
    let _com = native::Com::new()?;
    app.onboarding = false;
    app.organization_open = true;
    app.organization_files = view == "organization-files";
    let mut plan = organize::analyze(
        &app.store,
        organize::Options {
            include_files: app.organization_files,
        },
        &app.config,
        &AtomicBool::new(false),
    )?;
    if app.store.get::<bool>("test_singleton_review")? {
        // A capture-only scenario, after the explicit isolated-profile guards.
        // Use the production plan; never save an exclusion or move a file to
        // manufacture the view. Refuse an irrelevant fixture rather than
        // reporting a fully selected or mixed-group screenshot as this case.
        ensure!(
            plan.items.len() >= 2
                && plan
                    .items
                    .iter()
                    .all(|item| item.group == plan.items[0].group),
            "The singleton review capture needs at least two items in one group"
        );
        for (index, item) in plan.items.iter_mut().enumerate() {
            item.selected = index == 0;
        }
        ensure!(
            plan.summary() == (0, 0, 0),
            "The singleton review capture must not propose any moves"
        );
    }
    app.organization_plan = Some(plan);
    app.organization_projection = None;
    app.organization_query = if view == "organization-filtered" {
        app.store.get("test_search")?
    } else {
        String::new()
    };
    app.organization_page = 0;
    app.layout_receiver = None;
    app.find_after_layout = false;
    if view == "organization-warning" {
        app.fail("An object changed after preview; refresh the plan");
    }
    if app.capture_scroll_bottom {
        // Give the isolated native review driver time to deliver ordinary
        // window-scoped wheel messages through winit/egui. Do not substitute
        // a hand-drawn view or change scrolling in a normal Settings window.
        app.capture_at = Instant::now() + Duration::from_secs(6);
    }
    Ok(())
}

#[cfg(test)]
mod kept_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn organizer_capture_refuses_the_real_desktop_or_an_external_fixture() {
        let home = std::env::temp_dir().join(format!(
            "test-organizer-capture-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = Store::open(&home).unwrap();
        store.set("setup_complete", &true).unwrap();
        store.set("background_disabled", &true).unwrap();
        store.set("test_singleton_review", &true).unwrap();
        drop(store);
        let mut app = Settings::new(home.clone()).unwrap();
        app.layout_receiver = None;
        assert!(organization(&mut app, "organization").is_err());
        app.capture = Some(home.join("images"));
        assert!(organization(&mut app, "organization").is_err());
        app.store
            .set("test_desktop", &std::env::temp_dir().to_string_lossy())
            .unwrap();
        assert!(organization(&mut app, "organization").is_err());
        app.store
            .set("test_desktop", &home.to_string_lossy())
            .unwrap();
        assert!(organization(&mut app, "organization").is_err());
        drop(app);
        let _ = std::fs::remove_dir_all(home);
    }
}
