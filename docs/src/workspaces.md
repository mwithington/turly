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

Switching is refused while the open request has unsaved edits. The open request and response are cleared on a switch, and each workspace keeps its own active environment.
