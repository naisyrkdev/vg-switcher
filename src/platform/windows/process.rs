use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs a single command line through cmd.exe with no visible console
/// window. The OS-specific primitive the rest of the app (`vanguard`) is
/// built on - a future non-Windows backend would run the line through `sh
/// -c` here instead.
pub fn run_line(line: &str) -> Result<(bool, String), String> {
    let output = Command::new("cmd.exe")
        .args(["/C", line])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run '{line}': {e}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Ok((output.status.success(), text))
}

/// Force-kills every running process with the given image name (e.g.
/// "vgtray.exe"). The process not currently running is treated as success -
/// callers use this defensively and don't care whether there was anything
/// to kill.
pub fn kill_process(image_name: &str) -> Result<(), String> {
    match run_line(&format!("taskkill /F /IM {image_name}")) {
        Ok((true, _)) => Ok(()),
        Ok((false, out)) => {
            if out.contains("not found") {
                Ok(())
            } else {
                Err(out.trim().to_string())
            }
        }
        Err(e) => Err(e),
    }
}

/// Schedules an immediate OS restart, giving Windows' own shutdown dialog
/// grace period (a few seconds) rather than tearing down the session
/// instantly - the caller is expected to have already confirmed this with
/// the user, since it isn't reversible.
pub fn reboot_now() -> Result<(), String> {
    let output = Command::new("shutdown.exe")
        .args(["/r", "/t", "5", "/c", "Restarting from VG Switcher"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("failed to run shutdown.exe: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let mut text = String::from_utf8_lossy(&output.stdout).to_string();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Err(text.trim().to_string())
    }
}

/// Whether any process with the given image name is currently running.
///
/// Runs `tasklist.exe` directly rather than through `run_line` - the filter
/// value needs its own quoted argument, and handing a pre-quoted string like
/// `"IMAGENAME eq foo.exe"` to `cmd.exe /C` through `Command::args` mangles
/// it (Rust escapes the embedded quotes for the C-runtime argv convention,
/// which isn't how cmd.exe's own `/C` parsing works), so tasklist actually
/// errors out on an unrecognized `/FI` value instead of filtering anything.
pub fn is_running(image_name: &str) -> bool {
    let filter = format!("IMAGENAME eq {image_name}");
    match Command::new("tasklist")
        .args(["/FI", &filter, "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
    {
        Ok(output) => {
            String::from_utf8_lossy(&output.stdout).to_lowercase().contains(&image_name.to_lowercase())
        }
        Err(_) => false,
    }
}
