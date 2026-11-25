//! How each status is shown: its text and its colour.

use crate::platform::startup_apps::StartupStatus;
use crate::theme::{self, Rgb};
use crate::vanguard::VanguardStatus;

pub trait StatusView {
    fn label(&self) -> &'static str;
    fn color(&self) -> Rgb;
}

impl StatusView for VanguardStatus {
    fn label(&self) -> &'static str {
        match self {
            VanguardStatus::Enabled => "ENABLED",
            VanguardStatus::Disabled => "DISABLED",
            VanguardStatus::NotInstalled => "NOT INSTALLED",
            VanguardStatus::Unknown => "UNKNOWN",
        }
    }

    fn color(&self) -> Rgb {
        match self {
            VanguardStatus::Enabled => theme::success(),
            VanguardStatus::Disabled => theme::error(),
            VanguardStatus::NotInstalled | VanguardStatus::Unknown => theme::neutral(),
        }
    }
}

/// Whether `vgtray.exe` is running right now.
pub struct TrayProcess(pub bool);

impl StatusView for TrayProcess {
    fn label(&self) -> &'static str {
        if self.0 { "ENABLED" } else { "DISABLED" }
    }

    fn color(&self) -> Rgb {
        if self.0 { theme::success() } else { theme::error() }
    }
}

impl StatusView for StartupStatus {
    fn label(&self) -> &'static str {
        match self {
            StartupStatus::Enabled => "ENABLED",
            StartupStatus::Disabled => "DISABLED",
            StartupStatus::NotFound => "NOT FOUND",
        }
    }

    fn color(&self) -> Rgb {
        match self {
            StartupStatus::Enabled => theme::success(),
            StartupStatus::Disabled => theme::error(),
            StartupStatus::NotFound => theme::neutral(),
        }
    }
}

/// Tray tooltip / menu text, where there's no "Current status:" prefix.
pub fn tray_text(status: VanguardStatus) -> String {
    format!("Vanguard: {}", status.label())
}
