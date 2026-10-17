<p align="center">
  <img src="assets/logo.svg" alt="Turly logo" width="128">
</p>

<h1 align="center">Turly</h1>

<p align="center">
  A Postman-style API client that lives in your terminal. Vim keys, plain-file workspaces, OpenAPI import.
</p>

<p align="center">
  <a href="https://github.com/mwithington/turly/actions/workflows/ci.yml"><img src="https://github.com/mwithington/turly/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://crates.io/crates/turly"><img src="https://img.shields.io/crates/v/turly.svg" alt="crates.io"></a>
  <img src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue" alt="License">
</p>

## Features

- **Three-pane TUI**: request tree, request editor, response viewer.
- **Vim keys out of the box**: `hjkl`, `gg`/`G`, `Ctrl-w` pane focus, and a modal Vim editor (normal, insert, visual, `.` repeat) for URLs, params, headers, auth and bodies. Everything is remappable. The mouse works too: click folders, requests, tabs and fields, and scroll with the wheel (hold Shift for native text selection in most terminals).
- **Command and search boxes**: `:` opens a centered command box that explains each command as you type (`:w` Save current request, `:qa` Quit out). `/` searches whichever pane has focus.
- **Workspaces are plain directories**: one YAML file per request, folders for structure. Diff them, review them, commit them.
- **Multiple workspaces**: open one with `turly <path>`, or switch with `:ws`.
- **OpenAPI and Swagger import**: build a whole workspace from an OpenAPI 3.x or Swagger 2.0 spec (JSON or YAML, file or URL).
- **Environments and variables**: `{{base_url}}`, `{{$OS_ENV_VAR}}`, and secrets kept out of git.
- **Auth helpers**: Bearer, Basic and API key.
- **Syntax-highlighted responses** (via `syntect`), pretty-printed JSON, response search, request history, curl export.
- **Themes**: ten presets plus your own TOML themes.

## Install

```sh
# from crates.io
cargo install turly

# from source
git clone https://github.com/mwithington/turly && cd turly
cargo install --path .
```

Prebuilt binaries for macOS, Linux and Windows are attached to each [GitHub release](https://github.com/mwithington/turly/releases).

## Quick start

```sh
# Build a workspace from a spec (file or URL)
turly import https://petstore3.swagger.io/api/v3/openapi.json --dir petstore

# or try a spec in a throwaway workspace (deleted on quit)
turly -i petstore-openapi.json
turly petstore
```

Or start empty:

```sh
turly init my-api
turly my-api
```

`turly` with no arguments opens the workspace in the current directory, or the most recently used one. Inside the app, `:wsnew <path>` and `:import <spec>` do the same as the commands above.

Imported requests point at `{{base_url}}`. Set it for your environment with `:set base_url=https://api.example.com`, and secrets with `:secret token=...`.

## Using it

| Key | Action |
| --- | --- |
| `h` `j` `k` `l` | Move, collapse and expand folders, switch tabs |
| `gg` / `G` | Top / bottom |
| `Ctrl-d` / `Ctrl-u` | Half page down / up |
| `Ctrl-w` + `h` `j` `k` `l` | Focus a pane (`Ctrl-h/j/k/l` also work) |
| `Tab` / `Shift-Tab` | Next / previous tab |
| `Enter` | Open a request and send it |
| `i` / `a` | Edit the URL or the open tab in the Vim editor |
| `Esc` | Leave the editor, or clear the sidebar filter |
| `Ctrl-r` | Send the request (also while editing) |
| `Ctrl-s` | Save the request |
| `:` | Command box |
| `/`, `n`, `N` | Search the focused pane, next, previous |
| `o` | New request |
| `dd` | Delete a request (asks first) |
| `m` / `t` | Cycle the method / the auth or body type |
| `e` | Next environment |
| `y` | Copy the request as curl |
| `?` | Key reference |
| `q` / `Ctrl-c` | Quit |

`/` is context-sensitive. In the sidebar it live-filters requests by name, method and URL. In the response it highlights matches in the body. In the editor it searches the buffer.

### Commands

Press `:` and type. Tab completes, and the list shows what each command does.

| Command | What it does |
| --- | --- |
| `:w` / `:wq` | Save / save and quit |
| `:q` / `:q!` / `:qa` | Quit (warns on unsaved changes) / quit and discard / quit out |
| `:new [name]`, `:mkdir <name>`, `:rm`, `:dup`, `:rename <name>`, `:discard` | Manage requests and folders |
| `:send`, `:method <M>`, `:curl`, `:save <path>` | Send, set the method, copy as curl, save the response body |
| `:ws <path\|name>`, `:wsnew <path>` | Switch to / create a workspace |
| `:env [name]`, `:envnew <name>` | Switch to / create an environment |
| `:set k=v`, `:secret k=v` | Set a variable / a secret in the active environment |
| `:vars` | Edit the active environment line by line (`key=value`, `secret key=value`; `Ctrl-s` or `:w` saves, `:wq` saves and closes, `:q` or `Esc` closes) |
| `:example` | Reset the body to the spec's example |
| `:import <spec> [dir]` | Build a workspace from an OpenAPI or Swagger spec |
| `:theme [name]` | Switch theme |
| `:help` | Key reference |

### Editor tabs

Each request tab is a small Vim buffer:

```text
Params     status=available          one key=value per line, "# " disables a line
Headers    Accept: application/json  one Name: value per line
Auth       token: {{token}}          fields depend on the type (press t to change)
Body       { "name": "doggie" }      json, text or form (key=value lines)
Example    { "name": "doggie" }      read-only sample from the OpenAPI spec (:example copies it into Body)
```

`Ctrl-e` in editor normal mode opens the buffer in `$EDITOR`.

`{{vars}}` show their resolved value (red) when you're not on that line, and the raw `{{name}}` while the cursor is on the line. Unresolved ones stay as `{{name}}` in yellow.

Beyond edtui's built-in motions, normal mode adds `F` `T` `;` `,` `^` `C`, and `d`/`c`/`y` with `$ 0 ^ e b f F t T ; ,` (for example `d$`, `dtB`, `cF"`). These custom operators aren't repeated by `.`.

## Workspaces

```text
my-api/
├── turly.yaml               name, timeout, active environment
├── requests/
│   └── pet/
│       └── find-pets-by-status.yaml
├── environments/
│   └── default.yaml         variables and the names of secret variables
└── .turly/                  gitignored: secrets.yaml, history.jsonl
```

A request file:

```yaml
name: findPetsByStatus
method: GET
url: "{{base_url}}/pet/findByStatus"
params:
  - { key: status, value: available, enabled: true }
auth: { type: bearer, token: "{{token}}" }
body: { type: none, content: "" }
docs: "Finds pets by status."
order: 3 # optional: pins the position in the sidebar
```

Variables resolve from the active environment. Secret values live in `.turly/secrets.yaml`, which `turly init` adds to `.gitignore`. `{{$NAME}}` reads an OS environment variable.

All registered workspaces are remembered in `~/.config/turly/workspaces.yaml`.

## OpenAPI import

`turly import <spec>` (or `:import`) accepts OpenAPI 3.0/3.1 and Swagger 2.0, as JSON or YAML, from a file or an `http(s)` URL. Swagger 2.0 is converted to 3.0 in memory first.

- Tags become folders, operations become requests, in the spec's order.
- `servers` become environments with a `base_url` variable. Path parameters become `{{vars}}` with example values.
- Query and header parameters are listed, enabled when the spec marks them required.
- JSON and form bodies are pre-filled from the spec's `example`/`examples` (or a schema's `example`/`default`), else generated from the schema's types. The original stays in the Example tab.
- Bearer, Basic and API key security schemes become auth, with secret variables to fill in.
- Summaries and descriptions show up in each request's Docs tab.

Import creates a new workspace. Re-importing over an existing one is not supported yet.

## Configuration

`~/.config/turly/config.toml` (or `$XDG_CONFIG_HOME/turly/`):

```toml
theme = "nord"
mouse = true   # false keeps native terminal text selection

# Remap or add keys. Chords are space separated.
[keys]
"ctrl-s" = "save"
"g d"    = "delete_request"
"x"      = "quit"
```

Actions: `up down left right top bottom half_down half_up focus_left focus_right focus_up focus_down focus_next next_tab prev_tab open edit send save command search search_next search_prev cancel quit help new_request delete_request cycle_method cycle_kind next_env copy_curl`.

### Themes

Presets: `turly` (dark), `turly-light`, `gruvbox`, `nord`, `catppuccin`, `catppuccin-latte`, `solarized`, `solarized-light`, `tokyonight`, `dracula` and `mono`. The default follows your terminal background when it reports one (`COLORFGBG`), and truecolor falls back to 256 colors when `COLORTERM` doesn't advertise it.

Add your own in `~/.config/turly/themes/<name>.toml`, then `:theme <name>`:

```toml
base = "nord"        # optional starting preset
accent = "#ff8800"
string = "#a3be8c"
```

Slots: `bg fg dim accent border selection search ok warn err info key string number keyword comment`.

## Limitations

- No scripting, tests or assertions, and no Postman collection import yet.
- HTTP only: no WebSocket, GraphQL or gRPC.
- The cookie jar lasts for the session and isn't saved to disk.
- Responses over 10 MB are truncated in the viewer. `:save <path>` writes the full body.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Releases are automated with release-please, and dependencies with Renovate.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
