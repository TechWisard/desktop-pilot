use crate::model::{App, Point, hash, norm};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    mem::size_of,
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{Com::*, Registry::*, SystemInformation::GetTickCount, Threading::*},
        UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{GUID, Interface, PCWSTR, VARIANT, w},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
fn text(b: &[u16]) -> String {
    String::from_utf16_lossy(&b[..b.iter().position(|c| *c == 0).unwrap_or(b.len())])
}
pub struct Com;
impl Com {
    pub fn new() -> Result<Self> {
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
        }
        Ok(Self)
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}
pub fn dpi() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
pub fn desktop() -> Result<PathBuf> {
    known_folder(&FOLDERID_Desktop)
}
pub fn programs() -> Result<PathBuf> {
    known_folder(&FOLDERID_Programs)
}
fn known_folder(id: &GUID) -> Result<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)?;
        let s = p.to_string()?;
        CoTaskMemFree(Some(p.0.cast()));
        Ok(s.into())
    }
}

pub fn read_link(path: &Path) -> Result<App> {
    let source = path.to_string_lossy().into_owned();
    let name = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    let mut a = App {
        source,
        name,
        valid: true,
        ..Default::default()
    };
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("url"))
    {
        let bytes = std::fs::read(path)?;
        let s = if bytes.starts_with(&[255, 254]) {
            String::from_utf16_lossy(
                &bytes[2..]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect::<Vec<_>>(),
            )
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        };
        a.target = s
            .lines()
            .find_map(|l| l.strip_prefix("URL="))
            .unwrap_or("")
            .trim()
            .into();
        a.icon = s
            .lines()
            .find_map(|l| l.strip_prefix("IconFile="))
            .unwrap_or("")
            .trim()
            .into();
        ensure!(!a.target.is_empty(), "Нет URL в ярлыке");
    } else {
        unsafe {
            let l: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let p: IPersistFile = l.cast()?;
            p.Load(PCWSTR(wide(&a.source).as_ptr()), STGM_READ)?;
            let mut b = vec![0u16; 32768];
            l.GetPath(&mut b, null_mut(), SLGP_RAWPATH.0 as u32)?;
            a.target = text(&b);
            b.fill(0);
            l.GetArguments(&mut b)?;
            a.arguments = text(&b);
            b.fill(0);
            l.GetWorkingDirectory(&mut b)?;
            a.cwd = text(&b);
            b.fill(0);
            l.GetIconLocation(&mut b, &mut a.icon_index)?;
            a.icon = text(&b);
        }
    }
    a.assign_id();
    a.valid = path.is_file() && valid_target(&a);
    Ok(a)
}
pub fn valid_target(a: &App) -> bool {
    if a.target.starts_with("steam://")
        || a.target.starts_with("https://")
        || a.target.starts_with("http://")
    {
        return true;
    }
    Path::new(&crate::model::expand_env(&a.target)).exists()
        && (a.cwd.is_empty() || Path::new(&crate::model::expand_env(&a.cwd)).is_dir())
}
pub fn make_link(
    path: &Path,
    exe: &Path,
    args: &str,
    icon: &str,
    index: i32,
    description: &str,
) -> Result<()> {
    unsafe {
        let l: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
        l.SetPath(PCWSTR(wide(&exe.to_string_lossy()).as_ptr()))?;
        l.SetArguments(PCWSTR(wide(args).as_ptr()))?;
        if let Some(p) = exe.parent() {
            l.SetWorkingDirectory(PCWSTR(wide(&p.to_string_lossy()).as_ptr()))?;
        }
        l.SetDescription(PCWSTR(wide(description).as_ptr()))?;
        if !icon.is_empty() {
            l.SetIconLocation(PCWSTR(wide(icon).as_ptr()), index)?;
        }
        l.SetShowCmd(SW_SHOWNORMAL)?;
        let p: IPersistFile = l.cast()?;
        p.Save(PCWSTR(wide(&path.to_string_lossy()).as_ptr()), true)?;
        Ok(())
    }
}
pub fn open_file(path: &Path) -> Result<()> {
    unsafe {
        let p = wide(&path.to_string_lossy());
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: w!("open"),
            lpFile: PCWSTR(p.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        ShellExecuteExW(&mut info).context("Windows не смогла открыть исходный ярлык")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Item {
    pub path: String,
    pub name: String,
    pub position: Point,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Layout {
    pub items: Vec<Item>,
    pub spacing: Point,
    pub origin: Point,
    pub work: [i32; 4],
    pub flags: u32,
    pub topology: String,
    #[serde(default = "default_icon_size")]
    pub icon_size: i32,
}
fn default_icon_size() -> i32 {
    32
}
struct Desktop {
    view: IFolderView,
    view2: IFolderView2,
    window: HWND,
}
impl Desktop {
    fn new() -> Result<Self> {
        unsafe {
            let sw: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_LOCAL_SERVER)?;
            let mut hwnd = 0;
            let d = sw.FindWindowSW(
                &VARIANT::from(0i32),
                &VARIANT::default(),
                SWC_DESKTOP,
                &mut hwnd,
                SWFO_NEEDDISPATCH,
            )?;
            let p: IServiceProvider = d.cast()?;
            let b: IShellBrowser =
                p.QueryService(&GUID::from_u128(0x4c96be40_915c_11cf_99d3_00aa004ae837))?;
            let v = b.QueryActiveShellView()?;
            Ok(Self {
                view: v.cast()?,
                view2: v.cast()?,
                window: v.GetWindow()?,
            })
        }
    }
    fn items(&self) -> Result<Vec<(Item, Vec<u8>)>> {
        unsafe {
            let n = self.view.ItemCount(SVGIO_ALLVIEW)?;
            ensure!(
                (0..=10000).contains(&n),
                "Некорректное число объектов рабочего стола"
            );
            let mut out = Vec::new();
            for i in 0..n {
                let p = self.view.Item(i)?;
                let result = (|| -> Result<_> {
                    let pos = self.view.GetItemPosition(p)?;
                    let name = pidl_name(p, SIGDN_NORMALDISPLAY)?;
                    let path = pidl_name(p, SIGDN_DESKTOPABSOLUTEPARSING)?;
                    let bytes =
                        std::slice::from_raw_parts(p.cast::<u8>(), ILGetSize(Some(p)) as usize)
                            .to_vec();
                    Ok((
                        Item {
                            path,
                            name,
                            position: Point { x: pos.x, y: pos.y },
                        },
                        bytes,
                    ))
                })();
                CoTaskMemFree(Some(p.cast()));
                out.push(result?);
            }
            Ok(out)
        }
    }
}
unsafe fn pidl_name(p: *const Common::ITEMIDLIST, kind: SIGDN) -> Result<String> {
    unsafe {
        let s = SHGetNameFromIDList(p, kind)?;
        let value = s.to_string();
        CoTaskMemFree(Some(s.0.cast()));
        Ok(value?)
    }
}
pub fn layout() -> Result<Layout> {
    unsafe {
        let d = Desktop::new()?;
        let mut spacing = POINT::default();
        d.view.GetSpacing(&mut spacing)?;
        ensure!(spacing.x > 20 && spacing.y > 20, "Недопустимая сетка");
        let h = d.window;
        let mut origin = POINT::default();
        let _ = ClientToScreen(h, &mut origin);
        let mon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        ensure!(
            GetMonitorInfoW(mon, &mut mi).as_bool(),
            "Нет геометрии монитора"
        );
        let work = [
            mi.rcWork.left - origin.x,
            mi.rcWork.top - origin.y,
            mi.rcWork.right - origin.x,
            mi.rcWork.bottom - origin.y,
        ];
        let flags = d.view2.GetCurrentFolderFlags()?;
        let mut mode = FOLDERVIEWMODE::default();
        let mut icon_size = 32;
        d.view2.GetViewModeAndIconSize(&mut mode, &mut icon_size)?;
        let topology = hash(
            format!(
                "{work:?}/{spacing:?}/{origin:?}/{}/{}/{}/{}",
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN),
                GetSystemMetrics(SM_CYVIRTUALSCREEN)
            )
            .as_bytes(),
        );
        Ok(Layout {
            items: d.items()?.into_iter().map(|i| i.0).collect(),
            spacing: Point {
                x: spacing.x,
                y: spacing.y,
            },
            origin: Point {
                x: origin.x,
                y: origin.y,
            },
            work,
            flags,
            topology,
            icon_size,
        })
    }
}
pub fn position_many(positions: &[(String, Point)]) -> Result<()> {
    if positions.is_empty() {
        return Ok(());
    }
    let d = Desktop::new()?;
    ensure!(
        unsafe { d.view2.GetCurrentFolderFlags()? } & 1 == 0,
        "В Проводнике включено автоматическое упорядочивание: позиции не меняются"
    );
    let entries = d.items()?;
    let mut pointers = Vec::new();
    let mut points = Vec::new();
    for (path, point) in positions {
        let matching: Vec<_> = entries
            .iter()
            .filter(|(i, _)| norm(&i.path) == norm(path))
            .collect();
        ensure!(
            matching.len() == 1,
            "Объект ещё не появился в Проводнике или путь неоднозначен: {path}"
        );
        pointers.push(matching[0].1.as_ptr().cast());
        points.push(POINT {
            x: point.x,
            y: point.y,
        });
    }
    unsafe {
        d.view.SelectAndPositionItems(
            pointers.len() as u32,
            pointers.as_ptr(),
            Some(points.as_ptr()),
            SVSI_POSITIONITEM.0 as u32,
        )?;
    }
    let actual = layout()?;
    for (path, point) in positions {
        let item = actual
            .items
            .iter()
            .find(|i| norm(&i.path) == norm(path))
            .context("Потерян объект после позиционирования")?;
        ensure!(
            (item.position.x - point.x).abs() <= 1 && (item.position.y - point.y).abs() <= 1,
            "Проводник не подтвердил координаты: {path}"
        );
    }
    Ok(())
}
pub fn refresh(path: &Path) {
    unsafe {
        let p = wide(&path.to_string_lossy());
        SHChangeNotify(
            SHCNE_UPDATEDIR,
            SHCNF_PATHW | SHCNF_FLUSH,
            Some(p.as_ptr().cast()),
            None,
        );
    }
}

pub fn favorites() -> Result<Vec<String>> {
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Taskband");
        let mut size = 0;
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
        .ok()?;
        ensure!(size < 2_000_000, "Слишком большой Taskband");
        let mut b = vec![0u8; size as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            Some(b.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()?;
        let mut off = 0;
        let mut names = Vec::new();
        while off < b.len() && b[off] == 0 {
            ensure!(off + 5 <= b.len(), "Повреждён Taskband");
            let n = u32::from_le_bytes(b[off + 1..off + 5].try_into()?) as usize;
            ensure!(
                (2..=65535).contains(&n) && off + 5 + n <= b.len(),
                "Неподдерживаемый Taskband"
            );
            let payload = &b[off + 5..off + 5 + n];
            validate_pidl(payload)?;
            names.push(pidl_name(
                payload.as_ptr().cast(),
                SIGDN_DESKTOPABSOLUTEPARSING,
            )?);
            off += 5 + n;
        }
        ensure!(
            off + 1 == b.len() && b[off] == 255,
            "Неизвестная версия Taskband; автоматические изменения приостановлены"
        );
        Ok(names)
    }
}
pub fn validate_pidl(b: &[u8]) -> Result<()> {
    let mut p = 0;
    loop {
        ensure!(p + 2 <= b.len(), "Оборван PIDL");
        let n = u16::from_le_bytes([b[p], b[p + 1]]) as usize;
        if n == 0 {
            ensure!(p + 2 == b.len(), "Лишние данные PIDL");
            return Ok(());
        }
        ensure!(n >= 2 && p + n <= b.len(), "Недопустимый PIDL");
        p += n;
    }
}
pub fn pin_identities() -> Result<Vec<String>> {
    let mut ids = Vec::new();
    for p in favorites()? {
        if p.to_lowercase().ends_with(".lnk") {
            if let Ok(a) = read_link(Path::new(&p)) {
                ids.push(format!(
                    "{}|{}",
                    norm(&a.target),
                    a.arguments.trim().to_lowercase()
                ));
            }
        } else {
            ids.push(p.to_lowercase());
        }
    }
    Ok(ids)
}
pub fn is_pinned(a: &App, pins: &[String]) -> bool {
    let k = format!("{}|{}", norm(&a.target), a.arguments.trim().to_lowercase());
    pins.iter().any(|p| {
        *p == k
            || (!a.arguments.is_empty()
                && p.ends_with(
                    a.arguments
                        .trim_start_matches("shell:AppsFolder\\")
                        .to_lowercase()
                        .as_str(),
                ))
    })
}

pub fn steam_locations() -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();
    let mut buffer = vec![0u16; 32768];
    let mut size = (buffer.len() * 2) as u32;
    let code = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Valve\\Steam"),
            w!("SteamPath"),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if code != ERROR_SUCCESS {
        return result;
    }
    let root = std::path::PathBuf::from(text(&buffer));
    let mut libraries = vec![root.clone()];
    fn field(line: &str, key: &str) -> Option<String> {
        let parts: Vec<_> = line.split('"').collect();
        if parts.len() >= 4 && parts[1] == key {
            Some(parts[3].replace("\\\\", "\\"))
        } else {
            None
        }
    }
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        libraries.extend(
            vdf.lines()
                .filter_map(|l| field(l, "path"))
                .map(std::path::PathBuf::from),
        );
    }
    for library in libraries {
        let apps = library.join("steamapps");
        if let Ok(entries) = std::fs::read_dir(&apps) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                if !name.starts_with("appmanifest_") || !name.ends_with(".acf") {
                    continue;
                }
                if let Ok(manifest) = std::fs::read_to_string(entry.path()) {
                    let id = manifest.lines().find_map(|l| field(l, "appid"));
                    let directory = manifest.lines().find_map(|l| field(l, "installdir"));
                    if let (Some(id), Some(directory)) = (id, directory) {
                        let folder = apps.join("common").join(&directory);
                        if !directory.contains(['/', '\\', ':']) && folder.is_dir() {
                            result.insert(id, folder.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
    }
    result
}

pub struct Foreground {
    pub window: isize,
    pub process: String,
    pub app_id: String,
    pub idle: u32,
}
pub fn foreground() -> Result<Foreground> {
    unsafe {
        let mut input = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            ..Default::default()
        };
        ensure!(
            GetLastInputInfo(&mut input).as_bool(),
            "Нет данных активности"
        );
        let idle = GetTickCount().wrapping_sub(input.dwTime) / 1000;
        let w = GetForegroundWindow();
        ensure!(!w.is_invalid(), "Нет активного окна");
        let mut pid = 0;
        GetWindowThreadProcessId(w, Some(&mut pid));
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut b = vec![0u16; 32768];
        let mut n = b.len() as u32;
        let r = QueryFullProcessImageNameW(
            h,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(b.as_mut_ptr()),
            &mut n,
        );
        let mut app_buffer = vec![0u16; 512];
        let mut app_size = app_buffer.len() as u32;
        let app_result = windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId(
            h,
            &mut app_size,
            windows::core::PWSTR(app_buffer.as_mut_ptr()),
        );
        let app_id = if app_result == ERROR_SUCCESS {
            text(&app_buffer)
        } else {
            String::new()
        };
        let _ = CloseHandle(h);
        r?;
        Ok(Foreground {
            window: w.0 as isize,
            process: text(&b),
            app_id,
            idle,
        })
    }
}
pub fn quiet_for_changes() -> bool {
    unsafe {
        if GetAsyncKeyState(VK_LBUTTON.0 as i32) < 0 || GetAsyncKeyState(VK_RBUTTON.0 as i32) < 0 {
            return false;
        }
        let h = GetForegroundWindow();
        if h.is_invalid() {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(m, &mut mi).as_bool() {
            return false;
        }
        let full = r.left <= mi.rcMonitor.left
            && r.top <= mi.rcMonitor.top
            && r.right >= mi.rcMonitor.right
            && r.bottom >= mi.rcMonitor.bottom;
        !full && foreground().is_ok_and(|f| f.idle >= 15)
    }
}
pub fn autostart(exe: &Path, home: &Path, enable: bool) -> Result<()> {
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let name = w!("DesktopPilot");
        if enable {
            let command = wide(&format!(
                "\"{}\" --background --home \"{}\"",
                exe.display(),
                home.display()
            ));
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key,
                name,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            )
            .ok()?;
        } else {
            let code = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, name);
            if code != ERROR_FILE_NOT_FOUND {
                code.ok()?;
            }
        }
        Ok(())
    }
}
pub struct Lock(HANDLE);
impl Lock {
    pub fn take(name: &str) -> Result<Self> {
        unsafe {
            let h = CreateMutexW(None, true, PCWSTR(wide(name).as_ptr()))?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(h);
                bail!("Операция уже выполняется другим экземпляром");
            }
            Ok(Self(h))
        }
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}
pub fn exclusive(name: &str, home: &Path) -> Result<Lock> {
    Lock::take(&format!(
        "Local\\DesktopPilot-{name}-{}",
        &hash(norm(&home.to_string_lossy()).as_bytes())[..12]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_pidls_rejected() {
        for b in [&[][..], &[0], &[1, 0], &[4, 0, 0, 0], &[0, 0, 4]] {
            assert!(validate_pidl(b).is_err());
        }
        assert!(validate_pidl(&[2, 0, 0, 0]).is_ok());
    }
}
