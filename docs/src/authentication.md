# Authentication

Each request has an auth type, shown on the **Auth** tab. Press `t` to cycle through them:

`none` → `bearer` → `basic` → `apikey` → `oauth2` → `inherit`

Every field accepts `{{variables}}`, so keep real values in the environment (`:secret token=...`) and put `{{token}}` in the request.

| Type | What is sent |
| --- | --- |
| `bearer` | `Authorization: Bearer <token>` |
| `basic` | `Authorization: Basic base64(user:pass)` |
| `apikey` | a custom header (default `X-API-Key`), or a query parameter with `in: query` |
| `oauth2` | `Authorization: Bearer <access token>`, fetched for you (below) |
| `inherit` | whatever the workspace default is |

A header you add yourself on the Headers tab named `Authorization` always wins over bearer, basic and oauth2.

## Workspace default

Set the auth on one request, then run `:authws`. It is saved as `auth:` in `turly.yaml`, and every request set to `inherit` uses it. New requests start as `inherit`. `:authws clear` removes it.

```yaml
# turly.yaml
name: my-api
auth:
  type: bearer
  token: "{{token}}"
```

A request set to `none` stays unauthenticated, which is what OpenAPI import uses for public endpoints.

## OAuth2

The Auth tab for `oauth2` is one `field: value` per line:

```text
grant: client_credentials
token_url: https://auth.example.com/oauth/token
auth_url:
client_id: {{client_id}}
client_secret: {{client_secret}}
scope: read write
username:
password:
```

| `grant` | Use | Fields needed |
| --- | --- | --- |
| `client_credentials` | machine to machine | `token_url`, `client_id`, `client_secret` |
| `password` | legacy username and password login | plus `username`, `password` |
| `code` | sign in with a browser (authorization code with PKCE) | `auth_url`, `token_url`, `client_id` |

On send, Turly gets a token, adds it as a bearer header, and caches it in memory for the session. It reuses the token until shortly before it expires, then refreshes it with the refresh token if the server gave one, or gets a new one. `:logout` clears the cache. Credentials are sent in the request body (`client_secret_post`).

### Browser sign-in (`code`)

Sending the request opens your browser at `auth_url`. After you approve, the provider redirects to `http://127.0.0.1:8765/callback`, where Turly is listening, and the page tells you to return to the terminal. Turly waits two minutes. If the browser doesn't open, the sign-in URL is in the status line.

Register `http://127.0.0.1:8765/callback` as the redirect URI with your provider. Add a `port:` line to use a different port.

### From OpenAPI

Importing a spec with an `oauth2` scheme fills in the URLs and scopes from the spec, using the authorization code flow if it has one, then client credentials, then password. `client_id` and `client_secret` become secret variables for you to fill in.

## Not supported

Digest auth, AWS SigV4, client certificates (mTLS), the OAuth2 implicit and device flows, and folder-level auth. Tokens are not saved to disk, so a `code` login repeats on each launch. The copied curl command (`y`) can't include an OAuth2 token because it is fetched at send time.
