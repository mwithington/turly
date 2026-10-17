//! User config: `<config>/turly/config.toml`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    std::env::var_os("TURLY_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("turly")))
        .or_else(|| dirs::home_dir().map(|h| h.join(".config").join("turly")))
        .unwrap_or_else(|| PathBuf::from(".turly-config"))
}

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub theme: Option<String>,
    /// Mouse support (default on); off keeps native terminal text selection.
    pub mouse: Option<bool>,
    /// Extra or overriding key bindings, e.g. `"ctrl-s" = "save"`.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

impl Config {
    pub fn load() -> Result<Config> {
        let p = config_dir().join("config.toml");
        if !p.exists() {
            return Ok(Config::default());
        }
        let s = std::fs::read_to_string(&p)?;
        toml::from_str(&s).with_context(|| format!("parsing {}", p.display()))
    }
}
