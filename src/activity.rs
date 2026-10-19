use anyhow::{Context, Result, ensure};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
};
use windows::Win32::{
    Foundation::*,
    System::{
        LibraryLoader::GetModuleHandleW, SystemInformation::GetTickCount64,
        Threading::GetCurrentThreadId,
    },
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
        self.serial > 0
            && self.window != 0
            && tick >= self.tick
            && tick - self.tick < u64::from(seconds) * 1000
    }
}
static LAST: Mutex<Sample> = Mutex::new(Sample {
    window: 0,
    tick: 0,
    serial: 0,
});
static ENABLED: AtomicBool = AtomicBool::new(false);
static PENDING_CLICK: Mutex<Option<(isize, u64)>> = Mutex::new(None);
pub fn set_enabled(enabled: bool) {
    if ENABLED.swap(enabled, Ordering::AcqRel) != enabled {
        if let Ok(mut pending) = PENDING_CLICK.lock() {
            *pending = None;
        }
        if let Ok(mut sample) = LAST.lock() {
            sample.window = 0;
            sample.tick = 0;
        }
    }
}
pub fn snapshot() -> Sample {
    // A low-level mouse hook runs before Windows activates the clicked window.
    // Confirm that activation on the sampler thread, keeping only its handle
    // and timestamp and never recording an inactive window's wheel events.
    let pending = PENDING_CLICK.lock().ok().and_then(|mut pending| {
        let (window, at) = (*pending)?;
        let now = tick();
        if now.saturating_sub(at) > 3000 || !ENABLED.load(Ordering::Acquire) {
            *pending = None;
            return None;
        }
        if unsafe { GetAncestor(GetForegroundWindow(), GA_ROOT).0 as isize } == window {
            *pending = None;
            Some((window, at))
        } else {
            None
        }
    });
    if let Some((window, at)) = pending {
        record_at(HWND(window as *mut _), at);
    }
    LAST.lock().map(|s| *s).unwrap_or_default()
}
pub fn tick() -> u64 {
    unsafe { GetTickCount64() }
}
fn record(window: HWND) {
    if let Ok(mut pending) = PENDING_CLICK.try_lock() {
        *pending = None;
    }
    record_at(window, tick());
}

fn record_at(window: HWND, at: u64) {
    if !window.is_invalid()
        && let Ok(mut sample) = LAST.try_lock()
        && ENABLED.load(Ordering::Acquire)
        && at >= sample.tick
    {
        *sample = Sample {
            window: window.0 as isize,
            tick: at,
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
                } else if !target.is_invalid()
                    && matches!(
                        wp.0 as u32,
                        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
                    )
                    && ENABLED.load(Ordering::Acquire)
                    && let Ok(mut pending) = PENDING_CLICK.try_lock()
                {
                    *pending = Some((target.0 as isize, tick()));
                }
            }
        }
        CallNextHookEx(None, code, wp, lp)
    }
}
pub struct Hooks {
    thread: Option<std::thread::JoinHandle<()>>,
    thread_id: u32,
}
impl Hooks {
    pub fn install() -> Result<Self> {
        let (ready, result) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("input-provenance".into())
            .spawn(move || {
                let install = (|| -> Result<(HHOOK, HHOOK)> {
                    unsafe {
                        let module = GetModuleHandleW(None)?;
                        let keyboard = SetWindowsHookExW(
                            WH_KEYBOARD_LL,
                            Some(keyboard),
                            HINSTANCE(module.0),
                            0,
                        )?;
                        match SetWindowsHookExW(WH_MOUSE_LL, Some(mouse), HINSTANCE(module.0), 0) {
                            Ok(mouse) => Ok((keyboard, mouse)),
                            Err(error) => {
                                let _ = UnhookWindowsHookEx(keyboard);
                                Err(error.into())
                            }
                        }
                    }
                })();
                match install {
                    Ok((keyboard, mouse)) => unsafe {
                        let mut message = MSG::default();
                        // Establish the queue before announcing readiness, so Drop
                        // can always post WM_QUIT even during startup failures.
                        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
                        if ready.send(Ok(GetCurrentThreadId())).is_ok() {
                            while GetMessageW(&mut message, None, 0, 0).0 > 0 {
                                let _ = TranslateMessage(&message);
                                DispatchMessageW(&message);
                            }
                        }
                        let _ = UnhookWindowsHookEx(keyboard);
                        let _ = UnhookWindowsHookEx(mouse);
                    },
                    Err(error) => {
                        let _ = ready.send(Err(format!("{error:#}")));
                    }
                }
            })?;
        let initialized = result
            .recv()
            .context("Input monitoring thread stopped during startup")?;
        let thread_id = match initialized {
            Ok(id) => id,
            Err(error) => {
                let _ = thread.join();
                anyhow::bail!("Cannot start input monitoring: {error}");
            }
        };
        ensure!(thread_id != 0, "Invalid input monitoring thread");
        Ok(Self {
            thread: Some(thread),
            thread_id,
        })
    }
}
impl Drop for Hooks {
    fn drop(&mut self) {
        unsafe {
            let _ = PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        set_enabled(false);
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
        let seen_window = self.window != 0;
        if window == 0 {
            *self = Self::default();
            return false;
        }
        if self.window != window {
            self.window = window;
            self.confirmed = false;
        }
        // Compare with the previous poll, not the first sample of the new
        // window: the click that activates it is itself qualifying evidence.
        if seen_window && sample.serial > self.baseline && sample.window == window {
            self.confirmed = true;
        }
        self.baseline = sample.serial;
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

    #[test]
    fn first_click_in_a_new_foreground_window_is_enough() {
        let mut focus = FocusEvidence::default();
        assert!(!focus.active(10, Sample::default(), 1000, 120));
        let click = Sample {
            window: 20,
            tick: 1500,
            serial: 1,
        };
        assert!(focus.active(20, click, 2000, 120));
        assert!(focus.active(20, click, 3000, 120));
        assert!(!focus.active(30, click, 4000, 120));
        assert!(!focus.active(20, click, 5000, 120));
    }

    #[test]
    fn startup_stale_or_future_samples_do_not_count() {
        let input = Sample {
            window: 20,
            tick: 2000,
            serial: 1,
        };
        let mut focus = FocusEvidence::default();
        assert!(!focus.active(20, input, 3000, 120));
        assert!(!focus.active(20, input, 4000, 120));
        assert!(!input.recent(1999, 120));
        assert!(!Sample { window: 0, ..input }.recent(3000, 120));
        assert!(!input.recent(122000, 120));
    }
}
