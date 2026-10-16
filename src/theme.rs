//! Color themes: built-in presets plus user TOML files in `<config>/turly/themes/`.

use anyhow::{Result, bail};
use ratatui::style::Color;

fn parse_color(s: &str) -> Result<Color> {
    if s == "reset" {
        return Ok(Color::Reset);
    }
    let h = s.strip_prefix('#').unwrap_or("");
    if h.len() != 6 {
        bail!("bad color '{s}' (want #rrggbb or 'reset')");
    }
    let n = u32::from_str_radix(h, 16).map_err(|_| anyhow::anyhow!("bad color '{s}'"))?;
    Ok(Color::Rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

macro_rules! slots {
    ($($f:ident),*) => {
        #[derive(Clone, Debug)]
        pub struct Theme {
            pub name: String,
            $(pub $f: Color,)*
        }

        impl Theme {
            fn from_hex(name: &str, hex: &[&str]) -> Theme {
                let mut it = hex.iter();
                Theme { name: name.into(), $($f: parse_color(it.next().unwrap()).expect("preset color"),)* }
            }

            fn set(&mut self, slot: &str, c: Color) -> bool {
                match slot { $(stringify!($f) => { self.$f = c; true })* _ => false }
            }

            fn map(&mut self, f: impl Fn(Color) -> Color) {
                $(self.$f = f(self.$f);)*
            }
        }
    };
}

slots!(
    bg, fg, dim, accent, border, selection, search, ok, warn, err, info, key, string, number,
    keyword, comment
);

// Slot order: bg fg dim accent border selection search ok warn err info key string number keyword comment
const PRESETS: &[(&str, [&str; 16])] = &[
    (
        "turly",
        [
            "#1b1815", "#e4ded5", "#7d766d", "#e8893a", "#4a443d", "#3a2c1f", "#7a4a14", "#6cc08b",
            "#e0b04a", "#ef7b72", "#6fb0e0", "#6fb0e0", "#6cc08b", "#e0b04a", "#e8893a", "#7d766d",
        ],
    ),
    (
        "turly-light",
        [
            "#fbfaf8", "#2a2724", "#8a847c", "#b8570f", "#b9b3aa", "#ead9c6", "#f2c48f", "#2f7d4f",
            "#a06a00", "#b3332b", "#2a6a9a", "#2a6a9a", "#2f7d4f", "#a06a00", "#b8570f", "#8a847c",
        ],
    ),
    (
        "gruvbox",
        [
            "#282828", "#ebdbb2", "#928374", "#fe8019", "#504945", "#3c3836", "#b57614", "#b8bb26",
            "#fabd2f", "#fb4934", "#83a598", "#83a598", "#b8bb26", "#d3869b", "#fe8019", "#928374",
        ],
    ),
    (
        "nord",
        [
            "#2e3440", "#d8dee9", "#6c7a96", "#88c0d0", "#4c566a", "#3b4252", "#5e81ac", "#a3be8c",
            "#ebcb8b", "#bf616a", "#81a1c1", "#81a1c1", "#a3be8c", "#b48ead", "#88c0d0", "#616e88",
        ],
    ),
    (
        "catppuccin",
        [
            "#1e1e2e", "#cdd6f4", "#6c7086", "#cba6f7", "#45475a", "#313244", "#585b70", "#a6e3a1",
            "#f9e2af", "#f38ba8", "#89b4fa", "#89b4fa", "#a6e3a1", "#fab387", "#cba6f7", "#6c7086",
        ],
    ),
    (
        "catppuccin-latte",
        [
            "#eff1f5", "#4c4f69", "#9ca0b0", "#8839ef", "#bcc0cc", "#ccd0da", "#acb0be", "#40a02b",
            "#df8e1d", "#d20f39", "#1e66f5", "#1e66f5", "#40a02b", "#fe640b", "#8839ef", "#9ca0b0",
        ],
    ),
    (
        "solarized",
        [
            "#002b36", "#93a1a1", "#586e75", "#b58900", "#586e75", "#073642", "#657b83", "#859900",
            "#b58900", "#dc322f", "#268bd2", "#268bd2", "#2aa198", "#d33682", "#859900", "#586e75",
        ],
    ),
    (
        "solarized-light",
        [
            "#fdf6e3", "#657b83", "#93a1a1", "#b58900", "#93a1a1", "#eee8d5", "#e0d7b0", "#859900",
            "#b58900", "#dc322f", "#268bd2", "#268bd2", "#2aa198", "#d33682", "#859900", "#93a1a1",
        ],
    ),
    (
        "tokyonight",
        [
            "#1a1b26", "#c0caf5", "#565f89", "#7aa2f7", "#3b4261", "#292e42", "#3d59a1", "#9ece6a",
            "#e0af68", "#f7768e", "#7dcfff", "#7aa2f7", "#9ece6a", "#ff9e64", "#bb9af7", "#565f89",
        ],
    ),
    (
        "dracula",
        [
            "#282a36", "#f8f8f2", "#6272a4", "#bd93f9", "#44475a", "#44475a", "#6272a4", "#50fa7b",
            "#f1fa8c", "#ff5555", "#8be9fd", "#8be9fd", "#f1fa8c", "#bd93f9", "#ff79c6", "#6272a4",
        ],
    ),
    (
        "mono",
        [
            "reset", "reset", "#808080", "reset", "#808080", "#404040", "#606060", "reset",
            "reset", "reset", "reset", "reset", "reset", "reset", "reset", "#808080",
        ],
    ),
];

impl Theme {
    pub fn preset_names() -> Vec<String> {
        PRESETS.iter().map(|(n, _)| n.to_string()).collect()
    }

    pub fn preset(name: &str) -> Option<Theme> {
        PRESETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(n, h)| Theme::from_hex(n, h))
    }

    /// Preset by name, else `<config>/themes/<name>.toml` (optional `base = "<preset>"`, then slot overrides).
    pub fn load(name: &str) -> Result<Theme> {
        if let Some(t) = Theme::preset(name) {
            return Ok(t);
        }
        let p = crate::config::config_dir()
            .join("themes")
            .join(format!("{name}.toml"));
        let Ok(text) = std::fs::read_to_string(&p) else {
            bail!("unknown theme '{name}'")
        };
        Theme::from_toml(name, &text)
    }

    pub fn from_toml(name: &str, text: &str) -> Result<Theme> {
        let table: toml::Table = text.parse()?;
        let base = table
            .get("base")
            .and_then(|v| v.as_str())
            .unwrap_or("turly");
        let mut t =
            Theme::preset(base).ok_or_else(|| anyhow::anyhow!("unknown base theme '{base}'"))?;
        t.name = name.into();
        for (k, v) in &table {
            if k == "base" {
                continue;
            }
            let Some(s) = v.as_str() else {
                bail!("theme slot '{k}' must be a string")
            };
            if !t.set(k, parse_color(s)?) {
                bail!("unknown theme slot '{k}'");
            }
        }
        Ok(t)
    }

    /// All selectable names: presets plus user theme files.
    pub fn all_names() -> Vec<String> {
        let mut v = Theme::preset_names();
        if let Ok(rd) = std::fs::read_dir(crate::config::config_dir().join("themes")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "toml") {
                    let n = p.file_stem().unwrap().to_string_lossy().into_owned();
                    if !v.contains(&n) {
                        v.push(n);
                    }
                }
            }
        }
        v
    }

    /// Default preset following the terminal background (via `COLORFGBG`, dark if unknown).
    pub fn default_name() -> &'static str {
        let light = std::env::var("COLORFGBG")
            .ok()
            .and_then(|v| v.rsplit(';').next().and_then(|s| s.parse::<u8>().ok()))
            .is_some_and(|bg| bg == 7 || bg == 15);
        if light { "turly-light" } else { "turly" }
    }

    /// Convert RGB to the 256-color cube when the terminal lacks truecolor.
    pub fn downgrade_if_needed(&mut self) {
        let ct = std::env::var("COLORTERM").unwrap_or_default();
        if ct.contains("truecolor") || ct.contains("24bit") {
            return;
        }
        self.map(|c| match c {
            Color::Rgb(r, g, b) => Color::Indexed(rgb_to_256(r, g, b)),
            c => c,
        });
    }
}

fn rgb_to_256(r: u8, g: u8, b: u8) -> u8 {
    let q = |v: u8| ((v as u16 * 5 + 127) / 255) as u8;
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_parse() {
        for n in Theme::preset_names() {
            assert!(Theme::preset(&n).is_some());
        }
    }

    #[test]
    fn user_theme_overrides_base() {
        let t = Theme::from_toml("mine", "base = \"nord\"\naccent = \"#ff0000\"").unwrap();
        assert_eq!(t.accent, Color::Rgb(255, 0, 0));
        assert_eq!(t.bg, Theme::preset("nord").unwrap().bg);
        assert!(Theme::from_toml("x", "bogus = \"#000000\"").is_err());
    }

    #[test]
    fn downgrade_maps_to_cube() {
        assert_eq!(rgb_to_256(255, 0, 0), 196);
    }
}
