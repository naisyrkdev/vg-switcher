use crate::error::{AppError, Result};
use crate::platform::{cmd, paths};

/// Restarts Windows after a 5-second grace period, using the system's own
/// shutdown notice. The caller must have confirmed with the user first.
pub fn reboot_now() -> Result<()> {
    let out = cmd::run(&paths::system_tool("shutdown.exe"), &["/r", "/t", "5", "/c", "Restarting from VG Switcher"])?;
    if out.success() { Ok(()) } else { Err(AppError::CommandFailed(out.combined())) }
}
