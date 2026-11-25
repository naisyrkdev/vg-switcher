use crate::platform::process::{is_running, kill_process, run_line};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VanguardStatus {
    Enabled,
    Disabled,
    NotInstalled,
}

/// Image name of Riot's Vanguard tray app. `disable_commands` only flips the
/// services' start types for next boot, so a tray instance launched earlier
/// in the session lingers until reboot unless this kills it directly.
const TRAY_PROCESS: &str = "vgtray.exe";

/// Best-effort kill of the lingering Vanguard tray process. Not finding it
/// running is not an error - this is meant to be called defensively, both
/// right after disabling and at startup when Vanguard is already disabled.
pub fn kill_tray() -> Result<(), String> {
    kill_process(TRAY_PROCESS)
}

/// Whether the Vanguard tray process is currently running, independent of
/// the services' configured start type.
pub fn tray_running() -> bool {
    is_running(TRAY_PROCESS)
}

/// Runs every non-empty, non-comment line in `commands`, continuing past
/// failures (some lines, like the vgk driver tweak, are best-effort) and
/// returning a combined error if anything failed.
pub fn run_commands(commands: &[String]) -> Result<(), String> {
    let mut errors = Vec::new();
    for line in commands {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        match run_line(line) {
            Ok((true, _)) => {}
            Ok((false, out)) => errors.push(format!("{line}\n  -> {}", out.trim())),
            Err(e) => errors.push(format!("{line}\n  -> {e}")),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n\n"))
    }
}

/// Queries a Windows service's start type via `sc qc <service>`, the same
/// thing `vanguard status` would be checked with manually.
pub fn get_status(service: &str) -> VanguardStatus {
    let service = service.trim();
    if service.is_empty() {
        return VanguardStatus::NotInstalled;
    }
    match run_line(&format!("sc qc {service}")) {
        Ok((_, out)) => {
            if out.contains("does not exist") || out.contains("1060") {
                return VanguardStatus::NotInstalled;
            }
            for line in out.lines() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("START_TYPE") {
                    return if trimmed.contains("DISABLED") {
                        VanguardStatus::Disabled
                    } else {
                        VanguardStatus::Enabled
                    };
                }
            }
            VanguardStatus::NotInstalled
        }
        Err(_) => VanguardStatus::NotInstalled,
    }
}
