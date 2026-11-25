//! Safe wrappers over the Windows APIs the app needs. Everything `unsafe`
//! and Win32-specific outside of UI painting lives here, so the rest of the
//! app works with plain Rust types and `AppError`.
//!
//! VG Switcher is Windows-only by product scope (Riot Vanguard only exists
//! on Windows), so there is deliberately no cross-platform abstraction.

#[cfg(not(windows))]
compile_error!("VG Switcher only supports Windows.");

pub mod autostart;
pub mod cmd;
pub mod paths;
pub mod power;
pub mod processes;
pub mod registry;
pub mod security;
pub mod services;
pub mod single_instance;
pub mod startup_apps;

use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
use winapi::um::winnt::HANDLE;

/// UTF-16, NUL-terminated copy of `s` for passing to `*W` APIs.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Decodes a UTF-16 buffer up to its first NUL.
pub fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// A kernel handle closed on drop.
pub(crate) struct OwnedHandle(HANDLE);

impl OwnedHandle {
    /// Takes ownership of `handle`, or `None` for a null/invalid handle.
    pub(crate) fn new(handle: HANDLE) -> Option<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE { None } else { Some(OwnedHandle(handle)) }
    }

    pub(crate) fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_round_trips() {
        let w = wide("Riot Vanguard");
        assert_eq!(w.last(), Some(&0));
        assert_eq!(from_wide(&w), "Riot Vanguard");
    }

    #[test]
    fn from_wide_stops_at_nul() {
        let buf = [b'a' as u16, b'b' as u16, 0, b'c' as u16];
        assert_eq!(from_wide(&buf), "ab");
    }
}
