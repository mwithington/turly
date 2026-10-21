# Workspaces

A workspace is a plain directory:

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

## A request file

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

Folders sort first, then requests by `order`, then by name.

## Several workspaces

All registered workspaces are remembered in `~/.config/turly/workspaces.yaml`. Open one with `turly <path>`, or switch inside the app: press `W` (or run `:ws` with no argument) for a picker listing every known workspace, with a `●` on the current one. `:ws <name or path>` switches directly.

## Several workspaces in one folder

Launch Turly in a folder that holds workspaces, or point it at one (`turly ~/apis`). It finds every workspace in that folder and up to two levels below it, skipping hidden folders, `target` and `node_modules`. When there are two or more, they appear as the top-level rows of the sidebar. Expand as many as you like to see their requests side by side:

```text
▾ alpha
  GET  Get users
▾ gamma ●
  GET  Ping
  ▸ history
▸ team-beta
```

The `●` marks the active workspace: the one whose environment, variables and history the commands use. Opening a request in another workspace makes that workspace active. A folder with a single workspace shows the plain tree.

| Key | On a workspace row |
| --- | --- |
| `l` / `h` | Expand / collapse it |
| `Enter` or click | Collapsed: expand and activate. Expanded but not active: activate. Active: collapse |
| `z M` | Collapse every workspace |
| `z R` | Expand every workspace |

Activating a workspace closes the open request, so it is refused while that request has unsaved edits. Which workspace is active at startup is the most recently used one in the folder, else the first. `W` still lists every workspace Turly knows about, in any folder.

Switching is refused while the open request has unsaved edits. The open request and response are cleared on a switch, and each workspace keeps its own active environment.

## A base folder

Keep every workspace in one folder and Turly finds them from anywhere:

```
:base ~/api-workspaces
```

The folder is saved as `base_dir` in `config.toml`. When you run `turly` somewhere with no workspace, it opens the base folder instead. New workspaces (`:wsnew name`) and imports (`:import spec`) are created next to the workspace you have open, which is inside the base folder when that is where you work. Pass a folder to `:import spec dir` to put one elsewhere.

Workspaces nested inside other workspaces, or one folder level down, show up in the tree too.
