# Limitations

- No scripting, tests or assertions, and no Postman collection import yet.
- HTTP only: no WebSocket, GraphQL or gRPC.
- The cookie jar lasts for the session and isn't saved to disk.
- Responses over 10 MB are truncated in the viewer. `:save <path>` writes the full body.
- Inline variable values are revealed per line, not per variable.
- Import makes one folder level (the first tag).
