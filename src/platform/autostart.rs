//! "Start VG Switcher on logon", implemented as a Task Scheduler entry that
//! runs at logon with highest privileges. A Run-key entry for an
//! admin-manifested exe would show a UAC prompt at every login; a scheduled
//! task is started elevated by the Task Scheduler service with no prompt.
//!
//! `schtasks.exe` is used instead of the Task Scheduler COM API: the calls
//! are rare, and only the exit code is interpreted, never localized text.

use crate::error::{AppError, Result};
use crate::platform::{cmd, paths};

const TASK_NAME: &str = "VG_Switcher";

/// Flag the scheduled task launches with, so the app starts hidden in the
/// tray instead of opening its window.
pub const TRAY_ARG: &str = "--tray";

fn schtasks(args: &[&str]) -> Result<cmd::CmdOutput> {
    cmd::run(&paths::system_tool("schtasks.exe"), args)
}

pub fn is_enabled() -> bool {
    schtasks(&["/query", "/tn", TASK_NAME]).is_ok_and(|o| o.success())
}

pub fn enable() -> Result<()> {
    let exe = std::env::current_exe().map_err(|e| AppError::io("locating VG_Switcher.exe", e))?;
    let run = format!("\"{}\" {TRAY_ARG}", exe.display());
    let out = schtasks(&["/create", "/tn", TASK_NAME, "/tr", &run, "/sc", "onlogon", "/rl", "highest", "/f"])?;
    if out.success() { Ok(()) } else { Err(AppError::CommandFailed(out.combined())) }
}

pub fn disable() -> Result<()> {
    let out = schtasks(&["/delete", "/tn", TASK_NAME, "/f"])?;
    if out.success() || !is_enabled() { Ok(()) } else { Err(AppError::CommandFailed(out.combined())) }
}
