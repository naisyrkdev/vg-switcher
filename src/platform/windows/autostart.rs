use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TASK_NAME: &str = "VG_Switcher";

/// Flag the scheduled task launches with, so the app knows to start hidden
/// in the tray instead of popping the settings window open.
pub const TRAY_ARG: &str = "--tray";

/// We use a Task Scheduler entry (run at logon, highest privileges) instead
/// of the HKCU "Run" registry key: a Run-key entry pointing at an
/// admin-manifested exe still triggers a UAC prompt on every login, while a
/// scheduled task set to run with the highest privileges is launched
/// silently by the Task Scheduler service.
pub fn is_enabled() -> bool {
    Command::new("schtasks.exe")
        .args(["/query", "/tn", TASK_NAME])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub fn enable() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_str = exe.to_string_lossy().to_string();
    let run_arg = format!("\"{exe_str}\" {TRAY_ARG}");

    let output = Command::new("schtasks.exe")
        .args([
            "/create", "/tn", TASK_NAME, "/tr", &run_arg, "/sc", "onlogon", "/rl", "highest", "/f",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run schtasks: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn disable() -> Result<(), String> {
    let output = Command::new("schtasks.exe")
        .args(["/delete", "/tn", TASK_NAME, "/f"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run schtasks: {e}"))?;

    if output.status.success() || !is_enabled() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}
