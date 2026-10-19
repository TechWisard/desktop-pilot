use crate::{
    i18n::{self, text as t},
    model, native,
    store::Store,
};
use anyhow::{Context, Result, ensure};
use std::{
    cell::{Cell, RefCell},
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

const CALLBACK: u32 = WM_APP + 7;
const ICON_ID: u32 = 1;
const KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;
const TIMER: usize = 1;
// MAKEINTRESOURCEW(1): this is an integer resource identifier, never dereferenced.
// A u16 dangling pointer has address 2 and would select the wrong resource.
#[allow(clippy::manual_dangling_ptr)]
const APP_ICON_RESOURCE: PCWSTR = PCWSTR(1usize as *const u16);

pub fn class_name(home: &Path) -> String {
    format!(
        "DesktopPilotTray-{}",
        &model::hash(model::norm(&home.to_string_lossy()).as_bytes())[..12]
    )
}

pub fn request_close(home: &Path) -> Result<()> {
    let class = native::wide(&class_name(home));
    unsafe {
        if let Ok(window) = FindWindowW(PCWSTR(class.as_ptr()), None)
            && let Err(error) = PostMessageW(window, WM_CLOSE, WPARAM(0), LPARAM(0))
            && error.code() != windows::core::HRESULT::from_win32(ERROR_INVALID_WINDOW_HANDLE.0)
        {
            return Err(error.into());
        }
    }
    Ok(())
}

fn open_settings(home: &Path) -> Result<()> {
    std::process::Command::new(std::env::current_exe()?)
        .arg("--settings")
        .arg("--home")
        .arg(home)
        .creation_flags(0x08000000)
        .spawn()
        .context("Cannot open Desktop Pilot settings")?;
    Ok(())
}

fn copy_text(buffer: &mut [u16], text: &str) {
    buffer.fill(0);
    let mut length = 0;
    for character in text.chars().filter(|c| *c != '\0') {
        let mut chars = [0u16; 2];
        let chars = character.encode_utf16(&mut chars);
        if length + chars.len() >= buffer.len() {
            break;
        }
        buffer[length..length + chars.len()].copy_from_slice(chars);
        length += chars.len();
    }
}

struct TrayState {
    home: PathBuf,
    store: Store,
    icon: HICON,
    taskbar_created: u32,
    test_mode: bool,
    icon_added: Cell<bool>,
    tooltip: RefCell<String>,
    icon_missing_since: Cell<Option<Instant>>,
    terminal_error: RefCell<Option<String>>,
}

impl TrayState {
    fn icon_data(&self, window: HWND) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: ICON_ID,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: CALLBACK,
            hIcon: self.icon,
            Anonymous: NOTIFYICONDATAW_0 {
                uVersion: NOTIFYICON_VERSION_4,
            },
            ..Default::default()
        }
    }

    fn sync_icon(&self, window: HWND) -> Result<()> {
        if self.test_mode {
            return Ok(());
        }
        let config: model::Config = self.store.get("config")?;
        i18n::set(&config.language);
        let tip = format!(
            "Desktop Pilot\n{} · {}",
            if config.collect {
                t("Учёт включён", "Tracking on")
            } else {
                t("Учёт на паузе", "Tracking paused")
            },
            if config.automatic {
                t("Автоматика включена", "Automation on")
            } else {
                t("Автоматика на паузе", "Automation paused")
            }
        );
        if self.icon_added.get() && *self.tooltip.borrow() == tip {
            return Ok(());
        }
        let mut data = self.icon_data(window);
        copy_text(&mut data.szTip, &tip);
        unsafe {
            if self.icon_added.get() {
                ensure!(
                    Shell_NotifyIconW(NIM_MODIFY, &data).as_bool(),
                    "Cannot update the Desktop Pilot tray icon"
                );
            } else {
                ensure!(
                    Shell_NotifyIconW(NIM_ADD, &data).as_bool(),
                    "Не удалось создать значок в трее"
                );
                self.icon_added.set(true);
                if !Shell_NotifyIconW(NIM_SETVERSION, &data).as_bool() {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                    self.icon_added.set(false);
                    anyhow::bail!("Cannot enable keyboard access for the tray icon");
                }
            }
        }
        self.tooltip.replace(tip);
        self.icon_missing_since.set(None);
        Ok(())
    }

    fn retry_icon(&self, window: HWND) {
        if let Err(error) = self.sync_icon(window) {
            let since = self.icon_missing_since.get().unwrap_or_else(Instant::now);
            self.icon_missing_since.set(Some(since));
            if since.elapsed() >= Duration::from_secs(15) {
                self.terminal_error.replace(Some(format!("{error:#}")));
                let _ = self.store.set("shutdown", &true);
                unsafe {
                    let _ = DestroyWindow(window);
                }
            }
        }
    }

    fn report_error(&self, window: HWND, error: anyhow::Error) {
        let message = i18n::runtime(&format!("{error:#}"));
        let _ = self.store.event("error", &message);
        let _ = self.store.set("status", &message);
        if self.icon_added.get() {
            let mut data = self.icon_data(window);
            data.uFlags = NIF_INFO;
            data.dwInfoFlags = NIIF_WARNING;
            copy_text(&mut data.szInfoTitle, "Desktop Pilot");
            copy_text(&mut data.szInfo, &message);
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
        }
    }

    fn menu(&self, window: HWND, anchor: WPARAM) -> Result<()> {
        let config: model::Config = self.store.get("config")?;
        i18n::set(&config.language);
        unsafe {
            let menu = CreatePopupMenu()?;
            let selection = (|| -> Result<i32> {
                for (id, label) in [
                    (1, t("Открыть настройки", "Open settings")),
                    (
                        2,
                        if config.automatic {
                            t(
                                "Приостановить автоматические изменения",
                                "Pause automatic changes",
                            )
                        } else {
                            t(
                                "Возобновить автоматические изменения",
                                "Resume automatic changes",
                            )
                        },
                    ),
                    (
                        3,
                        if config.collect {
                            t("Приостановить учёт активности", "Pause activity tracking")
                        } else {
                            t("Возобновить учёт активности", "Resume activity tracking")
                        },
                    ),
                ] {
                    AppendMenuW(menu, MF_STRING, id, PCWSTR(native::wide(label).as_ptr()))?;
                }
                AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
                AppendMenuW(
                    menu,
                    MF_STRING,
                    4,
                    PCWSTR(
                        native::wide(t("Завершить фоновую работу", "Exit background process"))
                            .as_ptr(),
                    ),
                )?;
                // VERSION_4 supplies the icon location for keyboard invocation;
                // signed coordinates also support monitors left of the primary.
                let mut point = POINT {
                    x: (anchor.0 as u16 as i16) as i32,
                    y: ((anchor.0 >> 16) as u16 as i16) as i32,
                };
                if point.x == -1 && point.y == -1 {
                    GetCursorPos(&mut point)?;
                }
                let _ = SetForegroundWindow(window);
                Ok(TrackPopupMenu(
                    menu,
                    TPM_RETURNCMD | TPM_RIGHTBUTTON,
                    point.x,
                    point.y,
                    0,
                    window,
                    None,
                )
                .0)
            })();
            let _ = DestroyMenu(menu);
            let _ = PostMessageW(window, WM_NULL, WPARAM(0), LPARAM(0));
            let _ = Shell_NotifyIconW(NIM_SETFOCUS, &self.icon_data(window));
            match selection? {
                1 => open_settings(&self.home)?,
                id @ (2 | 3) => {
                    let collect = self.store.transaction(|| {
                        let mut latest: model::Config = self.store.get("config")?;
                        if id == 2 {
                            latest.automatic = !config.automatic;
                        } else {
                            latest.collect = !config.collect;
                        }
                        latest.validate()?;
                        self.store.set("config", &latest)?;
                        self.store.event(
                            "settings",
                            if id == 2 {
                                "Automation toggled from the tray"
                            } else {
                                "Activity tracking toggled from the tray"
                            },
                        )?;
                        Ok(latest.collect)
                    })?;
                    crate::activity::set_enabled(collect);
                    self.sync_icon(window)?;
                }
                4 => {
                    self.store.set("shutdown", &true)?;
                    DestroyWindow(window)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}

unsafe extern "system" fn procedure(window: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const TrayState;
        if pointer.is_null() {
            return DefWindowProcW(window, message, wp, lp);
        }
        let state = &*pointer;
        if message == state.taskbar_created {
            state.icon_added.set(false);
            // Explorer may announce its restart before the taskbar is ready;
            // the timer keeps trying without stopping the background worker.
            state.retry_icon(window);
            return LRESULT(0);
        }
        match message {
            CALLBACK => {
                let event = (lp.0 as u32) & 0xffff;
                if (lp.0 as u32 >> 16) == ICON_ID {
                    let result = match event {
                        NIN_SELECT | KEY_SELECT => open_settings(&state.home),
                        WM_CONTEXTMENU => state.menu(window, wp),
                        _ => Ok(()),
                    };
                    if let Err(error) = result {
                        state.report_error(window, error);
                    }
                }
                LRESULT(0)
            }
            WM_TIMER if wp.0 == TIMER => {
                match state.store.get::<bool>("shutdown") {
                    Ok(true) => {
                        let _ = DestroyWindow(window);
                    }
                    Ok(false) => state.retry_icon(window),
                    Err(error) => state.report_error(window, error),
                }
                LRESULT(0)
            }
            WM_QUERYENDSESSION => LRESULT(1),
            WM_ENDSESSION if wp.0 != 0 => {
                let _ = state.store.set("shutdown", &true);
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_CLOSE => {
                let _ = state.store.set("shutdown", &true);
                let _ = DestroyWindow(window);
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(window, TIMER);
                if state.icon_added.replace(false) {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &state.icon_data(window));
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            WM_NCDESTROY => {
                SetWindowLongPtrW(window, GWLP_USERDATA, 0);
                DefWindowProcW(window, message, wp, lp)
            }
            _ => DefWindowProcW(window, message, wp, lp),
        }
    }
}

pub fn run(home: PathBuf) -> Result<()> {
    let store = Store::open(&home)?;
    let test_mode = !store.get::<String>("test_desktop")?.is_empty();
    let class_text = native::wide(&class_name(&home));
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = PCWSTR(class_text.as_ptr());
        let icon =
            LoadIconW(instance, APP_ICON_RESOURCE).or_else(|_| LoadIconW(None, IDI_APPLICATION))?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        ensure!(
            RegisterClassW(&wc) != 0,
            "Cannot register the Desktop Pilot tray window"
        );
        let state = Box::new(TrayState {
            home,
            store,
            icon,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
            test_mode,
            icon_added: Cell::new(false),
            tooltip: RefCell::new(String::new()),
            icon_missing_since: Cell::new(None),
            terminal_error: RefCell::new(None),
        });
        let window = match CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Desktop Pilot background"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        ) {
            Ok(window) => window,
            Err(error) => {
                let _ = UnregisterClassW(class, instance);
                return Err(error.into());
            }
        };
        SetWindowLongPtrW(
            window,
            GWLP_USERDATA,
            (&*state as *const TrayState) as isize,
        );
        let result = (|| -> Result<()> {
            ensure!(
                state.taskbar_created != 0,
                "Cannot register Explorer restart notifications"
            );
            ensure!(
                SetTimer(window, TIMER, 1000, None) != 0,
                "Cannot start the tray shutdown timer"
            );
            state.retry_icon(window);
            // Covers a worker failure between its startup and window creation.
            if state.store.get::<bool>("shutdown")? {
                DestroyWindow(window)?;
            }
            let mut message = MSG::default();
            loop {
                let code = GetMessageW(&mut message, None, 0, 0).0;
                ensure!(code >= 0, "The Windows tray message loop failed");
                if code == 0 {
                    break;
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if let Some(error) = state.terminal_error.borrow().as_ref() {
                anyhow::bail!("{error}");
            }
            Ok(())
        })();
        if IsWindow(window).as_bool() {
            let _ = DestroyWindow(window);
        }
        let _ = UnregisterClassW(class, instance);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tooltip_truncation_preserves_utf16_and_nul_termination() {
        let mut buffer = [9u16; 4];
        copy_text(&mut buffer, "ab😀long");
        assert_eq!(String::from_utf16(&buffer[..2]).unwrap(), "ab");
        assert_eq!(&buffer[2..], &[0, 0]);
        copy_text(&mut buffer, "😀x");
        assert_eq!(String::from_utf16(&buffer[..3]).unwrap(), "😀x");
        assert_eq!(buffer[3], 0);
    }
    #[test]
    fn tray_windows_are_profile_scoped() {
        assert_ne!(
            class_name(Path::new(r"C:\Profiles\One")),
            class_name(Path::new(r"C:\Profiles\Two"))
        );
        assert_eq!(
            class_name(Path::new(r"C:\Profiles\One")),
            class_name(Path::new(r"c:/profiles/one"))
        );
    }
}
