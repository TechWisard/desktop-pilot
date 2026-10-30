fn collect_text(shape: &egui::epaint::Shape, fragments: &mut Vec<String>) {
    match shape {
        egui::epaint::Shape::Text(text) => fragments.push(text.galley.text().to_owned()),
        egui::epaint::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_text(shape, fragments);
            }
        }
        _ => {}
    }
}

use super::*;

fn details_text(settings: &mut Settings, id: &str) -> String {
    let ctx = egui::Context::default();
    configure_style(&ctx);
    let mut rendered = String::new();
    for _ in 0..3 {
        settings.details_app = Some(id.to_owned());
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(920.0, 760.0),
                )),
                ..Default::default()
            },
            |ctx| settings.app_details(ctx),
        );
        let mut fragments = Vec::new();
        for shape in &output.shapes {
            collect_text(&shape.shape, &mut fragments);
        }
        rendered = fragments.join("\n");
    }
    rendered
}

#[test]
fn manual_owned_shortcut_details_explain_pin_and_extended_outage_without_hiding_explicit_rules() {
    let root = std::env::temp_dir().join(format!(
        "desktop-pilot-manual-detail-reasons-{}-{}",
        std::process::id(),
        now()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("Research.url");
    let original = b"[InternetShortcut]\r\nURL=https://example.invalid/Research\r\n";
    std::fs::write(&source, original).unwrap();
    let store = Store::open(&root).unwrap();
    store.set("setup_complete", &true).unwrap();
    store.set("config", &Config::default()).unwrap();
    let mut pinned = native::read_link(&source).unwrap();
    pinned.pinned = true;
    pinned.policy = Policy::Auto;
    assert!(pinned.valid);
    store.put_app(&pinned).unwrap();

    let unavailable_source = root.join("Offline.url");
    std::fs::write(
        &unavailable_source,
        b"[InternetShortcut]\r\nURL=https://example.invalid/Offline\r\n",
    )
    .unwrap();
    let mut offline = native::read_link(&unavailable_source).unwrap();
    offline.valid = false;
    offline.unavailable_since = now() - 10 * 86400;
    store.put_app(&offline).unwrap();

    let owned = [pinned.id.clone(), offline.id.clone()]
        .into_iter()
        .map(|app| Owned {
            app,
            path: root.join("Desktop").to_string_lossy().into_owned(),
            sha: String::new(),
            position: Point::default(),
            manual: false,
            since: now(),
            file_id: None,
        })
        .collect();
    store
        .set(
            "state",
            &State {
                owned,
                initialized: true,
                ..Default::default()
            },
        )
        .unwrap();
    drop(store);

    let mut settings = Settings::new(root.clone()).unwrap();
    assert!(!settings.saved_config.collect && !settings.saved_config.automatic);
    assert_eq!(
        settings
            .ranking
            .iter()
            .find(|ranked| ranked.id == pinned.id)
            .unwrap()
            .reason,
        "Уже закреплено на панели задач"
    );
    let text = details_text(&mut settings, &pinned.id);
    assert!(
        text.contains("не удалит") || text.contains("will not remove"),
        "Manual owned taskbar-pinned shortcut explanation missing: {text}"
    );

    let text = details_text(&mut settings, &offline.id);
    assert!(
        text.contains("остаётся на месте") || text.contains("stays in place"),
        "Manual owned shortcut outage explanation missing: {text}"
    );

    // Observe is allowed to reevaluate placement. A user-authored Never
    // remains authoritative even in Manual, and must never imply retention.
    let observed = Config {
        collect: true,
        ..Config::default()
    };
    settings.store.set("config", &observed).unwrap();
    settings.reload().unwrap();
    let observed_text = details_text(&mut settings, &pinned.id);
    assert!(!observed_text.contains("will not remove"));
    assert!(!observed_text.contains("не удалит"));
    settings.store.set("config", &Config::default()).unwrap();
    settings.store.policy(&pinned.id, Policy::Never).unwrap();
    settings.reload().unwrap();
    let never_text = details_text(&mut settings, &pinned.id);
    assert!(!never_text.contains("will not remove"));
    assert!(!never_text.contains("не удалит"));

    assert_eq!(std::fs::read(&source).unwrap(), original);
    drop(settings);
    std::fs::remove_dir_all(root).unwrap();
}
