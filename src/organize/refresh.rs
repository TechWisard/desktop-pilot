//! Wait for the shell's asynchronous enumeration, not just notification delivery.
use super::*;
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

fn same_entry(left: &str, right: &str) -> bool {
    if norm_literal_path(left) == norm_literal_path(right) {
        return true;
    }
    let left = Path::new(left);
    let right = Path::new(right);
    let (Some(a), Some(b), Some(pa), Some(pb)) = (
        left.file_name(),
        right.file_name(),
        left.parent(),
        right.parent(),
    ) else {
        return false;
    };
    if a.to_string_lossy().to_lowercase() != b.to_string_lossy().to_lowercase() {
        return false;
    }
    let identity = |path: &Path| {
        fs::directory(path)
            .ok()
            .and_then(|file| fs::identity(&file).ok())
    };
    identity(pa).is_some_and(|id| identity(pb).as_ref() == Some(&id))
}

// Explorer can invalidate an enumeration index while its folder view updates.
// Retry only this recognized transient COM result, never an arbitrary error.
fn transient_shell_bounds(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<windows::core::Error>()
            .is_some_and(|error| error.code().0 == 0x8000_000Bu32 as i32)
    })
}

fn poll(
    before: &native::Layout,
    appeared: &[String],
    removed: &[String],
    mut read: impl FnMut() -> Result<native::Layout>,
    mut retry: impl FnMut() -> bool,
) -> Result<native::Layout> {
    loop {
        let current = match read() {
            Ok(current) => current,
            Err(error) if transient_shell_bounds(&error) && retry() => continue,
            Err(error) => return Err(error),
        };
        ensure!(
            current.topology == before.topology && current.flags & 1 == 0,
            "Files are safe, but the display layout changed. Icon positions were not restored."
        );
        let contains = |path: &String| {
            current
                .items
                .iter()
                .any(|item| same_entry(&item.path, path))
        };
        if (appeared.iter().all(contains) && !removed.iter().any(contains)) || !retry() {
            return Ok(current);
        }
    }
}

pub(super) fn settled(
    journal: &Journal,
    undo: bool,
    restored: &HashSet<String>,
) -> Result<native::Layout> {
    let before = journal
        .layout
        .as_ref()
        .context("Missing desktop snapshot")?;
    let root = Path::new(&journal.root);
    let text = |name: &str| root.join(name).to_string_lossy().into_owned();
    let appeared: Vec<String> = if undo {
        journal
            .movements
            .iter()
            .filter(|m| restored.contains(&m.item.name))
            .map(|m| text(&m.item.name))
            .collect()
    } else {
        journal
            .folders
            .iter()
            .filter(|f| f.created)
            .map(|f| text(&f.name))
            .collect()
    };
    let removed: Vec<String> = if undo {
        journal
            .folders
            .iter()
            .filter(|f| f.created)
            .map(|f| text(&f.name))
            .collect()
    } else {
        journal
            .movements
            .iter()
            .map(|m| text(&m.item.name))
            .collect()
    };
    // An object recreated by the user is a genuine occupant, not stale shell data.
    let removed: Vec<_> = removed
        .into_iter()
        .filter(|p| fs::absent(Path::new(p)).unwrap_or(false))
        .collect();
    native::refresh(root);
    let deadline = Instant::now() + Duration::from_secs(3);
    poll(before, &appeared, &removed, native::layout, || {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
        true
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(name: &str) -> String {
        format!(r"C:\Example\Desktop\{name}")
    }
    fn layout(names: &[&str]) -> native::Layout {
        native::Layout {
            items: names
                .iter()
                .enumerate()
                .map(|(i, name)| native::Item {
                    path: path(name),
                    name: (*name).into(),
                    position: Point {
                        x: i as i32 * 100,
                        y: 0,
                    },
                })
                .collect(),
            spacing: Point { x: 100, y: 100 },
            origin: Point { x: 0, y: 0 },
            work: [0, 0, 1000, 1000],
            flags: 0,
            topology: "fixture".into(),
            icon_size: 32,
        }
    }
    #[test]
    fn transient_explorer_bounds_race_rechecks_real_layout_without_waiving_safety() {
        let before = layout(&["Alpha.url", "Personal"]);
        let mut reads = 0;
        let mut waits = 0;
        let result = poll(
            &before,
            &[path("Sites")],
            &[path("Alpha.url")],
            || {
                reads += 1;
                if reads == 1 {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x8000_000Bu32 as i32,
                    ))
                    .into());
                }
                Ok(layout(&["Sites", "Personal"]))
            },
            || {
                waits += 1;
                true
            },
        )
        .unwrap();
        assert_eq!((reads, waits), (2, 1));
        assert_eq!(result.items.len(), 2);

        let mut reads = 0;
        let mut waits = 0;
        let error = poll(
            &before,
            &[path("Sites")],
            &[path("Alpha.url")],
            || {
                reads += 1;
                Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                    0x8000_000Bu32 as i32,
                ))
                .into())
            },
            || {
                waits += 1;
                waits < 3
            },
        )
        .unwrap_err();
        assert!(transient_shell_bounds(&error));
        assert_eq!((reads, waits), (3, 3));
    }

    #[test]
    fn unexpected_shell_error_is_not_retried_or_mislabeled_as_a_view_race() {
        let before = layout(&["Alpha.url"]);
        let error = poll(
            &before,
            &[],
            &[],
            || anyhow::bail!("An unrelated filesystem error"),
            || panic!("The unrelated failure must not be retried"),
        )
        .unwrap_err();
        assert!(!transient_shell_bounds(&error));
        assert!(error.to_string().contains("unrelated filesystem"));
    }
    #[test]
    fn notification_delivery_may_precede_both_folder_creation_and_old_icon_removal() {
        let before = layout(&["Alpha.url", "Beta.url", "Personal"]);
        let mut snapshots = vec![
            before.clone(),
            layout(&["Alpha.url", "Sites", "Personal"]),
            layout(&["Sites", "Personal"]),
        ]
        .into_iter();
        let mut waits = 0;
        let result = poll(
            &before,
            &[path("Sites")],
            &[path("Alpha.url"), path("Beta.url")],
            || Ok(snapshots.next().unwrap()),
            || {
                waits += 1;
                waits < 3
            },
        )
        .unwrap();
        assert_eq!(waits, 2);
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[1].name, "Personal");
        assert!(snapshots.next().is_none());
    }
    #[test]
    fn timeout_keeps_the_latest_occupancy_instead_of_inventing_a_safe_position() {
        let before = layout(&["Alpha.url", "Personal"]);
        let delayed = layout(&["Alpha.url", "Sites", "Personal"]);
        let mut reads = 0;
        let mut waits = 0;
        let result = poll(
            &before,
            &[path("Sites")],
            &[path("Alpha.url")],
            || {
                reads += 1;
                Ok(delayed.clone())
            },
            || {
                waits += 1;
                waits < 3
            },
        )
        .unwrap();
        assert_eq!(reads, 3);
        assert_eq!(result.items[0].name, "Alpha.url");
    }
    #[test]
    fn topology_or_auto_arrange_change_is_not_retried_or_overridden() {
        let before = layout(&["Alpha.url"]);
        for auto_arrange in [false, true] {
            let mut changed = layout(&["Sites"]);
            if auto_arrange {
                changed.flags = 1;
            } else {
                changed.topology = "another-monitor".into();
            }
            assert!(
                poll(
                    &before,
                    &[path("Sites")],
                    &[],
                    || Ok(changed.clone()),
                    || panic!("unsafe layout must not be retried")
                )
                .is_err()
            );
        }
    }
    #[test]
    fn undo_waits_for_returned_originals_and_removal_of_only_empty_created_folders() {
        let before = layout(&["Sites", "Personal"]);
        let mut snapshots = vec![
            before.clone(),
            layout(&["Alpha.url", "Beta.url", "Sites", "Personal"]),
            layout(&["Alpha.url", "Beta.url", "Personal"]),
        ]
        .into_iter();
        let result = poll(
            &before,
            &[path("Alpha.url"), path("Beta.url")],
            &[path("Sites")],
            || Ok(snapshots.next().unwrap()),
            || true,
        )
        .unwrap();
        assert_eq!(result.items.len(), 3);
        assert_eq!(result.items[2].name, "Personal");
    }

    #[test]
    fn literal_variable_filename_is_not_a_shell_enumeration_alias() {
        let username = std::env::var("USERNAME").expect("Windows username is required");
        let actual = path(&format!("{username}.url"));
        let literal = path("%USERNAME%.url");
        assert_ne!(literal, actual);
        assert!(!same_entry(&literal, &actual));
        assert!(same_entry(&literal, &literal));
    }
}
