# Keys and Vim motions

| Key | Action |
| --- | --- |
| `h` `j` `k` `l` | Move, collapse and expand folders, switch tabs |
| `gg` / `G` | Top / bottom |
| `Ctrl-d` / `Ctrl-u` | Half page down / up |
| `Ctrl-w` + `h` `j` `k` `l` | Focus a pane (`Ctrl-h/j/k/l` also work) |
| `Ctrl-w` `w` | Next pane |
| `Tab` / `Shift-Tab` | Next / previous tab |
| `Enter` | Open a request and send it |
| `i` / `a` | Edit the URL or the open tab in the Vim editor |
| `Esc` | Leave the editor, or clear the sidebar filter |
| `Ctrl-r` | Send the request (also while editing) |
| `Ctrl-s` | Save the request |
| `:` | Command box |
| `/`, `n`, `N` | Search the focused pane, next, previous |
| `o` | New request |
| `dd` | Delete a request (asks first) |
| `m` / `t` | Cycle the method / the auth or body type |
| `e` | Next environment |
| `y` | Copy the request as curl |
| `?` | Key reference |
| `q` / `Ctrl-c` | Quit |

## Search

`/` is context-sensitive. In the sidebar it live-filters requests by name, method and URL. In the response it highlights matches in the body. In the editor it searches the buffer.

## Editor motions

Each editable field is a modal Vim buffer (normal, insert, visual, `.` repeat). Beyond the built-in motions, normal mode adds `F` `T` `;` `,` `^` `C`, and `d`/`c`/`y` with `$ 0 ^ e b f F t T ; ,`, for example `d$`, `dtB` and `cF"`.

These custom operators aren't repeated by `.`.

## Mouse

Click folders, requests, tabs and fields, drag to select, and scroll with the wheel. Hold Shift for the terminal's native text selection, or set `mouse = false` in the [config](configuration.md).

## Remapping

Everything is remappable. See [Configuration](configuration.md).
