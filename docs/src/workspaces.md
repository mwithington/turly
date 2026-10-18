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

All registered workspaces are remembered in `~/.config/turly/workspaces.yaml`. Open one with `turly <path>` or switch inside the app with `:ws`.
