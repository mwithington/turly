# Commands

Press `:` and type. `Tab` completes, and the list shows what each command does.

| Command | What it does |
| --- | --- |
| `:w` / `:wq` | Save / save and quit |
| `:q` / `:q!` / `:qa` | Quit (warns on unsaved changes) / quit and discard / quit out |
| `:new [name]`, `:mkdir <name>`, `:rm`, `:dup`, `:rename <name>`, `:discard` | Manage requests and folders |
| `:copy` | Copy the response (body, or headers on that tab) |
| `:send`, `:method <M>`, `:curl`, `:save <path>` | Send, set the method, copy as curl, save the response body |
| `:ws [path\|name]`, `:wsnew <path>` | Switch to a workspace (no argument opens the picker) / create one |
| `:env [name]`, `:envnew <name>` | Switch to / create an environment |
| `:set k=v`, `:secret k=v` | Set a variable / a secret in the active environment |
| `:vars` | Edit the active environment line by line |
| `:authws [clear]` | Make this request's auth the workspace default |
| `:logout` | Forget cached OAuth2 tokens |
| `:example` | Reset the body to the spec's example |
| `:import <spec> [dir]` | Build a workspace from an OpenAPI or Swagger spec |
| `:theme [name]` | Switch theme |
| `:help` | Key reference |

`/` opens the search box instead.
