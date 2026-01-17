use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::stats::{self, StatId};

#[derive(Deserialize)]
pub struct Config {
    #[serde(default)]
    pub display: DisplayConfig,
}

#[derive(Deserialize)]
pub struct DisplayConfig {
    /// Which stats to display, in order.
    #[serde(default = "stats::default_stats")]
    pub stats: Vec<StatId>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            display: DisplayConfig::default(),
        }
    }
}

impl Default for DisplayConfig {
    fn default() -> Self {
        DisplayConfig {
            stats: stats::default_stats(),
        }
    }
}

impl Config {
    /// Returns the path to the config file (~/.config/pacfetch.toml).
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("pacfetch.toml"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Config::default();
        };

        let Ok(contents) = fs::read_to_string(&path) else {
            return Config::default();
        };

        toml::from_str(&contents).unwrap_or_default()
    }
}
