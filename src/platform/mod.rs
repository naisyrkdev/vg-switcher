//! OS abstraction point. Everything outside this module (`vanguard`, `config`,
//! the UI) only calls `platform::process::run_line` and
//! `platform::autostart::*`, so a future Linux/macOS backend can be dropped
//! in here without touching the rest of the app. The GUI itself
//! (`native-windows-gui` + the raw Win32 dark-mode hooks in `darkmode.rs`)
//! is a separate concern and would need its own backend swap on another OS.
//!
//! Only a Windows backend exists today - intentionally, per product scope.

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::{autostart, process, startup_apps};

#[cfg(not(windows))]
compile_error!(
    "VG_Switcher only supports Windows today. This module is the OS \
     abstraction point for a future Linux/macOS backend (see platform/windows \
     for what such a backend would need to provide: process::run_line and \
     the autostart:: functions)."
);
