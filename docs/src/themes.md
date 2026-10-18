# Themes

Presets: `turly` (dark), `turly-light`, `gruvbox`, `nord`, `catppuccin`, `catppuccin-latte`, `solarized`, `solarized-light`, `tokyonight`, `dracula` and `mono`.

Switch with `:theme <name>`, or set `theme` in the config. The default follows your terminal background when it reports one (`COLORFGBG`), and truecolor falls back to 256 colors when `COLORTERM` doesn't advertise it.

## Your own

Add `~/.config/turly/themes/<name>.toml`, then `:theme <name>`:

```toml
base = "nord"        # optional starting preset
accent = "#ff8800"
string = "#a3be8c"
```

Slots: `bg fg dim accent border selection search ok warn err info key string number keyword comment`.
