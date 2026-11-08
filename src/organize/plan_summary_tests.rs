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
                folder.created && movements.iter().any(|movement| movement.folder == folder.name)
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
