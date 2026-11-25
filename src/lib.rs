//! VG Switcher: a tray app to switch Riot Vanguard on and off.
//!
//! Layers, from the bottom up:
//! - `platform`: safe wrappers over the Windows APIs used.
//! - `config`, `vanguard`, `riot`: settings and domain logic.
//! - `theme`: palettes and owner-drawn painting of Win32 controls.
//! - `ui`: the windows and the tray, each mapping clicks to an `Action`.
//! - `app`: state, startup and the single action dispatcher.

mod action;
mod app;
mod config;
mod error;
mod logging;
mod platform;
mod riot;
mod theme;
mod ui;
mod vanguard;

pub use app::run;
