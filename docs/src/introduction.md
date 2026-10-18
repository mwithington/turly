<div class="hero">
  <img src="assets/logo.svg" alt="Turly">
  <h1>Turly</h1>
  <p>A Postman-style API client that lives in your terminal. Vim keys, plain-file workspaces and OpenAPI import.</p>
  <div class="cta">
    <a class="primary" href="getting-started.html">Get started</a>
    <a href="https://github.com/mwithington/turly">GitHub</a>
  </div>
</div>

```sh
cargo install turly
turly -i https://petstore3.swagger.io/api/v3/openapi.json
```

<div class="features">
  <div><h3>Three-pane TUI</h3><p>Request tree, request editor and response viewer, with the mouse if you want it.</p></div>
  <div><h3>Vim keys</h3><p>Modal editing everywhere, a command box, search, and every key remappable.</p></div>
  <div><h3>Plain-file workspaces</h3><p>One YAML file per request. Diff them, review them, commit them.</p></div>
  <div><h3>OpenAPI import</h3><p>Build a whole workspace from an OpenAPI 3.x or Swagger 2.0 spec, file or URL.</p></div>
  <div><h3>Real auth</h3><p>Bearer, Basic, API key and OAuth2 with browser sign-in, plus a workspace default.</p></div>
  <div><h3>Secrets stay out of git</h3><p>Environments and variables, with secret values kept in a gitignored file.</p></div>
</div>

Responses are syntax-highlighted and searchable, every request is kept in history, and any request can be copied as curl. Ten themes ship in the box, and you can add your own in TOML.

New here? Start with [Getting started](getting-started.md), then read [Editing requests](editing.md) and [Authentication](authentication.md).
