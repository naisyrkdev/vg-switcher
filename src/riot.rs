//! Riot Client: force-closing it and its helper processes.

use crate::platform::processes;

/// Riot Client's own process plus the helpers it spawns. Killing only
/// `RiotClientServices.exe` can leave one of these behind (e.g. a stuck UI
/// render process after a crash), which is exactly the state this utility
/// exists to clear, so every name is tried.
const RIOT_CLIENT_PROCESSES: &[&str] =
    &["RiotClientServices.exe", "RiotClientUx.exe", "RiotClientUxRender.exe", "RiotClientCrashHandler.exe"];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct KillReport {
    pub killed: Vec<&'static str>,
    pub failed: Vec<(&'static str, String)>,
}

impl KillReport {
    pub fn is_success(&self) -> bool {
        self.failed.is_empty()
    }

    /// The text shown to the user for this outcome.
    pub fn message(&self) -> String {
        if self.is_success() {
            return if self.killed.is_empty() {
                "Riot Client isn't running - nothing to close.".to_string()
            } else {
                format!("Force-closed: {}", self.killed.join(", "))
            };
        }
        let mut msg = String::from("Could not fully force-close the Riot Client.\n\n");
        let failed: Vec<String> = self.failed.iter().map(|(name, e)| format!("{name}:\n  -> {}", e.trim())).collect();
        msg.push_str(&failed.join("\n\n"));
        if !self.killed.is_empty() {
            msg.push_str("\n\nAlready closed successfully: ");
            msg.push_str(&self.killed.join(", "));
        }
        msg.push_str(
            "\n\nThis usually means the process is protected or already exiting. Try again in a few \
             seconds, or close it manually from Task Manager.",
        );
        msg
    }
}

/// Force-kills every running Riot Client process.
pub fn force_kill_client() -> KillReport {
    let mut report = KillReport::default();
    for &name in RIOT_CLIENT_PROCESSES {
        match processes::kill_all(name) {
            Ok(0) => {}
            Ok(_) => report.killed.push(name),
            Err(e) => report.failed.push((name, e.to_string())),
        }
    }
    log::info!("force kill Riot Client: {report:?}");
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_running_message() {
        assert_eq!(KillReport::default().message(), "Riot Client isn't running - nothing to close.");
    }

    #[test]
    fn success_lists_killed_processes() {
        let r = KillReport { killed: vec!["A.exe", "B.exe"], failed: vec![] };
        assert!(r.is_success());
        assert_eq!(r.message(), "Force-closed: A.exe, B.exe");
    }

    #[test]
    fn failure_lists_both_failed_and_killed() {
        let r = KillReport { killed: vec!["A.exe"], failed: vec![("B.exe", " Access is denied. ".into())] };
        assert!(!r.is_success());
        let m = r.message();
        assert!(m.contains("B.exe:\n  -> Access is denied."), "{m}");
        assert!(m.contains("Already closed successfully: A.exe"), "{m}");
    }
}
