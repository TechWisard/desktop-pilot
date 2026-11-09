use super::*;

#[test]
fn large_changing_preview_summary_agrees_with_actual_movement_rules() {
    // Exercise the production Plan and Movement implementation, not a
    // separate approximation. A missing folder means no move, a reused
    // folder accepts singletons and a newly created folder does not.
    let mut plan = Plan {
        root: r"C:\Synthetic\Desktop".into(),
        root_id: "synthetic-root".into(),
        layout: None,
        items: Vec::new(),
        skipped: Vec::new(),
        options: Options {
            include_files: true,
        },
        folders: vec![
            Folder {
                group: Group::Sites,
                name: "Sites".into(),
                id: None,
                created: true,
                position: None,
            },
            Folder {
                group: Group::Games,
                name: "Games".into(),
                id: Some("existing-folder".into()),
                created: false,
                position: None,
            },
            Folder {
                group: Group::Images,
                name: "Images".into(),
                id: None,
                created: true,
                position: None,
            },
            Folder {
                group: Group::Archives,
                name: "Archives".into(),
                id: None,
                created: true,
                position: None,
            },
        ],
        russian: false,
    };
    let groups = [
        Group::Sites,
        Group::Games,
        Group::Images,
        Group::Archives,
        Group::Documents,
    ];
    for index in 0..6000 {
        plan.items.push(Item {
            name: format!("Synthetic-{index:04}.url"),
            group: groups[index % groups.len()],
            selected: false,
            position: None,
            stamp: fs::Stamp {
                id: format!("identity-{index}"),
                size: 0,
                modified: 0,
                digest: None,
            },
            launch: None,
        });
    }
    for mode in 0..8 {
        for (index, item) in plan.items.iter_mut().enumerate() {
            item.selected = match mode {
                0 => false,
                1 => true,
                2 => index % 7 == 0,
                3 => index % 3 != 0,
                4 => index == 0 || index == 1 || index == 4,
                5 => item.group == Group::Games,
                6 => item.group == Group::Sites || item.group == Group::Images,
                _ => index % 11 < 4,
            };
        }
        let movements = plan.movements();
        let new_folders = plan
            .folders
            .iter()
            .filter(|folder| {
                folder.created
                    && movements
                        .iter()
                        .any(|movement| movement.folder == folder.name)
            })
            .count();
        assert_eq!(
            plan.summary(),
            (
                movements.len(),
                new_folders,
                movements.len().saturating_sub(new_folders),
            ),
            "selection mode {mode} diverged from actual movement eligibility"
        );
    }
}

#[test]
fn created_folder_uses_first_known_source_position_and_never_promises_an_unknown_anchor() {
    // Model the real Plan and projected_zone used by the dialog. Explorer can
    // omit a position for one source without making its filesystem move unsafe.
    let root = r"C:\Synthetic\Desktop";
    let mut plan = Plan {
        root: root.into(),
        root_id: "synthetic-root".into(),
        layout: Some(native::Layout {
            items: vec![native::Item {
                path: format!(r"{root}\Known.url"),
                name: "Known.url".into(),
                position: Point { x: 80, y: 100 },
            }],
            spacing: Point { x: 80, y: 100 },
            origin: Point { x: 0, y: 0 },
            work: [0, 0, 1920, 1080],
            flags: 0,
            topology: "synthetic".into(),
            icon_size: 32,
        }),
        items: [
            ("Missing.url", None),
            ("Known.url", Some(Point { x: 80, y: 100 })),
        ]
        .into_iter()
        .map(|(name, position)| Item {
            name: name.into(),
            group: Group::Sites,
            selected: true,
            position,
            stamp: fs::Stamp {
                id: format!("synthetic-{name}"),
                size: 0,
                modified: 0,
                digest: None,
            },
            launch: None,
        })
        .collect(),
        skipped: Vec::new(),
        options: Options::default(),
        folders: vec![Folder {
            group: Group::Sites,
            name: "Sites".into(),
            id: None,
            created: true,
            position: None,
        }],
        russian: false,
    };
    let config = Config {
        slots: 1,
        zone_col: 4,
        zone_row: 0,
        zone_columns: 1,
        ..Default::default()
    };
    assert_eq!(plan.summary(), (2, 1, 1));
    assert!(!plan.has_unlocated_created_folder());
    let movements = plan.movements();
    assert!(movements[0].item.position.is_none());
    assert_eq!(
        known_folder_anchor(&movements, "Sites"),
        Some(Point { x: 80, y: 100 })
    );
    assert!(plan.projected_zone(&config).is_some());

    // When no moved source has a known position, Explorer chooses the folder's
    // eventual cell. The projection must no longer promise a free quick area.
    plan.items[1].position = None;
    assert_eq!(plan.summary(), (2, 1, 1));
    assert!(plan.has_unlocated_created_folder());
    assert_eq!(known_folder_anchor(&plan.movements(), "Sites"), None);
    assert!(plan.projected_zone(&config).is_none());

    // Reusing an existing folder does not create an unknown icon, and a
    // checked singleton in a new folder does not move at all.
    plan.folders[0].created = false;
    assert!(!plan.has_unlocated_created_folder());
    assert!(plan.projected_zone(&config).is_some());
    plan.folders[0].created = true;
    plan.items[1].selected = false;
    assert_eq!(plan.summary(), (0, 0, 0));
    assert!(!plan.has_unlocated_created_folder());
    assert!(plan.projected_zone(&config).is_some());
}
