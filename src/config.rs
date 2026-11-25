use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub status_service: String,
    pub enable_commands: Vec<String>,
    pub disable_commands: Vec<String>,
    #[serde(default = "default_theme")]
    pub theme: String,
}

fn default_theme() -> String {
    "dark".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Config {
            status_service: "vgc".to_string(),
            enable_commands: vec![
                "# Takes full effect after a reboot".to_string(),
                "sc config vgc start= demand".to_string(),
                "sc config vgk start= system".to_string(),
            ],
            disable_commands: vec![
                "# Takes full effect after a reboot".to_string(),
                "sc config vgc start= disabled".to_string(),
                "sc config vgk start= disabled".to_string(),
            ],
            theme: default_theme(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("VG_Switcher").join("config.json")
}

impl Config {
    pub fn load() -> Config {
        let path = config_path();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let mut file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        Ok(())
    }
}
