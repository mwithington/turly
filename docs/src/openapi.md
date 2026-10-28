# OpenAPI import

`turly import <spec>` (or `:import`), or `:paste` to paste the spec text into a box inside Turly (Ctrl-s imports, Esc closes), accepts OpenAPI 3.0/3.1 and Swagger 2.0, as JSON or YAML, from a file or an `http(s)` URL. Swagger 2.0 is converted to 3.0 in memory first.

- Tags become folders, operations become requests, in the spec's order.
- `servers` become environments with a `base_url` variable. Path parameters become `{{vars}}` with example values.
- Query and header parameters are listed, enabled when the spec marks them required.
- JSON and form bodies are pre-filled from the spec's `example`/`examples` (or a schema's `example`/`default`), else generated from the schema's types. The original stays in the Example tab.
- Bearer, Basic and API key security schemes become auth, with secret variables to fill in.
- Summaries and descriptions show up in each request's Docs tab.

Import creates a new workspace. Re-importing over an existing one is not supported yet. For a throwaway one, use `turly -i <spec>`.
