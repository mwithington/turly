# Configuration

`~/.config/turly/config.toml` (or `$XDG_CONFIG_HOME/turly/`):

```toml
theme = "nord"
mouse = true   # false keeps native terminal text selection
base_dir = "~/api-workspaces"   # folder holding all your workspaces (set with :base)

# Remap or add keys. Chords are space separated.
[keys]
"ctrl-s" = "save"
"g d"    = "delete_request"
"x"      = "quit"
```

## Actions

`up down left right top bottom half_down half_up focus_left focus_right focus_up focus_down focus_next next_tab prev_tab open edit send save command search search_next search_prev cancel quit help new_request delete_request cycle_method cycle_kind next_env copy_curl copy_response pane_wider pane_narrower pane_taller pane_shorter pane_reset workspaces collapse_all expand_all`
