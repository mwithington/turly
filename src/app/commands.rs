//! The `:` command line: table, completion and execution.

use super::*;
use crate::model::{Auth, Env};
use crate::openapi;
use crate::workspace::slug;

struct Cmd {
    name: &'static str,
    args: &'static str,
    desc: &'static str,
}

const CMDS: &[Cmd] = &[
    Cmd {
        name: "w",
        args: "",
        desc: "Save current request",
    },
    Cmd {
        name: "wq",
        args: "",
        desc: "Save and quit",
    },
    Cmd {
        name: "q",
        args: "",
        desc: "Quit (warns about unsaved changes)",
    },
    Cmd {
        name: "q!",
        args: "",
        desc: "Quit and discard changes",
    },
    Cmd {
        name: "qa",
        args: "",
        desc: "Quit out",
    },
    Cmd {
        name: "new",
        args: "[name]",
        desc: "New request in the selected folder",
    },
    Cmd {
        name: "mkdir",
        args: "<name>",
        desc: "New folder in the selected folder",
    },
    Cmd {
        name: "rm",
        args: "",
        desc: "Delete the selected request",
    },
    Cmd {
        name: "dup",
        args: "",
        desc: "Duplicate the open request",
    },
    Cmd {
        name: "rename",
        args: "<name>",
        desc: "Rename the open request",
    },
    Cmd {
        name: "discard",
        args: "",
        desc: "Discard unsaved changes",
    },
    Cmd {
        name: "send",
        args: "",
        desc: "Send the open request",
    },
    Cmd {
        name: "method",
        args: "<GET|POST|...>",
        desc: "Set the HTTP method",
    },
    Cmd {
        name: "ws",
        args: "[path|name]",
        desc: "Switch workspace (no argument opens the picker)",
    },
    Cmd {
        name: "wsnew",
        args: "<path>",
        desc: "Create a workspace",
    },
    Cmd {
        name: "env",
        args: "[name]",
        desc: "Switch environment (no name cycles)",
    },
    Cmd {
        name: "envnew",
        args: "<name>",
        desc: "Create an environment",
    },
    Cmd {
        name: "set",
        args: "<key=value>",
        desc: "Set a variable in the active environment",
    },
    Cmd {
        name: "secret",
        args: "<key=value>",
        desc: "Set a secret (kept out of git)",
    },
    Cmd {
        name: "import",
        args: "<spec> [dir]",
        desc: "Import an OpenAPI/Swagger spec as a workspace",
    },
    Cmd {
        name: "theme",
        args: "[name]",
        desc: "Switch color theme",
    },
    Cmd {
        name: "copy",
        args: "",
        desc: "Copy the response (body, or headers on that tab)",
    },
    Cmd {
        name: "curl",
        args: "",
        desc: "Copy the request as a curl command",
    },
    Cmd {
        name: "save",
        args: "<path>",
        desc: "Save the response body to a file",
    },
    Cmd {
        name: "example",
        args: "",
        desc: "Reset the body to the spec's example",
    },
    Cmd {
        name: "authws",
        args: "[clear]",
        desc: "Make this request's auth the workspace default (inherit)",
    },
    Cmd {
        name: "logout",
        args: "",
        desc: "Forget cached OAuth2 tokens",
    },
    Cmd {
        name: "vars",
        args: "",
        desc: "Edit the active environment's variables line by line",
    },
    Cmd {
        name: "help",
        args: "",
        desc: "Show key bindings",
    },
];

pub struct Comp {
    pub label: String,
    pub desc: String,
    pub insert: String,
}

fn expand(p: &str) -> PathBuf {
    match (p.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(p),
    }
}

impl App {
    pub fn completions(&self) -> Vec<Comp> {
        let input = self.input.trim_start();
        let Some((cmd, arg)) = input.split_once(' ') else {
            return CMDS
                .iter()
                .filter(|c| c.name.starts_with(input))
                .map(|c| Comp {
                    label: format!(":{} {}", c.name, c.args).trim_end().to_string(),
                    desc: c.desc.to_string(),
                    insert: if c.args.is_empty() {
                        c.name.to_string()
                    } else {
                        format!("{} ", c.name)
                    },
                })
                .collect();
        };
        let arg = arg.trim_start();
        let cands: Vec<(String, String)> = match cmd {
            "theme" => Theme::all_names()
                .into_iter()
                .map(|n| (n, String::new()))
                .collect(),
            "env" => self
                .ws
                .as_ref()
                .map(|w| w.envs().unwrap_or_default())
                .unwrap_or_default()
                .into_iter()
                .map(|n| (n, String::new()))
                .collect(),
            "method" => Method::ALL
                .iter()
                .map(|m| (m.as_str().to_string(), String::new()))
                .collect(),
            "ws" => registry::load()
                .into_iter()
                .map(|p| {
                    let name = Workspace::open(&p).map(|w| w.meta.name).unwrap_or_default();
                    (p.display().to_string(), name)
                })
                .collect(),
            _ => vec![],
        };
        let a = arg.to_lowercase();
        let mut hits: Vec<_> = cands
            .into_iter()
            .filter(|(v, _)| v.to_lowercase().contains(&a))
            .collect();
        hits.sort_by_key(|(v, _)| !v.to_lowercase().starts_with(&a));
        hits.into_iter()
            .map(|(v, d)| Comp {
                label: v.clone(),
                desc: d,
                insert: format!("{cmd} {v}"),
            })
            .collect()
    }

    pub fn complete(&mut self) {
        let comps = self.completions();
        if let Some(c) = comps.get(self.sel.min(comps.len().saturating_sub(1))) {
            self.input = c.insert.clone();
            self.sel = 0;
        }
    }

    /// Typed line if its command is exact, else the selected completion.
    pub fn resolve_command_line(&mut self) -> String {
        let line = self.input.trim().to_string();
        let first = line.split_whitespace().next().unwrap_or("");
        let comps = self.completions();
        if CMDS.iter().any(|c| c.name == first) {
            // Partial argument for a command with a candidate list: take the selection.
            let partial =
                line.contains(' ') && !comps.is_empty() && !comps.iter().any(|c| c.insert == line);
            if !partial {
                return line;
            }
        }
        comps
            .get(self.sel.min(comps.len().saturating_sub(1)))
            .map(|c| c.insert.trim().to_string())
            .unwrap_or(line)
    }

    pub fn run_command(&mut self, line: &str) {
        let (cmd, arg) = line
            .split_once(' ')
            .map(|(c, a)| (c, a.trim()))
            .unwrap_or((line, ""));
        if self.mode == Mode::Vars {
            match cmd {
                "w" => return self.save_vars(false),
                "wq" => return self.save_vars(true),
                "q" | "qa" => return self.close_vars(false),
                "q!" => return self.close_vars(true),
                _ => {}
            }
        }
        match cmd {
            "" => {}
            "w" => self.save_current(),
            "wq" => {
                self.save_current();
                self.try_quit(false);
            }
            "q" | "qa" => self.try_quit(false),
            "q!" => self.try_quit(true),
            "new" => self.cmd_new(if arg.is_empty() { "New request" } else { arg }),
            "mkdir" => self.cmd_mkdir(arg),
            "rm" => {
                self.focus = Focus::Tree;
                self.act(Action::DeleteRequest);
            }
            "dup" => self.cmd_dup(),
            "rename" => match &mut self.open {
                Some(o) if !arg.is_empty() => o.req.name = arg.to_string(),
                Some(_) => self.err("usage: rename <name>"),
                None => self.err("No request open"),
            },
            "discard" => {
                self.open = None;
                self.info("Discarded changes");
            }
            "send" => self.do_send(),
            "method" => match (Method::parse(arg), &mut self.open) {
                (Some(m), Some(o)) => o.req.method = m,
                (None, _) => self.err("usage: method <GET|POST|PUT|PATCH|DELETE|HEAD|OPTIONS>"),
                _ => self.err("No request open"),
            },
            "ws" => self.cmd_ws(arg),
            "wsnew" => self.cmd_wsnew(arg),
            "env" => {
                if arg.is_empty() {
                    self.act(Action::NextEnv);
                } else if self
                    .ws
                    .as_ref()
                    .is_some_and(|w| w.envs().unwrap_or_default().iter().any(|e| e == arg))
                {
                    self.set_env(arg);
                } else {
                    self.err(format!("No environment '{arg}'"));
                }
            }
            "envnew" => self.cmd_envnew(arg),
            "set" | "secret" => self.cmd_set(arg, cmd == "secret"),
            "import" => self.cmd_import(arg),
            "theme" => {
                if arg.is_empty() {
                    self.err("usage: theme <name>  (Tab lists themes)");
                } else {
                    self.set_theme(arg);
                }
            }
            "curl" => self.act(Action::CopyCurl),
            "copy" => self.act(Action::CopyResponse),
            "save" => self.cmd_save(arg),
            "vars" => self.open_vars(),
            "example" => self.cmd_example(),
            "authws" => self.cmd_authws(arg),
            "logout" => {
                self.tokens.lock().unwrap().clear();
                self.info("Cleared cached OAuth2 tokens");
            }
            "help" => self.mode = Mode::Help,
            other => self.err(format!("Unknown command: {other}")),
        }
    }

    fn cmd_new(&mut self, name: &str) {
        let Some(ws) = self.need_ws() else { return };
        if self.open.as_ref().is_some_and(|o| o.dirty()) {
            return self.err("Unsaved changes. Use :w to save or :discard to drop them");
        }
        let dir = self.selected_dir();
        let path = ws.new_request_path(&dir, name);
        let req = Request::new(name);
        if let Err(e) = ws.save_request(&path, &req) {
            return self.err(e.to_string());
        }
        self.reload_tree();
        self.select_path(&path);
        self.load_request(&path);
        self.focus = Focus::Request;
        self.info(format!("Created '{name}'"));
    }

    fn cmd_mkdir(&mut self, name: &str) {
        if name.is_empty() {
            return self.err("usage: mkdir <name>");
        }
        if self.need_ws().is_none() {
            return;
        }
        match std::fs::create_dir_all(self.selected_dir().join(slug(name))) {
            Ok(()) => self.reload_tree(),
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_dup(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let Some(o) = &mut self.open else {
            return self.err("No request open");
        };
        o.sync();
        let mut req = o.req.clone();
        req.name = format!("{} copy", req.name);
        req.order = None;
        let dir = o
            .path
            .as_ref()
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| ws.requests_dir());
        let path = ws.new_request_path(&dir, &req.name);
        match ws.save_request(&path, &req) {
            Ok(()) => {
                self.reload_tree();
                self.select_path(&path);
                self.open = None;
                self.load_request(&path);
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_ws(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.open_picker();
        }
        let direct = expand(arg);
        let found = if Workspace::is_workspace(&direct) {
            Some(direct)
        } else {
            registry::load().into_iter().find(|p| {
                p.file_name().is_some_and(|n| n.to_string_lossy() == arg)
                    || Workspace::open(p).is_ok_and(|w| w.meta.name == arg)
            })
        };
        match found {
            Some(p) => self.switch_workspace(&p),
            None => self.err(format!("No workspace '{arg}'")),
        }
    }

    /// Switch to the workspace at `path`, unless the open request has unsaved edits.
    pub fn switch_workspace(&mut self, path: &Path) {
        if self.open.as_mut().is_some_and(|o| {
            o.sync();
            o.dirty()
        }) {
            return self.err("Unsaved changes. Use :w to save or :discard to drop them");
        }
        match Workspace::open(path) {
            Ok(ws) => {
                let n = ws.meta.name.clone();
                self.set_workspace(ws);
                self.info(format!("Workspace: {n}"));
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_wsnew(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.err("usage: wsnew <path>");
        }
        let dir = expand(arg);
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "workspace".into());
        match Workspace::init(&dir, &name) {
            Ok(ws) => {
                self.set_workspace(ws);
                self.info(format!("Created workspace '{name}'"));
            }
            Err(e) => self.err(format!("{e:#}")),
        }
    }

    fn cmd_envnew(&mut self, name: &str) {
        let Some(ws) = self.need_ws() else { return };
        if name.is_empty() {
            return self.err("usage: envnew <name>");
        }
        let mut env = Env::default();
        env.vars.insert("base_url".into(), String::new());
        match ws.save_env(&slug(name), &env) {
            Ok(()) => self.set_env(&slug(name)),
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_authws(&mut self, arg: &str) {
        let auth = if arg == "clear" {
            Auth::None
        } else {
            let Some(o) = &mut self.open else {
                return self.err("No request open");
            };
            o.sync();
            o.req.auth.clone()
        };
        if auth == Auth::Inherit {
            return self.err("This request inherits; open one with its own auth");
        }
        let Some(ws) = self.ws.as_mut() else {
            return self.err("No workspace");
        };
        ws.meta.auth = auth;
        let label = ws.meta.auth.label();
        match ws.save_meta() {
            Ok(()) => self.info(format!(
                "Workspace auth: {label} (used by 'inherit' requests)"
            )),
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_example(&mut self) {
        let Some(o) = &mut self.open else {
            return self.err("No request open");
        };
        let Some(ex) = o.req.example.clone() else {
            return self.err("This request has no spec example");
        };
        o.req.body = ex;
        o.ed = Editors::new(&o.req);
        self.info("Body reset to the spec example");
    }

    fn cmd_set(&mut self, arg: &str, secret: bool) {
        let Some(ws) = self.need_ws() else { return };
        let Some(env) = self.env_name() else {
            return self.err("No active environment (:env <name>)");
        };
        let Some((k, v)) = arg.split_once('=') else {
            return self.err("usage: set <key=value>");
        };
        match ws.set_var(&env, k.trim(), v.trim(), secret) {
            Ok(()) => self.info(format!(
                "{} {} in '{env}'",
                if secret { "Secret set:" } else { "Set" },
                k.trim()
            )),
            Err(e) => self.err(e.to_string()),
        }
    }

    fn cmd_import(&mut self, arg: &str) {
        let mut it = arg.split_whitespace();
        let Some(src) = it.next().map(str::to_string) else {
            return self.err("usage: import <spec path|url> [dir]");
        };
        let dir = it.next().map(expand);
        let tx = self.tx.clone();
        self.info(format!("Importing {src}..."));
        self.rt.spawn(async move {
            let r = async {
                let imp = openapi::import(&openapi::fetch(&src).await?)?;
                let dir = dir.unwrap_or_else(|| PathBuf::from(slug(&imp.title)));
                let dir = if dir.is_absolute() {
                    dir
                } else {
                    std::env::current_dir()?.join(dir)
                };
                openapi::build_workspace(&dir, &imp)?;
                anyhow::Ok(dir)
            }
            .await;
            let _ = tx.send(Msg::Imported(r.map_err(|e| format!("{e:#}"))));
        });
    }

    fn cmd_save(&mut self, arg: &str) {
        if arg.is_empty() {
            return self.err("usage: save <path>");
        }
        let Some(r) = &self.resp else {
            return self.err("No response to save");
        };
        match std::fs::write(expand(arg), &r.raw) {
            Ok(()) => self.info(format!("Saved {} bytes to {arg}", r.raw.len())),
            Err(e) => self.err(e.to_string()),
        }
    }
}
