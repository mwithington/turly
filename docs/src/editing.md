# Editing requests

Press `i` or `a` on the URL or on a tab to edit it in the Vim editor. `Esc` leaves it, `Ctrl-s` saves, and `Ctrl-r` sends.

## Tabs

```text
Params     status=available          one key=value per line, "# " disables a line
Headers    Accept: application/json  one Name: value per line
Auth       token: {{token}}          fields depend on the type (press t to change)
Body       { "name": "doggie" }      json, text or form (key=value lines)
Example    { "name": "doggie" }      read-only sample from the OpenAPI spec
```

`:example` copies the Example tab into Body. `Ctrl-e` in editor normal mode opens the buffer in `$EDITOR`.

## Variables inline

`{{vars}}` show their resolved value (red) when the cursor is not on that line, and the raw `{{name}}` while it is. Unresolved ones stay as `{{name}}` in yellow.

## Responses

The response pane shows the body (pretty-printed and syntax-highlighted), headers, and status. `/` searches the body. `:save <path>` writes the full body to a file, and `y` copies the request as curl.
