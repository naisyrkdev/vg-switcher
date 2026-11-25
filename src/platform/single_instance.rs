//! Single-instance guard. A second launch (e.g. a double-click while the
//! autostarted copy sits in the tray) asks the running copy to show its
//! window, then exits, instead of adding a second tray icon that polls and
//! writes the config alongside the first.

use std::ptr::null_mut;
use std::sync::OnceLock;

use winapi::shared::winerror::ERROR_ALREADY_EXISTS;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::synchapi::CreateMutexW;
use winapi::um::winuser::{AllowSetForegroundWindow, HWND_BROADCAST, PostMessageW, RegisterWindowMessageW};

use crate::platform::{OwnedHandle, wide};

const MUTEX_NAME: &str = r"Local\VG_Switcher.SingleInstance";
const SHOW_MESSAGE_NAME: &str = "VG_Switcher.ShowMainWindow";
const ASFW_ANY: u32 = u32::MAX;

/// Held for the lifetime of the app. Dropping it releases the instance lock.
pub struct InstanceGuard(#[allow(dead_code)] Option<OwnedHandle>);

/// `None` if another instance already holds the lock.
pub fn acquire() -> Option<InstanceGuard> {
    unsafe {
        let handle = OwnedHandle::new(CreateMutexW(null_mut(), 0, wide(MUTEX_NAME).as_ptr()));
        if handle.is_some() && GetLastError() == ERROR_ALREADY_EXISTS {
            return None;
        }
        if handle.is_none() {
            log::warn!("could not create the single-instance mutex (error {}); continuing", GetLastError());
        }
        Some(InstanceGuard(handle))
    }
}

/// The registered window message the running instance listens for.
pub fn show_request_message() -> u32 {
    static MSG: OnceLock<u32> = OnceLock::new();
    *MSG.get_or_init(|| unsafe { RegisterWindowMessageW(wide(SHOW_MESSAGE_NAME).as_ptr()) })
}

/// Asks the already-running instance to bring its main window forward.
pub fn ask_running_instance_to_show() {
    unsafe {
        // Let the other process take the foreground: we're the one the user
        // just interacted with, so Windows would otherwise refuse it.
        AllowSetForegroundWindow(ASFW_ANY);
        PostMessageW(HWND_BROADCAST, show_request_message(), 0, 0);
    }
}
