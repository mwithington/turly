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
- **Auth**: Bearer, Basic, API key and OAuth2 (client credentials, password, browser sign-in), with a workspace default.
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

`turly` with no arguments opens the workspace in the current directory. Anywhere else it shows a start screen listing your recent workspaces (`j`/`k`, `Enter` or a click to open one). With no request open, the request pane lists the requests you sent recently. Inside the app, `:wsnew <path>` and `:import <spec>` do the same as the commands above.

Imported requests point at `{{base_url}}`. Set it for your environment with `:set base_url=https://api.example.com`, and secrets with `:secret token=...`.

## Documentation

Full docs live at **https://mwithington.github.io/turly**:

- [Keys and Vim motions](https://mwithington.github.io/turly/keys.html)
- [Commands](https://mwithington.github.io/turly/commands.html)
- [Workspaces](https://mwithington.github.io/turly/workspaces.html)
- [Authentication](https://mwithington.github.io/turly/authentication.html)
- [Environments and variables](https://mwithington.github.io/turly/environments.html)
- [OpenAPI import](https://mwithington.github.io/turly/openapi.html)
- [Configuration](https://mwithington.github.io/turly/configuration.html) and [themes](https://mwithington.github.io/turly/themes.html)

Press `?` in the app for the key reference, and `:` for the command box.

## Limitations

- No scripting, tests or assertions, and no Postman collection import yet.
- HTTP only: no WebSocket, GraphQL or gRPC.
- The cookie jar lasts for the session and isn't saved to disk.
- Responses over 10 MB are truncated in the viewer. `:save <path>` writes the full body.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Docs are an mdBook in `docs/` (`mdbook serve docs`). Releases are automated with release-please, and dependencies with Renovate.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT) at your option.
