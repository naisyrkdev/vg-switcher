use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The value name Riot's installer registers Vanguard's tray app under, in
/// whichever Run key it lands in.
const VALUE_NAME: &str = "Riot Vanguard";

struct Location {
    run_key: &'static str,
    approved_key: &'static str,
}

/// Everywhere a per-machine or per-user autostart entry named "Riot
/// Vanguard" could land, paired with the `StartupApproved` key Windows'
/// own Settings > Apps > Startup / Task Manager "Startup" tab reads back to
/// decide enabled/disabled for that same entry.
const LOCATIONS: &[Location] = &[
    Location {
        run_key: r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
    },
    Location {
        run_key: r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
    },
    Location {
        run_key: r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStatus {
    Enabled,
    Disabled,
    NotFound,
}

fn reg_query(key: &str, value: &str) -> Option<String> {
    let output = Command::new("reg.exe")
        .args(["query", key, "/v", value])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Finds which known Run-key location currently holds the Vanguard startup
/// entry, if any.
fn find_location() -> Option<&'static Location> {
    LOCATIONS.iter().find(|loc| reg_query(loc.run_key, VALUE_NAME).is_some())
}

/// Extracts the first byte of a REG_BINARY value's hex data from `reg
/// query`'s output, e.g. a line like
/// "    Riot Vanguard    REG_BINARY    03000000A13D70352318DB01" -> `0x03`.
/// That leading byte is what Windows itself uses to mark the entry disabled
/// (`0x02`) vs enabled (anything else) in its Startup Apps UI.
fn first_binary_byte(query_output: &str) -> Option<u8> {
    for line in query_output.lines() {
        let trimmed = line.trim();
        let Some(idx) = trimmed.find("REG_BINARY") else {
            continue;
        };
        let hex = trimmed[idx + "REG_BINARY".len()..].trim();
        if hex.len() >= 2 {
            return u8::from_str_radix(&hex[0..2], 16).ok();
        }
    }
    None
}

/// Current Vanguard startup status, read the same way Windows' own Startup
/// Apps page / Task Manager "Startup" tab would show it: present in a Run
/// key, and not flagged disabled by the matching `StartupApproved` override.
pub fn status() -> StartupStatus {
    let Some(loc) = find_location() else {
        return StartupStatus::NotFound;
    };
    match reg_query(loc.approved_key, VALUE_NAME).and_then(|out| first_binary_byte(&out)) {
        Some(0x02) => StartupStatus::Disabled,
        // No override recorded at all, or a byte other than 0x02, both mean
        // enabled - an un-overridden Run entry runs at logon by default.
        _ => StartupStatus::Enabled,
    }
}

/// Enables or disables Vanguard's startup entry without touching the Run
/// key itself - the same thing toggling it in Windows' Settings > Apps >
/// Startup (or Task Manager's Startup tab) does under the hood.
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    let loc = find_location().ok_or_else(|| {
        "No \"Riot Vanguard\" startup entry was found in any of the Windows Run keys, so \
         there's nothing to enable or disable. Vanguard may not be installed, or its autostart \
         entry was removed."
            .to_string()
    })?;

    if enabled {
        // Absence of a StartupApproved override means "enabled" - just
        // clear any disabled flag previously written, ignoring the case
        // where there was none to clear.
        let _ = Command::new("reg.exe")
            .args(["delete", loc.approved_key, "/v", VALUE_NAME, "/f"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        Ok(())
    } else {
        let output = Command::new("reg.exe")
            .args([
                "add",
                loc.approved_key,
                "/v",
                VALUE_NAME,
                "/t",
                "REG_BINARY",
                "/d",
                "020000000000000000000000",
                "/f",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("failed to run reg.exe: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
        }
    }
}
