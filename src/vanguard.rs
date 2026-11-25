//! Riot Vanguard: status of its service, its tray process and its startup
//! entry, plus running the user's enable/disable command lists.

use crate::error::{AppError, Result};
use crate::platform::services::{self, StartType};
use crate::platform::startup_apps::{self, StartupStatus};
use crate::platform::{cmd, processes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VanguardStatus {
    Enabled,
    Disabled,
    NotInstalled,
    /// The status couldn't be read (access denied, SCM unavailable...).
    /// Kept apart from `NotInstalled` so a failure isn't reported as fact.
    Unknown,
}

/// Image name of Riot's Vanguard tray app. `disable_commands` only change
/// the services' start types for the next boot, so a tray instance started
/// earlier lingers until reboot unless it's killed directly.
const TRAY_PROCESS: &str = "vgtray.exe";

/// Everything the main window and tray show, read in one go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Snapshot {
    pub service: VanguardStatus,
    pub tray_running: bool,
    pub startup: StartupStatus,
}

impl Snapshot {
    pub fn read(service: &str) -> Self {
        Snapshot { service: get_status(service), tray_running: tray_running(), startup: startup_apps::status() }
    }
}

pub fn status_from_start_type(start_type: Option<StartType>) -> VanguardStatus {
    match start_type {
        None => VanguardStatus::NotInstalled,
        Some(StartType::Disabled) => VanguardStatus::Disabled,
        Some(_) => VanguardStatus::Enabled,
    }
}

/// Reads the configured start type of `service` (normally `vgc`).
pub fn get_status(service: &str) -> VanguardStatus {
    match services::query_start_type(service.trim()) {
        Ok(start_type) => status_from_start_type(start_type),
        Err(e) => {
            log::warn!("reading the status of service {service:?} failed: {e}");
            VanguardStatus::Unknown
        }
    }
}

/// Kills the lingering Vanguard tray process. Not running is not an error.
pub fn kill_tray() -> Result<()> {
    processes::kill_all(TRAY_PROCESS).map(|n| log::info!("terminated {n} {TRAY_PROCESS} process(es)"))
}

pub fn tray_running() -> bool {
    processes::is_running(TRAY_PROCESS)
}

/// The lines of a command list that actually run: trimmed, without blank
/// lines and `#` comments.
pub fn executable_lines(commands: &[String]) -> impl Iterator<Item = &str> {
    commands.iter().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#'))
}

/// Runs every executable line in `commands`, continuing past failures
/// (some lines are best-effort) and returning a combined error if any failed.
pub fn run_commands(commands: &[String]) -> Result<()> {
    let errors: Vec<String> = executable_lines(commands)
        .filter_map(|line| {
            log::info!("running: {line}");
            match cmd::run_shell_line(line) {
                Ok(out) if out.success() => None,
                Ok(out) => Some(format!("{line}\n  -> {}", out.combined())),
                Err(e) => Some(format!("{line}\n  -> {e}")),
            }
        })
        .collect();
    if errors.is_empty() { Ok(()) } else { Err(AppError::CommandFailed(errors.join("\n\n"))) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_start_types_to_status() {
        assert_eq!(status_from_start_type(None), VanguardStatus::NotInstalled);
        assert_eq!(status_from_start_type(Some(StartType::Disabled)), VanguardStatus::Disabled);
        assert_eq!(status_from_start_type(Some(StartType::Demand)), VanguardStatus::Enabled);
        assert_eq!(status_from_start_type(Some(StartType::System)), VanguardStatus::Enabled);
    }

    #[test]
    fn executable_lines_skip_comments_and_blanks() {
        let cmds = ["# comment", "", "  sc config vgc start= demand  ", "   ", "  # indented comment", "echo hi"]
            .map(String::from);
        let lines: Vec<_> = executable_lines(&cmds).collect();
        assert_eq!(lines, ["sc config vgc start= demand", "echo hi"]);
    }

    #[test]
    fn run_commands_reports_failures_but_runs_everything() {
        let cmds = ["exit 3", "exit 0", "exit 5"].map(String::from);
        let err = run_commands(&cmds).unwrap_err().to_string();
        assert!(err.contains("exit 3") && err.contains("exit 5") && !err.contains("exit 0"), "{err}");
    }

    #[test]
    fn invalid_service_name_is_unknown_not_injected() {
        assert_eq!(get_status("vgc & echo pwned"), VanguardStatus::Unknown);
    }
}
