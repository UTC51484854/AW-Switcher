use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSource {
    pub name: String,
    /// DDC/CI VCP 0x60 input source value, e.g. 0x11 for HDMI-1.
    pub code: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Global hotkey that cycles to the next input in `inputs`.
    /// Parsed by the `global-hotkey` crate, e.g. "CmdOrCtrl+Alt+I".
    pub hotkey: String,
    /// Case-insensitive substring matched against a display's model name,
    /// used to pick the right monitor when more than one is connected.
    pub monitor_match: String,
    /// Inputs to cycle through, in order, when the hotkey is pressed.
    pub inputs: Vec<InputSource>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "CmdOrCtrl+Alt+I".into(),
            monitor_match: "AW3926".into(),
            inputs: vec![
                InputSource { name: "HDMI 1".into(), code: 0x11 },
                InputSource { name: "HDMI 2".into(), code: 0x12 },
                InputSource { name: "DisplayPort 1".into(), code: 0x0f },
                InputSource { name: "DisplayPort 2".into(), code: 0x10 },
            ],
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .context("Could not determine a config directory for this platform")?
            .join("aw-switcher");
        Ok(dir.join("config.toml"))
    }

    /// Loads the config, creating a default one on disk the first time this runs.
    pub fn load_or_create() -> Result<Self> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        }

        if !path.exists() {
            let default = Config::default();
            fs::write(&path, toml::to_string_pretty(&default)?)
                .with_context(|| format!("Failed to write default config to {}", path.display()))?;
            return Ok(default);
        }

        let text = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config at {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("Failed to parse config at {}", path.display()))
    }
}
