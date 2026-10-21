//! User config: `<config>/turly/config.toml`.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn config_dir() -> PathBuf {
    std::env::var_os("TURLY_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(|p| PathBuf::from(p).join("turly")))
        .or_else(|| dirs::home_dir().map(|h| h.join(".config").join("turly")))
        .unwrap_or_else(|| PathBuf::from(".turly-config"))
}

/// Default home for new workspaces: `$XDG_DATA_HOME/turly/workspaces` or `~/.local/share/turly/workspaces`.
pub fn default_workspaces_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from(".turly-data"))
        .join("turly")
        .join("workspaces")
}

/// Expand a leading `~/`.
pub fn expand(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(p),
    }
}

#[derive(Deserialize, Default, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub theme: Option<String>,
    /// Mouse support (default on); off keeps native terminal text selection.
    pub mouse: Option<bool>,
    /// Folder holding all your workspaces; opened when the launch folder has none.
    pub base_dir: Option<String>,
    /// Extra or overriding key bindings, e.g. `"ctrl-s" = "save"`.
    #[serde(default)]
    pub keys: BTreeMap<String, String>,
}

impl Config {
    pub fn base(&self) -> Option<PathBuf> {
        self.base_dir.as_deref().map(expand)
    }

    /// Set `base_dir` in config.toml, keeping the rest of the file as is.
    pub fn save_base_dir(dir: &Path) -> Result<()> {
        let p = config_dir().join("config.toml");
        let line = format!(
            "base_dir = {}",
            toml::Value::String(dir.display().to_string())
        );
        let old = std::fs::read_to_string(&p).unwrap_or_default();
        let mut done = false;
        let mut lines: Vec<String> = old
            .lines()
            .map(|l| {
                if l.trim_start().starts_with("base_dir") && l.contains('=') {
                    done = true;
                    line.clone()
                } else {
                    l.to_string()
                }
            })
            .collect();
        if !done {
            lines.insert(0, line);
        }
        std::fs::create_dir_all(config_dir())?;
        std::fs::write(&p, lines.join("\n") + "\n")?;
        Ok(())
    }

    pub fn load() -> Result<Config> {
        let p = config_dir().join("config.toml");
        if !p.exists() {
            return Ok(Config::default());
        }
        let s = std::fs::read_to_string(&p)?;
        toml::from_str(&s).with_context(|| format!("parsing {}", p.display()))
    }
}
