//! Scope cloud-placeholder visibility to the current filesystem check.
use anyhow::{Result, ensure};
use std::{marker::PhantomData, rc::Rc, sync::OnceLock};
use windows::{
    Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
    core::{s, w},
};

type SetMode = unsafe extern "system" fn(i8) -> i8;

fn setter() -> Option<SetMode> {
    static API: OnceLock<Option<SetMode>> = OnceLock::new();
    *API.get_or_init(|| unsafe {
        let module = GetModuleHandleW(w!("ntdll.dll")).ok()?;
        let address = GetProcAddress(module, s!("RtlSetThreadPlaceholderCompatibilityMode"))?;
        // Documented ntdll ABI: CHAR function(CHAR). Resolve dynamically so
        // pre-Cloud-Files Windows versions do not acquire a missing import.
        Some(std::mem::transmute::<
            unsafe extern "system" fn() -> isize,
            SetMode,
        >(address))
    })
}

pub(in crate::organize) struct Guard {
    previous: Option<(SetMode, i8)>,
    // The mode belongs to this OS thread, not to the process. A guard must
    // never be moved to another thread before its previous mode is restored.
    _thread_bound: PhantomData<Rc<()>>,
}

pub(in crate::organize) fn expose() -> Result<Guard> {
    let previous = if let Some(set) = setter() {
        let previous = unsafe { set(2) };
        ensure!(
            previous >= 0,
            "Cloud placeholder visibility could not be verified"
        );
        Some((set, previous))
    } else {
        // The Cloud Files compatibility API and placeholders were introduced
        // together in Windows 10 1709. Ordinary reparse checks still apply.
        None
    };
    Ok(Guard {
        previous,
        _thread_bound: PhantomData,
    })
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some((set, previous)) = self.previous {
            // A previously returned valid mode is accepted on this same thread.
            unsafe { set(previous) };
        }
    }
}
