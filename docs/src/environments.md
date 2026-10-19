# Environments and variables

Variables resolve from the active environment. Use them anywhere in a request as `{{name}}`. `{{$NAME}}` reads an OS environment variable.

Switch environments with `e` or `:env <name>`, and create one with `:envnew <name>`.

## Editing

- `:set key=value` sets one variable.
- `:secret key=value` sets a secret.
- `:vars` opens the whole environment as text, one `key=value` or `secret key=value` per line. `Ctrl-s` or `:w` saves, `:wq` saves and closes, and `:q` or `Esc` closes.

Values longer than 20 characters are shown cut off with `…` in the `:vars` editor. Move the cursor onto a line and the window widens to show it in full.

## Secrets

Secret values live in `.turly/secrets.yaml`, which `turly init` adds to `.gitignore`. The environment file only records their names, so it is safe to commit.
