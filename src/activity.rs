use anyhow::Result;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::{
    Foundation::*,
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64},
    UI::WindowsAndMessaging::*,
};

#[derive(Clone, Copy, Default)]
pub struct Sample {
    pub window: isize,
    pub tick: u64,
    pub serial: u64,
}
impl Sample {
    pub fn recent(self, tick: u64, seconds: u32) -> bool {
        self.serial > 0 && tick.saturating_sub(self.tick) < u64::from(seconds) * 1000
    }
}
static LAST: Mutex<Sample> = Mutex::new(Sample {
    window: 0,
    tick: 0,
    serial: 0,
});
static ENABLED: AtomicBool = AtomicBool::new(false);
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}
pub fn snapshot() -> Sample {
    LAST.lock().map(|s| *s).unwrap_or_default()
}
pub fn tick() -> u64 {
    unsafe { GetTickCount64() }
}
fn record(window: HWND) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    if !window.is_invalid()
        && let Ok(mut sample) = LAST.try_lock()
    {
        *sample = Sample {
            window: window.0 as isize,
            tick: tick(),
            serial: sample.serial.saturating_add(1),
        };
    }
}
fn keyboard_allowed(message: u32, flags: u32) -> bool {
    matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN)
        && flags & (LLKHF_INJECTED.0 | LLKHF_LOWER_IL_INJECTED.0) == 0
}
fn mouse_allowed(message: u32, flags: u32) -> bool {
    matches!(
        message,
        WM_LBUTTONDOWN
            | WM_RBUTTONDOWN
            | WM_MBUTTONDOWN
            | WM_XBUTTONDOWN
            | WM_MOUSEWHEEL
            | WM_MOUSEHWHEEL
    ) && flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) == 0
}
unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && lp.0 != 0 {
            // Read provenance flags only. Never inspect or retain key codes or text.
            let event = &*(lp.0 as *const KBDLLHOOKSTRUCT);
            if keyboard_allowed(wp.0 as u32, event.flags.0) {
                record(GetForegroundWindow());
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}
unsafe extern "system" fn mouse(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        if code == HC_ACTION as i32 && lp.0 != 0 {
            let event = &*(lp.0 as *const MSLLHOOKSTRUCT);
            if mouse_allowed(wp.0 as u32, event.flags) {
                let target = GetAncestor(WindowFromPoint(event.pt), GA_ROOT);
                let foreground = GetAncestor(GetForegroundWindow(), GA_ROOT);
                if target == foreground {
                    record(foreground);
                }
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}
pub struct Hooks {
    keyboard: HHOOK,
    mouse: HHOOK,
}
impl Hooks {
    pub fn install() -> Result<Self> {
        unsafe {
            let module = GetModuleHandleW(None)?;
            let keyboard =
                SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), HINSTANCE(module.0), 0)?;
            match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), HINSTANCE(module.0), 0) {
                Ok(mouse) => Ok(Self { keyboard, mouse }),
                Err(error) => {
                    let _ = UnhookWindowsHookEx(keyboard);
                    Err(error.into())
                }
            }
        }
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.keyboard);
            let _ = UnhookWindowsHookEx(self.mouse);
        }
    }
}

#[derive(Default)]
pub struct FocusEvidence {
    window: isize,
    baseline: u64,
    confirmed: bool,
}
impl FocusEvidence {
    pub fn active(&mut self, window: isize, sample: Sample, tick: u64, idle_seconds: u32) -> bool {
        if self.window != window {
            self.window = window;
            self.baseline = sample.serial;
            self.confirmed = false;
        }
        if sample.serial > self.baseline && sample.window == window {
            self.confirmed = true;
        }
        self.confirmed && sample.window == window && sample.recent(tick, idle_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn injected_keyboard_and_mouse_are_excluded() {
        assert!(keyboard_allowed(WM_KEYDOWN, 0));
        assert!(!keyboard_allowed(WM_KEYDOWN, LLKHF_INJECTED.0));
        assert!(!keyboard_allowed(WM_KEYDOWN, LLKHF_LOWER_IL_INJECTED.0));
        assert!(mouse_allowed(WM_LBUTTONDOWN, 0));
        assert!(!mouse_allowed(WM_LBUTTONDOWN, LLMHF_INJECTED));
        assert!(!mouse_allowed(WM_MOUSEWHEEL, LLMHF_LOWER_IL_INJECTED));
        assert!(!mouse_allowed(WM_MOUSEMOVE, 0));
    }
    #[test]
    fn startup_window_without_interaction_never_qualifies() {
        let mut focus = FocusEvidence::default();
        let other = Sample {
            window: 10,
            tick: 1000,
            serial: 1,
        };
        for tick in 1000..2000 {
            assert!(!focus.active(20, other, tick, 120));
        }
    }
    #[test]
    fn evidence_must_arrive_in_the_active_window_and_expires() {
        let mut focus = FocusEvidence::default();
        assert!(!focus.active(20, Sample::default(), 1000, 120));
        assert!(!focus.active(
            20,
            Sample {
                window: 10,
                tick: 1000,
                serial: 1
            },
            1000,
            120
        ));
        let input = Sample {
            window: 20,
            tick: 1000,
            serial: 2,
        };
        assert!(focus.active(20, input, 2000, 120));
        assert!(!focus.active(20, input, 121001, 120));
        assert!(!focus.active(30, input, 2000, 120));
    }
}
