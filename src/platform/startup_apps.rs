//! Vanguard's entry in Windows' "Startup Apps" list (Settings > Apps >
//! Startup, Task Manager > Startup).
//!
//! Windows keeps the Run-key entry itself untouched and records the user's
//! choice in a matching `StartupApproved` value: 12 bytes, where an **even**
//! first byte (0x02, 0x06) means enabled and an **odd** one (0x03, 0x07,
//! 0x01) means disabled. Disabled entries also carry the FILETIME at which
//! they were disabled in bytes 4..12.

use winapi::shared::minwindef::FILETIME;
use winapi::um::sysinfoapi::GetSystemTimeAsFileTime;

use crate::error::{AppError, Result};
use crate::platform::registry::{self, Hive};

/// The value name Riot's installer registers Vanguard's tray app under.
const VALUE_NAME: &str = "Riot Vanguard";

struct Location {
    hive: Hive,
    run_key: &'static str,
    approved_key: &'static str,
}

/// Every Run key a "Riot Vanguard" entry could be in, paired with the
/// `StartupApproved` key Windows reads for that same entry.
const LOCATIONS: &[Location] = &[
    Location {
        hive: Hive::LocalMachine,
        run_key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
    },
    Location {
        hive: Hive::LocalMachine,
        run_key: r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32",
    },
    Location {
        hive: Hive::CurrentUser,
        run_key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
        approved_key: r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run",
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupStatus {
    Enabled,
    Disabled,
    NotFound,
}

fn find_location() -> Option<&'static Location> {
    LOCATIONS.iter().find(|loc| registry::value_exists(loc.hive, loc.run_key, VALUE_NAME).unwrap_or(false))
}

/// Decodes a `StartupApproved` value. No value at all means enabled: an
/// un-overridden Run entry runs at logon.
pub fn decode_approved(value: Option<&[u8]>) -> StartupStatus {
    match value.and_then(<[u8]>::first) {
        Some(b) if b & 1 == 1 => StartupStatus::Disabled,
        _ => StartupStatus::Enabled,
    }
}

/// Encodes a `StartupApproved` value the way Task Manager writes it.
pub fn encode_approved(enabled: bool, now_filetime: u64) -> [u8; 12] {
    let mut value = [0u8; 12];
    if enabled {
        value[0] = 0x02;
    } else {
        value[0] = 0x03;
        value[4..].copy_from_slice(&now_filetime.to_le_bytes());
    }
    value
}

fn now_filetime() -> u64 {
    let mut ft = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    unsafe { GetSystemTimeAsFileTime(&mut ft) };
    (u64::from(ft.dwHighDateTime) << 32) | u64::from(ft.dwLowDateTime)
}

pub fn status() -> StartupStatus {
    let Some(loc) = find_location() else {
        return StartupStatus::NotFound;
    };
    match registry::read_binary(loc.hive, loc.approved_key, VALUE_NAME) {
        Ok(value) => decode_approved(value.as_deref()),
        Err(e) => {
            log::warn!("reading Vanguard's StartupApproved value failed: {e}");
            StartupStatus::Enabled
        }
    }
}

/// Enables or disables Vanguard's startup entry without touching the Run
/// key, the same way toggling it in Task Manager does.
pub fn set_enabled(enabled: bool) -> Result<()> {
    let loc = find_location().ok_or_else(|| {
        AppError::NotFound(
            "No \"Riot Vanguard\" startup entry was found in any of the Windows Run keys, so there's \
             nothing to enable or disable. Vanguard may not be installed, or its autostart entry was removed."
                .to_string(),
        )
    })?;
    registry::write_binary(loc.hive, loc.approved_key, VALUE_NAME, &encode_approved(enabled, now_filetime()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_values_seen_in_the_wild() {
        let hex = |s: &str| {
            (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<_>>()
        };
        assert_eq!(decode_approved(Some(&hex("020000000000000000000000"))), StartupStatus::Enabled);
        assert_eq!(decode_approved(Some(&hex("060000000000000000000000"))), StartupStatus::Enabled);
        assert_eq!(decode_approved(Some(&hex("03000000A13D70352318DB01"))), StartupStatus::Disabled);
        assert_eq!(decode_approved(Some(&hex("010000003509C9DDF977DD01"))), StartupStatus::Disabled);
    }

    #[test]
    fn missing_or_empty_value_means_enabled() {
        assert_eq!(decode_approved(None), StartupStatus::Enabled);
        assert_eq!(decode_approved(Some(&[])), StartupStatus::Enabled);
    }

    #[test]
    fn encode_round_trips_through_decode() {
        for enabled in [true, false] {
            let v = encode_approved(enabled, 0x01DB_1823_3570_3DA1);
            let expected = if enabled { StartupStatus::Enabled } else { StartupStatus::Disabled };
            assert_eq!(decode_approved(Some(&v)), expected);
        }
    }

    #[test]
    fn disabled_value_carries_the_timestamp() {
        let v = encode_approved(false, 0x0102_0304_0506_0708);
        assert_eq!(&v[..4], &[3, 0, 0, 0]);
        assert_eq!(&v[4..], &0x0102_0304_0506_0708u64.to_le_bytes());
        assert_eq!(encode_approved(true, 123), [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
}
