use crate::platform::process::{is_running, kill_process};

/// Riot Client's own process plus the helper processes it spawns. Killing
/// only `RiotClientServices.exe` can leave one of these lingering (e.g. a
/// stuck UI render process after a crash), which is exactly the kind of
/// "nothing happens when I click it again" state this utility exists to
/// clear out - so every name here is tried, not just the main one.
const RIOT_CLIENT_PROCESSES: &[&str] = &[
    "RiotClientServices.exe",
    "RiotClientUx.exe",
    "RiotClientUxRender.exe",
    "RiotClientCrashHandler.exe",
];

/// Force-kills every running Riot Client process. Returns a human-readable
/// summary on success (including the case where nothing was running), or a
/// detailed per-process error report if any kill failed.
pub fn force_kill_client() -> Result<String, String> {
    let mut killed = Vec::new();
    let mut failed = Vec::new();

    for &name in RIOT_CLIENT_PROCESSES {
        if !is_running(name) {
            continue;
        }
        match kill_process(name) {
            Ok(()) => killed.push(name),
            Err(e) => failed.push(format!("{name}:\n  -> {}", e.trim())),
        }
    }

    if !failed.is_empty() {
        let mut msg = String::from("Could not fully force-close the Riot Client.\n\n");
        msg.push_str(&failed.join("\n\n"));
        if !killed.is_empty() {
            msg.push_str(&format!("\n\nAlready closed successfully: {}", killed.join(", ")));
        }
        msg.push_str(
            "\n\nThis usually means the process is protected, already exiting, or VG Switcher \
             isn't running with the permissions needed to terminate it. Try again in a few \
             seconds, or close it manually from Task Manager.",
        );
        return Err(msg);
    }

    if killed.is_empty() {
        Ok("Riot Client isn't running - nothing to close.".to_string())
    } else {
        Ok(format!("Force-closed: {}", killed.join(", ")))
    }
}
