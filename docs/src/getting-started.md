# Getting started

## Install

```sh
# from crates.io
cargo install turly

# from source
git clone https://github.com/mwithington/turly && cd turly
cargo install --path .
```

Prebuilt binaries for macOS, Linux and Windows are attached to each [GitHub release](https://github.com/mwithington/turly/releases).

## Try a spec without leaving anything behind

```sh
turly -i petstore-openapi.json
```

`-i` / `--import` accepts a file or URL. It imports into a temporary workspace that is deleted when you quit and is never registered.

## Build a workspace from a spec

```sh
turly import https://petstore3.swagger.io/api/v3/openapi.json --dir petstore
turly petstore
```

## Start empty

```sh
turly init my-api
turly my-api
```

`turly` with no arguments opens the workspace in the current directory, or the most recently used one. Inside the app, `:wsnew <path>` and `:import <spec>` do the same as the commands above.

## First request

Imported requests point at `{{base_url}}`. Set it for your environment with `:set base_url=https://api.example.com`, and secrets with `:secret token=...`. Or edit them all at once with `:vars`.

Move to a request with `j`/`k`, press `Enter` to open and send it, and press `?` for the key reference.
