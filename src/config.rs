//! User settings and where they're stored.
//!
//! The config holds command lines that the (elevated) app runs, so it lives
//! in an admin-only folder (`%ProgramData%\VG_Switcher`, see
//! `platform::security`) and a file not owned by Administrators/SYSTEM is
//! never loaded.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};
use crate::platform::{security, services};
use crate::theme::Theme;

pub const DEFAULT_STATUS_SERVICE: &str = "vgc";
const FILE_NAME: &str = "config.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// Missing fields fall back to their defaults, so adding a setting never
// invalidates an older config file.
#[serde(default)]
pub struct Config {
    pub status_service: String,
    pub enable_commands: Vec<String>,
    pub disable_commands: Vec<String>,
    pub theme: Theme,
}

impl Default for Config {
    fn default() -> Self {
        let lines = |v: &[&str]| v.iter().map(ToString::to_string).collect();
        Config {
            status_service: DEFAULT_STATUS_SERVICE.to_string(),
            enable_commands: lines(&[
                "# Takes full effect after a reboot",
                "sc config vgc start= demand",
                "sc config vgk start= system",
            ]),
            disable_commands: lines(&[
                "# Takes full effect after a reboot",
                "sc config vgc start= disabled",
                "sc config vgk start= disabled",
            ]),
            theme: Theme::default(),
        }
    }
}

/// One command per line; blank lines dropped, surrounding whitespace trimmed.
pub fn parse_commands(text: &str) -> Vec<String> {
    text.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect()
}

impl Config {
    /// Parses a config file, repairing invalid values. Returns warnings about
    /// anything that had to be repaired.
    pub fn from_json(text: &str) -> serde_json::Result<(Config, Vec<String>)> {
        let mut config: Config = serde_json::from_str(text)?;
        let mut warnings = Vec::new();
        if !services::is_valid_service_name(&config.status_service) {
            warnings.push(format!(
                "The status-check service name {:?} isn't a valid service name and was reset to \"{DEFAULT_STATUS_SERVICE}\".",
                config.status_service
            ));
            config.status_service = DEFAULT_STATUS_SERVICE.to_string();
        }
        Ok((config, warnings))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("Config always serializes")
    }

    /// Sets the status-check service from user input. Empty means default.
    pub fn set_status_service(&mut self, input: &str) -> Result<()> {
        let name = input.trim();
        let name = if name.is_empty() { DEFAULT_STATUS_SERVICE } else { name };
        if !services::is_valid_service_name(name) {
            return Err(AppError::InvalidInput(format!(
                "\"{name}\" isn't a valid service name. Use only letters, digits, '_', '-' and '.'."
            )));
        }
        self.status_service = name.to_string();
        Ok(())
    }
}

/// The result of loading: the config to use, plus a message for the user
/// if something had to be repaired or set aside.
pub struct Loaded {
    pub config: Config,
    pub notice: Option<String>,
}

impl Loaded {
    fn ok(config: Config) -> Self {
        Loaded { config, notice: None }
    }
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// `dir` must already be secured with `security::ensure_secure_dir`.
    pub fn in_dir(dir: &Path) -> Self {
        ConfigStore { path: dir.join(FILE_NAME) }
    }

    /// Loads the config, migrating a pre-0.6 one from `legacy` if there's
    /// none yet. Never fails: problems fall back to defaults with a notice,
    /// and the problem file is renamed rather than overwritten.
    pub fn load(&self, legacy: Option<&Path>) -> Loaded {
        if self.path.exists() {
            return self.load_existing();
        }
        if let Some(legacy) = legacy.filter(|p| p.exists()) {
            return self.migrate(legacy);
        }
        Loaded::ok(Config::default())
    }

    fn load_existing(&self) -> Loaded {
        match security::has_trusted_owner(&self.path) {
            Ok(true) => {}
            Ok(false) => {
                return self.set_aside(
                    "untrusted",
                    "isn't owned by Administrators, so it may not have been written by VG Switcher",
                );
            }
            Err(e) => return self.set_aside("unreadable", &format!("couldn't be checked ({e})")),
        }
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) => return self.set_aside("unreadable", &format!("couldn't be read ({e})")),
        };
        match Config::from_json(&text) {
            Ok((config, warnings)) => Loaded { config, notice: (!warnings.is_empty()).then(|| warnings.join("\n\n")) },
            Err(e) => self.set_aside("broken", &format!("couldn't be parsed ({e})")),
        }
    }

    /// Imports the old per-user config once. Saved straight into the
    /// secured location; the old file is renamed so it's never read again.
    fn migrate(&self, legacy: &Path) -> Loaded {
        let loaded = match std::fs::read_to_string(legacy).map(|t| Config::from_json(&t)) {
            Ok(Ok((config, warnings))) => {
                Loaded { config, notice: (!warnings.is_empty()).then(|| warnings.join("\n\n")) }
            }
            Ok(Err(e)) => {
                log::warn!("legacy config {} couldn't be parsed: {e}", legacy.display());
                Loaded::ok(Config::default())
            }
            Err(e) => {
                log::warn!("legacy config {} couldn't be read: {e}", legacy.display());
                return Loaded::ok(Config::default());
            }
        };
        match self.save(&loaded.config) {
            Ok(()) => {
                log::info!("migrated config from {}", legacy.display());
                if let Err(e) = std::fs::rename(legacy, legacy.with_extension("json.migrated")) {
                    log::warn!("couldn't rename the legacy config: {e}");
                }
            }
            Err(e) => log::error!("saving the migrated config failed: {e}"),
        }
        loaded
    }

    /// Renames the current file out of the way and returns defaults.
    fn set_aside(&self, tag: &str, reason: &str) -> Loaded {
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let aside = self.path.with_extension(format!("json.{tag}-{stamp}"));
        let moved = std::fs::rename(&self.path, &aside);
        log::warn!("config {} {reason}; moved to {} ({moved:?})", self.path.display(), aside.display());
        let where_ = match moved {
            Ok(()) => format!("It was moved to:\n{}", aside.display()),
            Err(e) => format!("It also couldn't be moved aside ({e}), so it will be overwritten on the next save."),
        };
        Loaded {
            config: Config::default(),
            notice: Some(format!("Your settings file {reason}.\n\n{where_}\n\nVG Switcher is using default settings.")),
        }
    }

    /// Writes the config atomically: a crash mid-save leaves the previous
    /// file intact instead of a truncated one.
    pub fn save(&self, config: &Config) -> Result<()> {
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, config.to_json()).map_err(|e| AppError::io(format!("writing {}", tmp.display()), e))?;
        security::secure_file(&tmp)?;
        std::fs::rename(&tmp, &self.path).map_err(|e| AppError::io(format!("replacing {}", self.path.display()), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let mut c = Config { theme: Theme::Dracula, ..Config::default() };
        c.enable_commands.push("echo extra".into());
        let (back, warnings) = Config::from_json(&c.to_json()).unwrap();
        assert_eq!(back, c);
        assert!(warnings.is_empty());
    }

    #[test]
    fn missing_fields_take_defaults() {
        let (c, _) = Config::from_json(r#"{ "theme": "light" }"#).unwrap();
        assert_eq!(c.theme, Theme::Light);
        assert_eq!(c.status_service, DEFAULT_STATUS_SERVICE);
        assert_eq!(c.enable_commands, Config::default().enable_commands);
    }

    #[test]
    fn reads_a_0_5_config() {
        let old = r#"{ "status_service": "vgc", "enable_commands": ["a"], "disable_commands": ["b"] }"#;
        let (c, _) = Config::from_json(old).unwrap();
        assert_eq!(c.enable_commands, ["a"]);
        assert_eq!(c.theme, Theme::Dark);
    }

    #[test]
    fn unknown_theme_falls_back_to_dark() {
        let (c, _) = Config::from_json(r#"{ "theme": "neon" }"#).unwrap();
        assert_eq!(c.theme, Theme::Dark);
    }

    #[test]
    fn injected_service_name_is_reset_with_warning() {
        let (c, warnings) = Config::from_json(r#"{ "status_service": "vgc & calc.exe" }"#).unwrap();
        assert_eq!(c.status_service, DEFAULT_STATUS_SERVICE);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn set_status_service_validates() {
        let mut c = Config::default();
        assert!(c.set_status_service("  vgk ").is_ok());
        assert_eq!(c.status_service, "vgk");
        assert!(c.set_status_service("").is_ok());
        assert_eq!(c.status_service, DEFAULT_STATUS_SERVICE);
        assert!(c.set_status_service("vgc | x").is_err());
        assert_eq!(c.status_service, DEFAULT_STATUS_SERVICE);
    }

    #[test]
    fn parse_commands_trims_and_drops_blanks() {
        assert_eq!(parse_commands("  a  \r\n\r\n b\n   \n"), ["a", "b"]);
    }

    #[test]
    fn garbage_is_a_parse_error() {
        assert!(Config::from_json("not json").is_err());
    }
}
