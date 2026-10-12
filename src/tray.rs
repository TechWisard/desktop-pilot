use crate::{
    i18n::{self, text as t},
    model, native,
    store::Store,
};
use anyhow::Result;
use std::{os::windows::process::CommandExt, path::PathBuf, sync::OnceLock};
use windows::{
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
static HOME: OnceLock<PathBuf> = OnceLock::new();
const CALLBACK: u32 = WM_APP + 7;
pub fn class_name(home: &std::path::Path) -> String {
    format!(
        "DesktopPilotTray-{}",
        &model::hash(model::norm(&home.to_string_lossy()).as_bytes())[..12]
    )
}
fn open_settings() {
    if let (Ok(exe), Some(home)) = (std::env::current_exe(), HOME.get()) {
        let _ = std::process::Command::new(exe)
            .arg("--settings")
            .arg("--home")
            .arg(home)
            .creation_flags(0x08000000)
            .spawn();
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            CALLBACK => {
                match lp.0 as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => open_settings(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        if let Some(home) = HOME.get()
                            && let Ok(store) = Store::open(home)
                            && let Ok(cfg) = store.get::<model::Config>("config")
                        {
                            i18n::set(&cfg.language);
                        }
                        if let Ok(menu) = CreatePopupMenu() {
                            let _ = AppendMenuW(
                                menu,
                                MF_STRING,
                                1,
                                PCWSTR(
                                    native::wide(t("Открыть настройки", "Open settings")).as_ptr(),
                                ),
                            );
                            let _ = AppendMenuW(
                                menu,
                                MF_STRING,
                                2,
                                PCWSTR(
                                    native::wide(t(
                                        "Пауза / продолжить автоматику",
                                        "Pause / resume automation",
                                    ))
                                    .as_ptr(),
                                ),
                            );
                            let _ = AppendMenuW(
                                menu,
                                MF_STRING,
                                3,
                                PCWSTR(
                                    native::wide(t(
                                        "Завершить фоновую работу",
                                        "Exit background process",
                                    ))
                                    .as_ptr(),
                                ),
                            );
                            let mut p = POINT::default();
                            let _ = GetCursorPos(&mut p);
                            let _ = SetForegroundWindow(hwnd);
                            let id = TrackPopupMenu(
                                menu,
                                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                                p.x,
                                p.y,
                                0,
                                hwnd,
                                None,
                            )
                            .0;
                            let _ = DestroyMenu(menu);
                            if id == 1 {
                                open_settings();
                            } else if let Some(home) = HOME.get()
                                && let Ok(s) = Store::open(home)
                            {
                                if id == 2 {
                                    if let Ok(mut c) = s.get::<crate::model::Config>("config") {
                                        c.automatic = !c.automatic;
                                        let _ = s.set("config", &c);
                                    }
                                } else if id == 3 {
                                    let _ = s.set("shutdown", &true);
                                    let _ = DestroyWindow(hwnd);
                                }
                            }
                        }
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let data = NOTIFYICONDATAW {
                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: hwnd,
                    uID: 1,
                    ..Default::default()
                };
                let _ = Shell_NotifyIconW(NIM_DELETE, &data);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
}
pub fn run(home: PathBuf) -> Result<()> {
    let class_text = native::wide(&class_name(&home));
    let _ = HOME.set(home);
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = PCWSTR(class_text.as_ptr());
        let icon = LoadIconW(None, IDI_APPLICATION)?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Desktop Pilot"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        )?;
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: CALLBACK,
            hIcon: icon,
            ..Default::default()
        };
        let tip = native::wide("Desktop Pilot");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        anyhow::ensure!(
            Shell_NotifyIconW(NIM_ADD, &data).as_bool(),
            "Не удалось создать значок в трее"
        );
        let taskbar = RegisterWindowMessageW(w!("TaskbarCreated"));
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result <= 0 {
                break;
            }
            if message.message == taskbar {
                let _ = Shell_NotifyIconW(NIM_ADD, &data);
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}
