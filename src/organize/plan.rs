use super::*;
use crate::model::App;
use anyhow::bail;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

fn plain_group(extension: &str) -> Option<Group> {
    match extension {
        "txt" | "pdf" | "rtf" | "doc" | "docx" | "odt" | "ppt" | "pptx" => Some(Group::Documents),
        "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tif" | "tiff" | "heic" => {
            Some(Group::Images)
        }
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "mp4" | "mkv" | "avi" | "mov" | "webm" => {
            Some(Group::Media)
        }
        "zip" | "7z" | "rar" | "tar" | "gz" => Some(Group::Archives),
        _ => None,
    }
}

pub(super) fn classify(path: &Path, options: &Options) -> Result<(Group, Option<String>)> {
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "lnk" | "url") {
        let app = metadata::read(path)?;
        let target = app.target.trim();
        let lower = target.to_ascii_lowercase();
        let absolute = |value: &str| {
            value.is_empty() || Path::new(&crate::model::expand_env(value)).is_absolute()
        };
        ensure!(
            absolute(&app.cwd) && absolute(&app.icon),
            "Relative working directory or icon: left in place"
        );
        let group = if lower.starts_with("https://") || lower.starts_with("http://") {
            Group::Sites
        } else if (lower.starts_with("steam://rungameid/")
            && target.len() > 18
            && target[18..].bytes().all(|byte| byte.is_ascii_digit()))
            || lower.starts_with("com.epicgames.launcher://apps/")
        {
            Group::Games
        } else if Path::new(&crate::model::expand_env(target)).is_absolute()
            || lower.starts_with("shell:")
            || lower.starts_with("::{")
        {
            ensure!(
                extension != "lnk" || !app.cwd.is_empty(),
                "Shortcut without an explicit working directory: left in place"
            );
            Group::Programs
        } else {
            bail!("Relative or unrecognized shortcut target: left in place");
        };
        Ok((group, Some(app.identity())))
    } else {
        ensure!(
            options.include_files,
            "Files are not selected for organization"
        );
        let group = plain_group(&extension)
            .context("Executable, linked data or unknown file type: left in place")?;
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        for companion in [
            format!("{stem}_files"),
            format!("{stem}.files"),
            format!("{stem}_файлы"),
        ] {
            ensure!(
                fs::absent(&path.with_file_name(companion))?,
                "Associated file set: left in place"
            );
        }
        Ok((group, None))
    }
}

#[derive(Default)]
pub(super) struct Dependencies {
    paths: HashSet<String>,
    desktop_working_directory: bool,
}
impl Dependencies {
    fn record(&mut self, app: &App, root: &Path) {
        for value in [&app.target, &app.cwd, &app.icon] {
            let expanded = crate::model::expand_env(value);
            let path = Path::new(&expanded);
            if path.is_absolute() {
                self.paths.insert(norm(&expanded));
            }
        }
        self.desktop_working_directory |=
            !app.cwd.is_empty() && norm(&app.cwd) == norm(&root.to_string_lossy());
    }
    pub(super) fn check(&self, path: &Path, shortcut: bool) -> Result<()> {
        ensure!(
            !self.paths.contains(&norm(&path.to_string_lossy())),
            "Another known shortcut depends on this object: left in place"
        );
        ensure!(
            shortcut || !self.desktop_working_directory,
            "The desktop is used as an application's working directory: files left in place"
        );
        Ok(())
    }
}

pub(super) fn paths(root: &Path, cancel: &AtomicBool) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(root)? {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Organization analysis cancelled"
        );
        paths.push(entry?.path());
        ensure!(
            paths.len() <= LIMIT,
            "The desktop contains more than 6000 objects. No incomplete plan was created."
        );
    }
    paths.sort_by_key(|path| norm(&path.to_string_lossy()));
    Ok(paths)
}

pub(super) fn dependencies(
    root: &Path,
    paths: &[PathBuf],
    apps: &[App],
    cancel: &AtomicBool,
) -> Result<Dependencies> {
    let mut dependencies = Dependencies::default();
    for app in apps {
        dependencies.record(app, root);
    }
    for path in paths.iter().filter(|p| is_shortcut(p)) {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Organization analysis cancelled"
        );
        if fs::inspect(path, true).is_ok()
            && let Ok(app) = metadata::read(path)
        {
            dependencies.record(&app, root);
        }
    }
    Ok(dependencies)
}

pub(super) fn is_shortcut(path: &Path) -> bool {
    path.extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("lnk") || s.eq_ignore_ascii_case("url"))
}

pub fn analyze(
    store: &Store,
    options: Options,
    config: &Config,
    cancel: &AtomicBool,
) -> Result<Plan> {
    let _changes = native::exclusive("changes", &store.home)?;
    ensure_ready(store)?;
    no_placement_in_progress(store)?;
    config.validate()?;
    let root = root(store)?;
    let root_handle = fs::directory(&root)?;
    let root_id = fs::identity(&root_handle)?;
    let layout = layout(store)?;
    let state: State = store.get("state")?;
    let apps = store.apps()?;
    let keep: HashSet<String> = store.get::<Vec<String>>(KEEP)?.into_iter().collect();
    let managed_paths: HashSet<_> = state.owned.iter().map(|o| norm(&o.path)).collect();
    let managed_ids: HashSet<_> = state
        .owned
        .iter()
        .filter_map(|o| o.file_id.as_deref())
        .collect();
    let manual_paths: HashSet<_> = apps
        .iter()
        .filter(|a| a.policy != Policy::Auto)
        .map(|a| norm(&a.source))
        .collect();
    let mut plan = Plan {
        root: root.to_string_lossy().into_owned(),
        root_id,
        layout,
        items: Vec::new(),
        skipped: Vec::new(),
        options,
        folders: Vec::new(),
        russian: config.language == "ru",
    };
    let paths = paths(&root, cancel)?;
    let dependencies = dependencies(&root, &paths, &apps, cancel)?;
    // Explorer may present an 8.3 alias (RUNNER~1) for the very same file
    // whose canonical desktop path uses the long directory name. Index the
    // current desktop positions once; compare file identity only when the
    // cheap normalized path lookup misses.
    let mut positions = HashMap::<String, Point>::new();
    let mut aliases = HashMap::<String, Vec<(PathBuf, Point)>>::new();
    if let Some(layout) = &plan.layout {
        for item in &layout.items {
            positions.insert(norm(&item.path), item.position.clone());
            if let Some(name) = Path::new(&item.path).file_name() {
                aliases
                    .entry(name.to_string_lossy().to_lowercase())
                    .or_default()
                    .push((PathBuf::from(&item.path), item.position.clone()));
            }
        }
    }
    let mut occupied: HashSet<String> = paths
        .iter()
        .filter_map(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_lowercase())
        .collect();
    for path in paths {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Organization analysis cancelled"
        );
        let display_name = path
            .file_name()
            .context("Desktop entry has no name")?
            .to_string_lossy()
            .into_owned();
        let candidate = (|| -> Result<Item> {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .context("The filename cannot be represented losslessly")?;
            fs::leaf(name)?;
            let shortcut = is_shortcut(&path);
            let stamp = fs::inspect(&path, shortcut)?;
            ensure!(
                !keep.contains(&stamp.id),
                "You chose to keep this object on the desktop"
            );
            let normalized = norm(&path.to_string_lossy());
            ensure!(
                !managed_paths.contains(&normalized) && !managed_ids.contains(stamp.id.as_str()),
                "Managed quick shortcut: left in place"
            );
            ensure!(
                !manual_paths.contains(&normalized),
                "Manual shortcut rule: left in place"
            );
            dependencies.check(&path, shortcut)?;
            let (group, launch) = classify(&path, &plan.options)?;
            let position = positions.get(&normalized).cloned().or_else(|| {
                aliases.get(&name.to_lowercase()).and_then(|candidates| {
                    candidates
                        .iter()
                        .find(|(alias, _)| fs::matches_id(alias, &stamp.id).unwrap_or(false))
                        .map(|(_, position)| position.clone())
                })
            });
            Ok(Item {
                name: name.to_owned(),
                group,
                selected: true,
                position,
                stamp,
                launch,
            })
        })();
        match candidate {
            Ok(item) => plan.items.push(item),
            Err(error) => plan.skipped.push(Skipped {
                name: display_name,
                reason: format!("{error:#}"),
            }),
        }
    }
    let known: Vec<Folder> = store.get(FOLDERS)?;
    let groups: BTreeSet<_> = plan.items.iter().map(|i| i.group).collect();
    for group in groups {
        let existing = known.iter().find(|folder| {
            folder.group == group
                && fs::leaf(&folder.name).is_ok()
                && fs::directory(&root.join(&folder.name))
                    .and_then(|f| fs::identity(&f))
                    .is_ok_and(|id| folder.id.as_ref() == Some(&id))
        });
        if let Some(folder) = existing {
            let mut folder = folder.clone();
            folder.created = false;
            plan.folders.push(folder);
            continue;
        }
        let label = group.label(plan.russian);
        let name = (0..1000)
            .map(|n| match n {
                0 => label.to_owned(),
                1 => format!("{label} (Desktop Pilot)"),
                _ => format!("{label} (Desktop Pilot {n})"),
            })
            .find(|name| !occupied.contains(&name.to_lowercase()))
            .context("No safe category folder name is available")?;
        occupied.insert(name.to_lowercase());
        plan.folders.push(Folder {
            group,
            name,
            id: None,
            created: true,
            position: None,
        });
    }
    Ok(plan)
}

impl Plan {
    pub(super) fn movements(&self) -> Vec<Movement> {
        let mut counts = BTreeMap::<Group, usize>::new();
        for item in self.items.iter().filter(|i| i.selected) {
            *counts.entry(item.group).or_default() += 1;
        }
        self.items
            .iter()
            .filter(|item| item.selected)
            .filter_map(|item| {
                let folder = self.folders.iter().find(|f| f.group == item.group)?;
                // A new singleton folder would free no icon position.
                (!folder.created || counts[&item.group] >= 2).then(|| Movement {
                    item: item.clone(),
                    folder: folder.name.clone(),
                })
            })
            .collect()
    }

    pub fn summary(&self) -> (usize, usize, usize) {
        let movements = self.movements();
        let created = self
            .folders
            .iter()
            .filter(|f| f.created && movements.iter().any(|m| m.folder == f.name))
            .count();
        (
            movements.len(),
            created,
            movements.len().saturating_sub(created),
        )
    }

    pub fn destination(&self, item: &Item) -> String {
        self.folders
            .iter()
            .find(|f| f.group == item.group)
            .map(|f| f.name.clone())
            .unwrap_or_default()
    }

    pub fn will_move(&self, item: &Item) -> bool {
        item.selected
            && self.folders.iter().any(|folder| {
                folder.group == item.group
                    && (!folder.created
                        || self
                            .items
                            .iter()
                            .filter(|i| i.selected && i.group == item.group)
                            .take(2)
                            .count()
                            == 2)
            })
    }

    /// Projection accounts for the space used by new folders. It is not a
    /// screenshot or a promise: the actual layout is rechecked before applying.
    pub fn projected_zone(&self, config: &Config) -> Option<Config> {
        let mut projected = self.layout.clone()?;
        let movements = self.movements();
        let moved: HashSet<_> = movements
            .iter()
            .map(|m| norm(&Path::new(&self.root).join(&m.item.name).to_string_lossy()))
            .collect();
        let alias_ids: HashMap<_, _> = movements
            .iter()
            .map(|movement| {
                (
                    movement.item.name.to_lowercase(),
                    movement.item.stamp.id.as_str(),
                )
            })
            .collect();
        projected.items.retain(|item| {
            if moved.contains(&norm(&item.path)) {
                return false;
            }
            // A normalized text mismatch must not leave an already moved
            // icon in the projected occupancy grid. Never hide an unrelated
            // public-desktop item with the same name: verify its file ID.
            let Some(name) = Path::new(&item.path).file_name() else {
                return true;
            };
            alias_ids
                .get(&name.to_string_lossy().to_lowercase())
                .is_none_or(|id| !fs::matches_id(Path::new(&item.path), id).unwrap_or(false))
        });
        for folder in self.folders.iter().filter(|f| f.created) {
            if let Some(movement) = movements.iter().find(|m| m.folder == folder.name)
                && let Some(position) = &movement.item.position
            {
                projected.items.push(native::Item {
                    path: Path::new(&self.root)
                        .join(&folder.name)
                        .to_string_lossy()
                        .into_owned(),
                    name: folder.name.clone(),
                    position: position.clone(),
                });
            }
        }
        find_free_zone(&projected, config)
    }
}

pub fn find_free_zone(layout: &native::Layout, config: &Config) -> Option<Config> {
    if layout.flags & 1 != 0 || config.validate().is_err() {
        return None;
    }
    let anchor_config = Config {
        slots: 1,
        zone_col: 0,
        zone_row: 0,
        zone_columns: 1,
        ..config.clone()
    };
    let anchor = crate::engine::zone_candidates(layout, &anchor_config)
        .ok()?
        .into_iter()
        .next()?;
    let sx = i64::from(layout.spacing.x);
    let sy = i64::from(layout.spacing.y);
    let bx = i64::from(anchor.x).rem_euclid(sx);
    let by = i64::from(anchor.y).rem_euclid(sy);
    // At most 40 starting columns/rows plus the seven remaining slots. Build
    // occupancy once instead of probing thousands of paths for every candidate.
    const SIDE: usize = 48;
    let point = |col: usize, row: usize| {
        (
            i64::from(anchor.x) + col as i64 * sx,
            i64::from(anchor.y) + row as i64 * sy,
        )
    };
    let mut free = [[false; SIDE]; SIDE];
    for (row, cells) in free.iter_mut().enumerate() {
        for (col, cell) in cells.iter_mut().enumerate() {
            let (x, y) = point(col, row);
            *cell = x - bx >= i64::from(layout.work[0])
                && y - by >= i64::from(layout.work[1])
                && x - bx + sx <= i64::from(layout.work[2])
                && y - by + sy <= i64::from(layout.work[3])
                && i32::try_from(x).is_ok()
                && i32::try_from(y).is_ok();
        }
    }
    for item in &layout.items {
        let col = (i64::from(item.position.x) - i64::from(anchor.x)).div_euclid(sx);
        let row = (i64::from(item.position.y) - i64::from(anchor.y)).div_euclid(sy);
        for r in (row - 1).max(0)..=(row + 1).min(SIDE as i64 - 1) {
            for c in (col - 1).max(0)..=(col + 1).min(SIDE as i64 - 1) {
                let (x, y) = point(c as usize, r as usize);
                if (i64::from(item.position.x) - x).abs() < sx - 2
                    && (i64::from(item.position.y) - y).abs() < sy - 2
                {
                    free[r as usize][c as usize] = false;
                }
            }
        }
    }
    let fits = |cfg: &Config| {
        (0..cfg.slots).all(|index| {
            let col = cfg.zone_col as usize + index % cfg.zone_columns as usize;
            let row = cfg.zone_row as usize + index / cfg.zone_columns as usize;
            free[row][col]
        })
    };
    let verify = |candidate: Config| {
        crate::engine::reserve_positions(
            layout,
            &candidate,
            &State::default(),
            &[],
            candidate.slots,
        )
        .ok()
        .map(|_| candidate)
    };
    if fits(config) {
        return verify(config.clone());
    }
    let mut shapes = vec![config.zone_columns, 1, 2, config.slots as i32];
    shapes.dedup();
    for columns in shapes {
        for col in 0..=40 {
            for row in 0..=40 {
                let candidate = Config {
                    zone_col: col,
                    zone_row: row,
                    zone_columns: columns,
                    ..config.clone()
                };
                if fits(&candidate) {
                    return verify(candidate);
                }
            }
        }
    }
    None
}
