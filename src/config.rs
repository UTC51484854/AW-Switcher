use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputSource {
    pub name: String,
    /// DDC/CI VCP 0x60 input source value, e.g. 0x11 for HDMI-1.
    pub code: u16,
    /// Whether the hotkey's cycle includes this input. Disabled inputs are
    /// skipped by the hotkey but can still be switched to from the tray menu.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn default_open_at_login() -> bool {
    true
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
    /// Whether the app registers itself to launch at login. Defaults to on;
    /// toggled via the tray menu's "Open at Login" checkbox.
    #[serde(default = "default_open_at_login")]
    pub open_at_login: bool,
    /// Inputs to cycle through, in order, when the hotkey is pressed.
    pub inputs: Vec<InputSource>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            hotkey: "CmdOrCtrl+Alt+I".into(),
            monitor_match: "AW3926".into(),
            open_at_login: true,
            // The AW3926QW has 2x HDMI, 1x DisplayPort 2.1, and 1x USB-C
            // (10Gbps upstream, DisplayPort 2.1 Alt Mode) — no second
            // DisplayPort input.
            inputs: vec![
                InputSource { name: "HDMI 1".into(), code: 0x11, enabled: true },
                InputSource { name: "HDMI 2".into(), code: 0x12, enabled: true },
                InputSource { name: "DisplayPort".into(), code: 0x0f, enabled: true },
                // Dell's vendor-specific code for "DisplayPort over USB-C" on
                // other Dell/Alienware monitors (e.g. the U3818DW). Not yet
                // confirmed against the AW3926QW specifically — if switching
                // to it does nothing, check `ddcutil capabilities` and adjust.
                InputSource { name: "USB-C".into(), code: 0x1b, enabled: true },
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

    /// Writes this config to disk, creating the config directory if needed.
    pub fn save(&self) -> Result<()> {
        let path = Self::path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory {}", parent.display()))?;
        }
        fs::write(&path, toml::to_string_pretty(self)?)
            .with_context(|| format!("Failed to write config to {}", path.display()))
    }
}
