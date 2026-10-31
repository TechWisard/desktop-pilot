use crate::model::{App, Point, hash, norm};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    mem::size_of,
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows::{
    Win32::{
        Foundation::*,
        Globalization::{CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar},
        Graphics::Gdi::*,
        Storage::FileSystem::{
            GetFileVersionInfoSizeW, GetFileVersionInfoW, VS_FIXEDFILEINFO, VerQueryValueW,
        },
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
pub fn common_desktop() -> Result<PathBuf> {
    known_folder(&FOLDERID_PublicDesktop)
}
pub fn programs() -> Result<PathBuf> {
    known_folder(&FOLDERID_Programs)
}
pub fn common_programs() -> Result<PathBuf> {
    known_folder(&FOLDERID_CommonPrograms)
}
fn known_folder(id: &GUID) -> Result<PathBuf> {
    unsafe {
        let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None)?;
        let s = p.to_string();
        CoTaskMemFree(Some(p.0.cast()));
        Ok(s?.into())
    }
}

fn shortcut_text(bytes: &[u8]) -> Result<String> {
    ensure!(bytes.len() <= 1024 * 1024, "Internet shortcut is too large");
    if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        ensure!(
            bytes.len().is_multiple_of(2),
            "Incomplete UTF-16 internet shortcut"
        );
        let little = bytes[0] == 0xff;
        let chars: Vec<_> = bytes[2..]
            .chunks_exact(2)
            .map(|pair| {
                if little {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        return String::from_utf16(&chars).context("Invalid UTF-16 internet shortcut");
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Ok(value) = std::str::from_utf8(bytes) {
        return Ok(value.into());
    }
    // Older Windows launchers write .url files using the user's ANSI code page.
    // Lossy UTF-8 decoding silently changed Cyrillic icon and file names.
    unsafe {
        let count = MultiByteToWideChar(CP_ACP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        ensure!(count > 0, "Cannot decode internet shortcut");
        let mut chars = vec![0u16; count as usize];
        ensure!(
            MultiByteToWideChar(
                CP_ACP,
                MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
                bytes,
                Some(&mut chars)
            ) == count,
            "Cannot decode internet shortcut"
        );
        String::from_utf16(&chars).context("Invalid internet shortcut encoding")
    }
}

fn internet_shortcut_fields(value: &str) -> Result<(String, String, i32)> {
    ensure!(
        !value.contains('\0'),
        "Internet shortcut contains an embedded NUL"
    );
    let mut section = false;
    let (mut target, mut icon, mut icon_index) = (None, String::new(), 0);
    for line in value.lines().map(str::trim) {
        if line.starts_with([';', '#']) || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1]
                .trim()
                .eq_ignore_ascii_case("InternetShortcut");
            continue;
        }
        if !section {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .unwrap_or(value);
            if key.trim().eq_ignore_ascii_case("URL") && target.is_none() {
                target = Some(value.to_owned());
            } else if key.trim().eq_ignore_ascii_case("IconFile") && icon.is_empty() {
                icon = value.to_owned();
            } else if key.trim().eq_ignore_ascii_case("IconIndex") {
                icon_index = value.parse().unwrap_or(0);
            }
        }
    }
    let target = target
        .filter(|value| !value.is_empty())
        .context("Нет URL в ярлыке")?;
    Ok((target, icon, icon_index))
}

pub fn read_link(path: &Path) -> Result<App> {
    ensure!(
        path.is_file(),
        "Shortcut file does not exist: {}",
        path.display()
    );
    ensure!(
        path.extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk") || ext.eq_ignore_ascii_case("url")),
        "Only .lnk and .url shortcuts are supported"
    );
    let absolute = crate::profile::ordinary_path(std::fs::canonicalize(path)?);
    let path = absolute.as_path();
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
        ensure!(
            std::fs::metadata(path)?.len() <= 1024 * 1024,
            "Internet shortcut is too large"
        );
        let mut bytes = Vec::new();
        // Bound the actual read too: the file may grow after the metadata check.
        std::fs::File::open(path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        (a.target, a.icon, a.icon_index) = internet_shortcut_fields(&shortcut_text(&bytes)?)?;
    } else {
        unsafe {
            let l: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)?;
            let p: IPersistFile = l.cast()?;
            p.Load(PCWSTR(wide(&a.source).as_ptr()), STGM_READ)?;
            let mut b = vec![0u16; 32768];
            l.GetPath(&mut b, null_mut(), SLGP_RAWPATH.0 as u32)?;
            a.target = text(&b);
            if a.target.is_empty()
                && let Ok(pidl) = l.GetIDList()
                && !pidl.is_null()
            {
                let parsed = pidl_name(pidl, SIGDN_DESKTOPABSOLUTEPARSING);
                CoTaskMemFree(Some(pidl.cast()));
                a.target = parsed?;
            }
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
// A web shortcut without an authority cannot open a real destination, even
// when its URL still begins with a familiar protocol prefix. Inspect only the
// authority: path and query components may legitimately contain spaces.
fn web_target_has_host(target: &str) -> bool {
    let Some((_, remainder)) = target.split_once("://") else {
        return false;
    };
    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    if host_port.is_empty()
        || host_port.starts_with(':')
        || host_port
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control() || ch == '\\')
    {
        return false;
    }
    // A bracketed authority must contain a valid IPv6 address, with an
    // optional port after the closing bracket. The address's colons are not
    // port separators.
    let port = if let Some(bracketed) = host_port.strip_prefix('[') {
        let Some((address, tail)) = bracketed.split_once(']') else {
            return false;
        };
        if address.parse::<std::net::Ipv6Addr>().is_err() {
            return false;
        }
        if tail.is_empty() {
            None
        } else {
            let Some(port) = tail.strip_prefix(':') else {
                return false;
            };
            Some(port)
        }
    } else {
        if host_port.contains(['[', ']']) {
            return false;
        }
        host_port.split_once(':').map(|(_, port)| port)
    };
    // An empty explicit port is treated by browsers as omitted. A nondecimal
    // or out-of-range port cannot be launched as an HTTP(S) authority.
    port.is_none_or(|port| {
        port.is_empty()
            || (port.bytes().all(|digit| digit.is_ascii_digit()) && port.parse::<u16>().is_ok())
    })
}
pub fn valid_target(a: &App) -> bool {
    let target = a.target.trim();
    if target.is_empty() || target.contains('\0') {
        return false;
    }
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        return web_target_has_host(target);
    }
    if lower.starts_with("steam://") || lower.starts_with("shell:") || lower.starts_with("::{") {
        return true;
    }
    // Registered launchers (Epic, mailto, etc.) are valid shell targets too.
    if let Some((scheme, _)) = target.split_once(':')
        && scheme.len() > 1
        && scheme.bytes().enumerate().all(|(i, c)| {
            c.is_ascii_alphabetic() || (i > 0 && (c.is_ascii_digit() || b"+-.".contains(&c)))
        })
    {
        let key = wide(scheme);
        return unsafe {
            RegGetValueW(
                HKEY_CLASSES_ROOT,
                PCWSTR(key.as_ptr()),
                w!("URL Protocol"),
                RRF_RT_REG_SZ,
                None,
                None,
                None,
            )
        } == ERROR_SUCCESS;
    }
    Path::new(&crate::model::expand_env(target)).exists()
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

pub fn show_in_folder(path: &Path) -> Result<()> {
    let path = crate::profile::ordinary_path(std::fs::canonicalize(path)?);
    let path = wide(&path.to_string_lossy());
    unsafe {
        let mut pidl = null_mut();
        SHParseDisplayName(PCWSTR(path.as_ptr()), None, &mut pidl, 0, None)?;
        ensure!(
            !pidl.is_null(),
            "Cannot locate the shortcut in Windows Explorer"
        );
        // With an empty selection array the absolute PIDL identifies the item,
        // so Explorer opens its parent and selects it without launching it.
        let result = SHOpenFolderAndSelectItems(pidl, None, 0);
        CoTaskMemFree(Some(pidl.cast()));
        result.context("Cannot show the shortcut in Windows Explorer")
    }
}

pub fn executable_version(path: &Path) -> Result<Option<[u16; 4]>> {
    let path = wide(&path.to_string_lossy());
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let size = GetFileVersionInfoSizeW(PCWSTR(path.as_ptr()), None);
        if size == 0 {
            let error = GetLastError();
            if matches!(
                error,
                ERROR_SUCCESS
                    | ERROR_RESOURCE_DATA_NOT_FOUND
                    | ERROR_RESOURCE_TYPE_NOT_FOUND
                    | ERROR_RESOURCE_NAME_NOT_FOUND
                    | ERROR_RESOURCE_LANG_NOT_FOUND
                    | ERROR_BAD_EXE_FORMAT
            ) {
                return Ok(None);
            }
            return Err(windows::core::Error::from_win32().into());
        }
        ensure!(
            size <= 16 * 1024 * 1024,
            "Executable version resource is too large"
        );
        let mut bytes = vec![0u8; size as usize];
        GetFileVersionInfoW(PCWSTR(path.as_ptr()), 0, size, bytes.as_mut_ptr().cast())?;
        let mut pointer = null_mut();
        let mut length = 0;
        ensure!(
            VerQueryValueW(bytes.as_ptr().cast(), w!("\\"), &mut pointer, &mut length).as_bool()
                && !pointer.is_null()
                && length as usize >= size_of::<VS_FIXEDFILEINFO>(),
            "Executable version resource is invalid"
        );
        let start = pointer as usize;
        let buffer = bytes.as_ptr() as usize;
        ensure!(
            start >= buffer
                && start.saturating_add(size_of::<VS_FIXEDFILEINFO>()) <= buffer + bytes.len(),
            "Executable version resource is outside its buffer"
        );
        let version = pointer.cast::<VS_FIXEDFILEINFO>().read_unaligned();
        ensure!(
            version.dwSignature == 0xfeef04bd,
            "Executable version signature is invalid"
        );
        Ok(Some([
            (version.dwProductVersionMS >> 16) as u16,
            version.dwProductVersionMS as u16,
            (version.dwProductVersionLS >> 16) as u16,
            version.dwProductVersionLS as u16,
        ]))
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
            let folder: IShellFolder = self.view.GetFolder()?;
            let mut out = Vec::new();
            for i in 0..n {
                let p = self.view.Item(i)?;
                let result = (|| -> Result<_> {
                    ensure!(!p.is_null(), "Explorer returned an empty item identifier");
                    let pos = self.view.GetItemPosition(p)?;
                    // The view can reorder between COM calls. Resolve names
                    // from the exact child PIDL used for its position, never
                    // from a second lookup of the now potentially stale index.
                    let item = child_item(&folder, p, pos)?;
                    let bytes =
                        std::slice::from_raw_parts(p.cast::<u8>(), ILGetSize(Some(p)) as usize)
                            .to_vec();
                    Ok((item, bytes))
                })();
                CoTaskMemFree(Some(p.cast()));
                out.push(result?);
            }
            Ok(out)
        }
    }
}
unsafe fn child_item(
    folder: &IShellFolder,
    pidl: *const Common::ITEMIDLIST,
    position: POINT,
) -> Result<Item> {
    unsafe {
        ensure!(
            !pidl.is_null(),
            "Explorer returned an empty item identifier"
        );
        Ok(Item {
            path: child_name(folder, pidl, SHGDN_FORPARSING)?,
            name: child_name(folder, pidl, SHGDN_NORMAL)?,
            position: Point {
                x: position.x,
                y: position.y,
            },
        })
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
unsafe fn child_name(
    folder: &IShellFolder,
    pidl: *const Common::ITEMIDLIST,
    kind: SHGDNF,
) -> Result<String> {
    unsafe {
        let mut returned = Common::STRRET::default();
        folder.GetDisplayNameOf(pidl, kind, &mut returned)?;
        let mut name = windows::core::PWSTR::null();
        // StrRetToStr consumes STRRET's storage; only its returned string needs
        // freeing. FORPARSING without INFOLDER gives the desktop-absolute name.
        StrRetToStrW(&mut returned, Some(pidl), &mut name)?;
        let value = name.to_string();
        CoTaskMemFree(Some(name.0.cast()));
        Ok(value?)
    }
}

fn monitor_topology() -> Result<Vec<[i32; 10]>> {
    unsafe extern "system" fn visit(monitor: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> BOOL {
        unsafe {
            let mut info = MONITORINFO {
                cbSize: size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return BOOL(0);
            }
            let values = &mut *(data.0 as *mut Vec<[i32; 10]>);
            let Ok(scale) = GetScaleFactorForMonitor(monitor) else {
                return BOOL(0);
            };
            values.push([
                info.rcMonitor.left,
                info.rcMonitor.top,
                info.rcMonitor.right,
                info.rcMonitor.bottom,
                info.rcWork.left,
                info.rcWork.top,
                info.rcWork.right,
                info.rcWork.bottom,
                info.dwFlags as i32,
                scale.0,
            ]);
            BOOL(1)
        }
    }
    let mut values: Vec<[i32; 10]> = Vec::new();
    ensure!(
        unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(visit),
                LPARAM((&mut values as *mut Vec<[i32; 10]>) as isize),
            )
        }
        .as_bool(),
        "Cannot read monitor topology"
    );
    ensure!(!values.is_empty(), "No desktop monitor is available");
    values.sort_unstable();
    Ok(values)
}
pub fn layout() -> Result<Layout> {
    unsafe {
        let d = Desktop::new()?;
        let mut spacing = POINT::default();
        d.view.GetSpacing(&mut spacing)?;
        ensure!(spacing.x > 20 && spacing.y > 20, "Недопустимая сетка");
        let h = d.window;
        let mut origin = POINT::default();
        ensure!(
            ClientToScreen(h, &mut origin).as_bool(),
            "Cannot read desktop screen coordinates"
        );
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
                "{work:?}/{spacing:?}/{origin:?}/{icon_size}/{:?}/{}/{}/{}/{}",
                monitor_topology()?,
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
            (i64::from(item.position.x) - i64::from(point.x)).abs() <= 1
                && (i64::from(item.position.y) - i64::from(point.y)).abs() <= 1,
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
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            w!("Favorites"),
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        );
        if matches!(result, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(Vec::new());
        }
        result.ok()?;
        ensure!(size < 2_000_000, "Слишком большой Taskband");
        if size == 0 {
            return Ok(Vec::new());
        }
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
        b.truncate(size as usize);
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
            let a = read_link(Path::new(&p))
                .with_context(|| format!("Cannot read pinned shortcut: {p}"))?;
            ids.push(format!(
                "{}|{}",
                crate::model::target_identity(&a.target),
                a.arguments.trim()
            ));
        } else {
            ids.push(p.to_lowercase());
        }
    }
    Ok(ids)
}
pub fn is_pinned(a: &App, pins: &[String]) -> bool {
    let k = format!(
        "{}|{}",
        crate::model::target_identity(&a.target),
        a.arguments.trim()
    );
    let args = a.arguments.trim().trim_matches('"').to_lowercase();
    let app_id = args
        .strip_prefix("shell:appsfolder\\")
        .filter(|id| !id.is_empty() && !id.contains(['\\', '/', '"']));
    pins.iter().any(|p| {
        *p == k
            || (a.arguments.trim().is_empty()
                && crate::model::target_identity(p) == crate::model::target_identity(&a.target))
            || app_id.is_some_and(|id| {
                p.eq_ignore_ascii_case(id) || p.to_lowercase().ends_with(&format!("\\{id}"))
            })
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
                        if valid_steam_directory(&id, &directory) && folder.is_dir() {
                            result.insert(id, folder.to_string_lossy().into_owned());
                        }
                    }
                }
            }
        }
    }
    result
}

fn valid_steam_directory(id: &str, directory: &str) -> bool {
    !id.is_empty()
        && id.bytes().all(|c| c.is_ascii_digit())
        && !directory.is_empty()
        && !directory.ends_with(['.', ' '])
        && !directory.contains(['/', '\\', ':', '<', '>', '"', '|', '?', '*', '\0'])
}

pub struct Foreground {
    pub window: isize,
    pub process: String,
    pub app_id: String,
    pub idle: u32,
}

fn idle_seconds() -> Result<u32> {
    unsafe {
        let mut input = LASTINPUTINFO {
            cbSize: size_of::<LASTINPUTINFO>() as u32,
            ..Default::default()
        };
        ensure!(
            GetLastInputInfo(&mut input).as_bool(),
            "Нет данных активности"
        );
        Ok(GetTickCount().wrapping_sub(input.dwTime) / 1000)
    }
}

pub fn foreground() -> Result<Foreground> {
    unsafe {
        let idle = idle_seconds()?;
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
        // Focus can change while Windows resolves a process. Never attribute a
        // new foreground window's input to the previous foreground process.
        ensure!(
            GetForegroundWindow() == w,
            "Foreground window changed while sampling"
        );
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
        if [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON, VK_XBUTTON1, VK_XBUTTON2]
            .iter()
            .any(|key| GetAsyncKeyState(key.0 as i32) < 0)
        {
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
        let mut class = [0u16; 256];
        let n = GetClassNameW(h, &mut class);
        let is_desktop = n > 0 && matches!(text(&class).as_str(), "Progman" | "WorkerW");
        let full = !is_desktop
            && r.left <= mi.rcMonitor.left
            && r.top <= mi.rcMonitor.top
            && r.right >= mi.rcMonitor.right
            && r.bottom >= mi.rcMonitor.bottom;
        !full && idle_seconds().is_ok_and(|idle| idle >= 15)
    }
}

pub fn quote_arg(value: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for c in value.chars() {
        if c == '\\' {
            slashes += 1;
            continue;
        }
        if c == '"' {
            quoted.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
        } else {
            quoted.extend(std::iter::repeat_n('\\', slashes));
        }
        quoted.push(c);
        slashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    quoted
}

fn command_line_args(command: &str) -> Result<Vec<String>> {
    ensure!(
        !command.is_empty() && !command.contains('\0'),
        "Invalid startup command"
    );
    unsafe {
        let mut count = 0;
        let chars = wide(command);
        let pointers = CommandLineToArgvW(PCWSTR(chars.as_ptr()), &mut count);
        ensure!(!pointers.is_null(), "Cannot read startup command");
        let result = (|| -> Result<Vec<String>> {
            ensure!(
                (1..=32768).contains(&count),
                "Invalid startup argument count"
            );
            std::slice::from_raw_parts(pointers, count as usize)
                .iter()
                .map(|value| Ok(value.to_string()?))
                .collect()
        })();
        let _ = LocalFree(HLOCAL(pointers.cast()));
        result
    }
}

fn command_belongs_to_home(command: &str, home: &Path) -> bool {
    let Ok(args) = command_line_args(command) else {
        return false;
    };
    let target = norm(&home.to_string_lossy());
    if let Some(index) = args.iter().position(|value| value == "--home") {
        return args
            .get(index + 1)
            .is_some_and(|value| norm(value) == target);
    }
    args.first()
        .and_then(|value| Path::new(value).parent())
        .is_some_and(|parent| norm(&parent.to_string_lossy()) == target)
}

fn startup_value(name: &str) -> Result<Option<String>> {
    unsafe {
        let name = wide(name);
        let mut size = 0;
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let result = RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        );
        if matches!(
            result,
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_UNSUPPORTED_TYPE
        ) {
            return Ok(None);
        }
        result.ok()?;
        ensure!(
            size <= 65536 && size % 2 == 0,
            "Invalid startup registry value"
        );
        let mut buffer = vec![0u16; (size / 2).max(1) as usize];
        RegGetValueW(
            HKEY_CURRENT_USER,
            key,
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
        .ok()?;
        Ok(Some(text(&buffer)))
    }
}

pub fn autostart(exe: &Path, home: &Path, enable: bool) -> Result<()> {
    if home.file_name().is_some_and(|name| {
        name.to_string_lossy()
            .to_ascii_lowercase()
            .starts_with("test-")
    }) {
        return Ok(());
    }
    unsafe {
        let key = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let profile = format!(
            "DesktopPilot-{}",
            &hash(norm(&home.to_string_lossy()).as_bytes())[..12]
        );
        let name = wide(&profile);
        let legacy = startup_value("DesktopPilot")?
            .is_some_and(|value| command_belongs_to_home(&value, home));
        if enable {
            ensure!(
                exe.is_file(),
                "The installed executable is missing; reinstall Desktop Pilot before enabling startup"
            );
            let command = format!(
                "{} --background --home {}",
                quote_arg(&exe.to_string_lossy()),
                quote_arg(&home.to_string_lossy())
            );
            ensure!(
                !command.contains('\0') && command.encode_utf16().count() <= 260,
                "The installation path is too long for Windows startup; choose a shorter profile path"
            );
            if startup_value(&profile)?.as_deref() != Some(&command) {
                let command = wide(&command);
                let mut handle = HKEY::default();
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    key,
                    0,
                    None,
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    None,
                    &mut handle,
                    None,
                )
                .ok()?;
                let result = RegSetKeyValueW(
                    handle,
                    None,
                    PCWSTR(name.as_ptr()),
                    REG_SZ.0,
                    Some(command.as_ptr().cast()),
                    (command.len() * 2) as u32,
                );
                let _ = RegCloseKey(handle);
                result.ok()?;
            }
        } else {
            let code = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, PCWSTR(name.as_ptr()));
            if !matches!(code, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
                code.ok()?;
            }
        }
        // Remove the old shared value only when its arguments identify this
        // profile. Disabling a test/custom profile must not disable another.
        if legacy {
            let code = RegDeleteKeyValueW(HKEY_CURRENT_USER, key, w!("DesktopPilot"));
            if !matches!(code, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
                code.ok()?;
            }
        }
        Ok(())
    }
}
pub struct Lock(HANDLE);
impl Lock {
    fn try_take(name: &str) -> Result<Option<Self>> {
        unsafe {
            let h = CreateMutexW(None, true, PCWSTR(wide(name).as_ptr()))?;
            if GetLastError() == ERROR_ALREADY_EXISTS {
                let _ = CloseHandle(h);
                return Ok(None);
            }
            Ok(Some(Self(h)))
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
    try_exclusive(name, home)?.context("Операция уже выполняется другим экземпляром")
}

fn lock_name(name: &str, home: &Path) -> String {
    format!(
        "Local\\DesktopPilot-{name}-{}",
        &hash(norm(&home.to_string_lossy()).as_bytes())[..12]
    )
}

pub fn try_exclusive(name: &str, home: &Path) -> Result<Option<Lock>> {
    Lock::try_take(&lock_name(name, home))
}

pub fn is_running(name: &str, home: &Path) -> Result<bool> {
    let name = wide(&lock_name(name, home));
    unsafe {
        match OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, PCWSTR(name.as_ptr())) {
            Ok(handle) => {
                let _ = CloseHandle(handle);
                Ok(true)
            }
            Err(error)
                if error.code() == windows::core::HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0) =>
            {
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }
}

pub fn activate_process_window(pid: u32) -> Result<bool> {
    if pid == 0 {
        return Ok(false);
    }
    struct Search {
        pid: u32,
        window: HWND,
    }
    unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> BOOL {
        unsafe {
            let search = &mut *(data.0 as *mut Search);
            let mut pid = 0;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if search.window.is_invalid()
                && pid == search.pid
                && IsWindowVisible(window).as_bool()
                && GetWindow(window, GW_OWNER).is_err()
            {
                search.window = window;
            }
            BOOL(1)
        }
    }
    let mut search = Search {
        pid,
        window: HWND::default(),
    };
    unsafe {
        EnumWindows(Some(visit), LPARAM((&mut search as *mut Search) as isize))?;
        if search.window.is_invalid() {
            return Ok(false);
        }
        if IsIconic(search.window).as_bool() {
            let _ = ShowWindowAsync(search.window, SW_RESTORE);
        }
        if !SetForegroundWindow(search.window).as_bool() {
            let _ = FlashWindowEx(&FLASHWINFO {
                cbSize: size_of::<FLASHWINFO>() as u32,
                hwnd: search.window,
                dwFlags: FLASHW_TRAY | FLASHW_TIMERNOFG,
                uCount: 3,
                dwTimeout: 0,
            });
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn desktop_namespace_children_keep_their_absolute_parsing_identity() {
        let _com = Com::new().unwrap();
        let expected = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
        let item = unsafe {
            let mut absolute = null_mut();
            SHParseDisplayName(
                PCWSTR(wide(expected).as_ptr()),
                None,
                &mut absolute,
                0,
                None,
            )
            .unwrap();
            let mut child = null_mut();
            let folder: IShellFolder = SHBindToParent(absolute, Some(&mut child)).unwrap();
            let item = child_item(&folder, child, POINT { x: -100, y: 42 });
            CoTaskMemFree(Some(absolute.cast()));
            item.unwrap()
        };
        assert_eq!(norm(&item.path), norm(expected));
        assert!(!item.name.is_empty());
        assert_eq!(item.position.x, -100);
        assert_eq!(item.position.y, 42);
    }

    #[test]
    fn child_identifiers_keep_their_parent_and_name_when_folder_contents_change() {
        let _com = Com::new().unwrap();
        let root = std::env::temp_dir().join(format!(
            "test-shell-child-{}-{}",
            std::process::id(),
            crate::model::now()
        ));
        std::fs::create_dir_all(&root).unwrap();
        // TEMP can contain an 8.3 alias; Shell parsing names use the full path.
        let root = crate::profile::ordinary_path(std::fs::canonicalize(root).unwrap());
        for (index, parent) in ["Первый", "Второй"].into_iter().enumerate() {
            let directory = root.join(parent);
            std::fs::create_dir_all(&directory).unwrap();
            let path = directory.join("Общий файл.txt");
            std::fs::write(&path, b"original file").unwrap();
            let resolved = unsafe {
                let mut absolute = null_mut();
                SHParseDisplayName(
                    PCWSTR(wide(&path.to_string_lossy()).as_ptr()),
                    None,
                    &mut absolute,
                    0,
                    None,
                )
                .unwrap();
                let mut child = null_mut();
                let folder: IShellFolder = SHBindToParent(absolute, Some(&mut child)).unwrap();
                // A newly inserted item can move the original to another view
                // index; its previously acquired child PIDL must still identify it.
                std::fs::write(directory.join("000-new-file.txt"), b"new item").unwrap();
                let item = child_item(
                    &folder,
                    child,
                    POINT {
                        x: index as i32,
                        y: 42,
                    },
                );
                CoTaskMemFree(Some(absolute.cast()));
                item.unwrap()
            };
            assert_eq!(norm(&resolved.path), norm(&path.to_string_lossy()));
            assert!(resolved.name.contains("Общий файл"));
            assert_eq!(resolved.position.x, index as i32);
            assert_eq!(resolved.position.y, 42);
            assert_eq!(std::fs::read(&path).unwrap(), b"original file");
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn executable_version_reads_the_fixed_product_version_resource() {
        let mut expected = [0u16; 4];
        for (index, value) in env!("CARGO_PKG_VERSION").split('.').enumerate() {
            expected[index] = value.parse().unwrap();
        }
        assert_eq!(
            executable_version(&std::env::current_exe().unwrap()).unwrap(),
            Some(expected)
        );
    }

    #[test]
    fn malformed_pidls_rejected() {
        for b in [&[][..], &[0], &[1, 0], &[4, 0, 0, 0], &[0, 0, 4]] {
            assert!(validate_pidl(b).is_err());
        }
        assert!(validate_pidl(&[2, 0, 0, 0]).is_ok());
    }

    #[test]
    fn internet_shortcuts_follow_ini_case_sections_and_icon_indices() {
        let source = "[Other]\r\nURL=https://wrong.invalid\r\n[ internetshortcut ]\r\n url = https://example.com/A?Key=Value \r\n IconFile = \"C:\\Иконки\\icon.dll\"\r\n iconindex=-42\r\n";
        let (url, icon, index) = internet_shortcut_fields(source).unwrap();
        assert_eq!(url, "https://example.com/A?Key=Value");
        assert_eq!(icon, r"C:\Иконки\icon.dll");
        assert_eq!(index, -42);
        assert!(internet_shortcut_fields("[Other]\nURL=https://example.com").is_err());
        assert!(
            internet_shortcut_fields("[InternetShortcut]\nURL=\nURL=https://ignored.invalid")
                .is_err()
        );
        assert!(internet_shortcut_fields("[InternetShortcut]\nURL=https://a\0b").is_err());
    }

    #[test]
    fn internet_shortcut_encodings_are_lossless() {
        let expected = "[InternetShortcut]\r\nURL=https://example.com/Путь\r\n";
        for little in [true, false] {
            let mut bytes = if little {
                vec![0xff, 0xfe]
            } else {
                vec![0xfe, 0xff]
            };
            bytes.extend(expected.encode_utf16().flat_map(|c| {
                if little {
                    c.to_le_bytes()
                } else {
                    c.to_be_bytes()
                }
            }));
            assert_eq!(shortcut_text(&bytes).unwrap(), expected);
        }
        let mut utf8_bom = vec![0xef, 0xbb, 0xbf];
        utf8_bom.extend_from_slice(expected.as_bytes());
        assert_eq!(shortcut_text(&utf8_bom).unwrap(), expected);
        assert!(shortcut_text(&[0xff, 0xfe, b'x']).is_err());
        assert!(shortcut_text(&[0xff, 0xfe, 0x00, 0xd8]).is_err());
    }

    #[test]
    fn empty_shortcut_target_is_never_the_working_directory() {
        assert!(!valid_target(&App::default()));
        assert!(!valid_target(&App {
            target: "   ".into(),
            ..Default::default()
        }));
        assert!(valid_target(&App {
            target: "HTTPS://example.com/A".into(),
            ..Default::default()
        }));
    }

    #[test]
    fn malformed_web_shortcuts_are_not_selected_even_with_old_usage_history() {
        // Exercise real .url parsing, target verification and production
        // placement selection: a corrupted link must not displace a working one.
        let directory = std::env::temp_dir().join(format!(
            "desktop-pilot-web-authority-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let broken = directory.join("Broken.url");
        let working = directory.join("Working.url");
        std::fs::write(&broken, b"[InternetShortcut]\r\nURL=https://\r\n").unwrap();
        std::fs::write(
            &working,
            b"[InternetShortcut]\r\nURL=https://example.invalid/task?name=hello%20world\r\n",
        )
        .unwrap();
        let mut broken_app = read_link(&broken).unwrap();
        let mut working_app = read_link(&working).unwrap();
        assert!(!broken_app.valid);
        assert!(working_app.valid);
        let observed_at = crate::model::now();
        broken_app.seed = 1000.0;
        broken_app.seed_at = observed_at;
        working_app.seed = 20.0;
        working_app.seed_at = observed_at;
        let config = crate::model::Config {
            collect: true,
            learning_enabled: false,
            slots: 1,
            ..Default::default()
        };
        let preview = crate::engine::preview_loaded(
            &config,
            &[broken_app.clone(), working_app.clone()],
            &[],
            &crate::model::State::default(),
            true,
            observed_at,
        )
        .unwrap();
        assert_eq!(preview.selected, [working_app.id]);
        assert!(
            !preview
                .ranking
                .iter()
                .find(|r| r.id == broken_app.id)
                .unwrap()
                .eligible
        );
        assert_eq!(
            std::fs::read(&broken).unwrap(),
            b"[InternetShortcut]\r\nURL=https://\r\n"
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn web_shortcut_authority_requires_a_host_without_overrejecting_paths() {
        for target in [
            "https://",
            "HTTP://?q=1",
            "https://#fragment",
            "https://user@",
            "https://:443/path",
            "https://bad host/path",
            r"https://bad\host/path",
        ] {
            assert!(
                !valid_target(&App {
                    target: target.into(),
                    ..Default::default()
                }),
                "{target}"
            );
        }
        for target in [
            "HTTPS://example.invalid/Path?key=Some%20Value",
            "http://localhost:8080/",
            "https://[::1]/",
            "https://user:secret@example.invalid/path with spaces",
        ] {
            assert!(
                valid_target(&App {
                    target: target.into(),
                    ..Default::default()
                }),
                "{target}"
            );
        }
    }

    #[test]
    fn malformed_http_ports_cannot_occupy_quick_slots() {
        // Real .url files and production selection: a high stored score must
        // not let an unlaunchable explicit port displace a usable link.
        let directory = std::env::temp_dir().join(format!(
            "desktop-pilot-web-port-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let broken = directory.join("Malformed port.url");
        let working = directory.join("Working.url");
        let broken_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid:word/project\r\n";
        let working_bytes = b"[InternetShortcut]\r\nURL=https://example.invalid:8443/project\r\n";
        std::fs::write(&broken, broken_bytes).unwrap();
        std::fs::write(&working, working_bytes).unwrap();
        let mut broken_app = read_link(&broken).unwrap();
        let mut working_app = read_link(&working).unwrap();
        assert!(
            !broken_app.valid,
            "a nonnumeric HTTP port is not launchable"
        );
        assert!(working_app.valid);
        let now = crate::model::now();
        broken_app.seed = 500.0;
        broken_app.seed_at = now;
        working_app.seed = 30.0;
        working_app.seed_at = now;
        let config = crate::model::Config {
            slots: 1,
            learning_enabled: false,
            collect: true,
            ..Default::default()
        };
        let preview = crate::engine::preview_loaded(
            &config,
            &[broken_app.clone(), working_app.clone()],
            &[],
            &crate::model::State::default(),
            true,
            now,
        )
        .unwrap();
        assert_eq!(preview.selected, [working_app.id]);
        assert!(
            !preview
                .ranking
                .iter()
                .find(|r| r.id == broken_app.id)
                .unwrap()
                .eligible
        );
        assert_eq!(std::fs::read(&broken).unwrap(), broken_bytes);
        assert_eq!(std::fs::read(&working).unwrap(), working_bytes);
        std::fs::remove_dir_all(directory).unwrap();

        for target in [
            "https://example.invalid:word/",
            "http://example.invalid:65536/",
            "https://example.invalid:12x/",
            "https://example.invalid:1:2/",
            "https://[::1",
            "https://[::1]junk/",
            "https://[not-ipv6]/",
            "https://[::1]:99999/",
        ] {
            assert!(
                !valid_target(&App {
                    target: target.into(),
                    ..Default::default()
                }),
                "{target}"
            );
        }
        for target in [
            "http://localhost:8080/",
            "https://[::1]:443/",
            "https://[2001:db8::1]/",
            "https://example.invalid:65535/path?q=1",
            "https://user:secret@example.invalid:8443/path",
            "https://example.invalid:/existing-shortcut",
        ] {
            assert!(
                valid_target(&App {
                    target: target.into(),
                    ..Default::default()
                }),
                "{target}"
            );
        }
    }

    #[test]
    fn startup_arguments_roundtrip_spaces_quotes_and_trailing_backslashes() {
        for arg in [
            "",
            r"C:\",
            r"C:\My profile\",
            "a\"b",
            r"\server\share\",
            "кириллица 😀",
        ] {
            let command = format!("pilot.exe --home {}", quote_arg(arg));
            let args = command_line_args(&command).unwrap();
            assert_eq!(args, ["pilot.exe", "--home", arg]);
        }
        assert!(command_belongs_to_home(
            r#""C:\Pilot\desktop-pilot.exe" --background --home "C:\Profile A""#,
            Path::new(r"c:\profile a")
        ));
        assert!(!command_belongs_to_home(
            r#""C:\Pilot\desktop-pilot.exe" --background --home "C:\Profile B""#,
            Path::new(r"C:\Profile A")
        ));
    }

    #[test]
    fn pin_matching_preserves_case_sensitive_arguments_and_exact_app_ids() {
        let app = App {
            target: r"C:\browser.exe".into(),
            arguments: "--profile=A".into(),
            ..Default::default()
        };
        assert!(!is_pinned(&app, &[r"c:\browser.exe|--profile=a".into()]));
        assert!(!is_pinned(&app, &[r"something--profile=a".into()]));
        assert!(is_pinned(&app, &[r"c:\browser.exe|--profile=A".into()]));
        let app = App {
            arguments: r"shell:AppsFolder\Publisher.App!Main".into(),
            ..app
        };
        assert!(is_pinned(
            &app,
            &[r"::{namespace}\publisher.app!main".into()]
        ));
        assert!(!is_pinned(
            &app,
            &[r"::{namespace}\otherpublisher.app!main".into()]
        ));
    }

    #[test]
    fn steam_manifest_paths_cannot_escape_common() {
        assert!(valid_steam_directory("42", "My Game"));
        for directory in [
            "",
            ".",
            "..",
            ".. ",
            r"..\outside",
            r"C:\Other",
            "Game.",
            "Game ",
        ] {
            assert!(!valid_steam_directory("42", directory));
        }
        assert!(!valid_steam_directory("42/13", "Game"));
    }

    #[test]
    fn singleton_presence_probe_does_not_take_ownership() {
        let home = std::env::temp_dir().join(format!("test-mutex-{}", std::process::id()));
        assert!(!is_running("test", &home).unwrap());
        let lock = try_exclusive("test", &home).unwrap().unwrap();
        assert!(is_running("test", &home).unwrap());
        assert!(try_exclusive("test", &home).unwrap().is_none());
        drop(lock);
        assert!(!is_running("test", &home).unwrap());
        assert!(try_exclusive("test", &home).unwrap().is_some());
    }
}
