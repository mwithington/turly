//! Application state and input handling.

use crate::config::Config;
use crate::highlight::{Highlighter, Lang};
use crate::http::{self, Response};
use crate::keymap::{Action, Key, KeyMap, Lookup};
use crate::model::{Auth, Grant, Method, OAuth, Param, Request};
use crate::theme::Theme;
use crate::vimext::Vim;
use crate::workspace::{Node, Workspace, registry};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use edtui::actions::search::{
    AppendCharToSearch, FindNext, FindPrevious, SelectCurrentSearch, StartSearch, StopSearch,
};
use edtui::{EditorEventHandler, EditorMode, EditorState, Lines};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::mpsc::UnboundedSender;

mod commands;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Tree,
    Request,
    Response,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Edit,
    Command,
    Search,
    Confirm,
    Help,
    Vars,
    Picker,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReqTab {
    Params,
    Headers,
    Auth,
    Body,
    Example,
    Docs,
}

impl ReqTab {
    pub const ALL: [ReqTab; 6] = [
        ReqTab::Params,
        ReqTab::Headers,
        ReqTab::Auth,
        ReqTab::Body,
        ReqTab::Example,
        ReqTab::Docs,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ReqTab::Params => "Params",
            ReqTab::Headers => "Headers",
            ReqTab::Auth => "Auth",
            ReqTab::Body => "Body",
            ReqTab::Example => "Example",
            ReqTab::Docs => "Docs",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ReqField {
    Url,
    Content,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RespTab {
    Body,
    Headers,
}

pub enum Msg {
    Response {
        prepared: http::Prepared,
        result: Result<Response, String>,
    },
    Imported(Result<PathBuf, String>),
    Info(String),
}

pub struct Editors {
    pub url: EditorState,
    pub params: EditorState,
    pub headers: EditorState,
    pub auth: EditorState,
    pub body: EditorState,
}

pub fn text_of(ed: &EditorState) -> String {
    String::from(ed.lines.clone())
}

fn editor(s: &str) -> EditorState {
    EditorState::new(Lines::from(s))
}

pub fn fmt_pairs(ps: &[Param], sep: &str) -> String {
    ps.iter()
        .map(|p| {
            format!(
                "{}{}{}{}",
                if p.enabled { "" } else { "# " },
                p.key,
                sep,
                p.value
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn parse_pairs(text: &str, sep: char) -> Vec<Param> {
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let (enabled, l) = match l.trim_start().strip_prefix('#') {
                Some(r) => (false, r.trim_start()),
                None => (true, l),
            };
            let (k, v) = l.split_once(sep).unwrap_or((l, ""));
            Param {
                key: k.trim().into(),
                value: v.trim().into(),
                enabled,
            }
        })
        .collect()
}

fn fmt_auth(a: &Auth) -> String {
    match a {
        Auth::None | Auth::Inherit => String::new(),
        Auth::Bearer { token } => format!("token: {token}"),
        Auth::Basic { username, password } => format!("username: {username}\npassword: {password}"),
        Auth::Apikey { key, value, query } => format!(
            "key: {key}\nvalue: {value}\nin: {}",
            if *query { "query" } else { "header" }
        ),
        Auth::Oauth2(o) => {
            let mut out = format!(
                "grant: {}\ntoken_url: {}\nauth_url: {}\nclient_id: {}\nclient_secret: {}\nscope: {}\nusername: {}\npassword: {}",
                o.grant.label(),
                o.token_url,
                o.auth_url,
                o.client_id,
                o.client_secret,
                o.scope,
                o.username,
                o.password
            );
            if o.port != 0 {
                out.push_str(&format!("\nport: {}", o.port));
            }
            out
        }
    }
}

fn parse_auth(base: &Auth, text: &str) -> Auth {
    let get = |name: &str| {
        text.lines()
            .filter_map(|l| l.split_once(':'))
            .find(|(k, _)| k.trim() == name)
            .map(|(_, v)| v.trim().to_string())
            .unwrap_or_default()
    };
    match base {
        Auth::None | Auth::Inherit => base.clone(),
        Auth::Bearer { .. } => Auth::Bearer {
            token: get("token"),
        },
        Auth::Basic { .. } => Auth::Basic {
            username: get("username"),
            password: get("password"),
        },
        Auth::Apikey { .. } => Auth::Apikey {
            key: get("key"),
            value: get("value"),
            query: get("in") == "query",
        },
        Auth::Oauth2(_) => Auth::Oauth2(OAuth {
            grant: Grant::parse(&get("grant")),
            token_url: get("token_url"),
            auth_url: get("auth_url"),
            client_id: get("client_id"),
            client_secret: get("client_secret"),
            scope: get("scope"),
            username: get("username"),
            password: get("password"),
            port: get("port").parse().unwrap_or(0),
        }),
    }
}

impl Editors {
    pub(crate) fn new(r: &Request) -> Editors {
        let mut url = editor(&r.url);
        url.set_single_line(true);
        Editors {
            url,
            params: editor(&fmt_pairs(&r.params, "=")),
            headers: editor(&fmt_pairs(&r.headers, ": ")),
            auth: editor(&fmt_auth(&r.auth)),
            body: editor(&r.body.content),
        }
    }

    pub fn tab_mut(&mut self, t: ReqTab) -> Option<&mut EditorState> {
        match t {
            ReqTab::Params => Some(&mut self.params),
            ReqTab::Headers => Some(&mut self.headers),
            ReqTab::Auth => Some(&mut self.auth),
            ReqTab::Body => Some(&mut self.body),
            ReqTab::Example | ReqTab::Docs => None,
        }
    }
}

pub struct Open {
    pub path: Option<PathBuf>,
    pub req: Request,
    pub saved: Request,
    pub ed: Editors,
}

impl Open {
    fn new(path: Option<PathBuf>, req: Request) -> Open {
        Open {
            path,
            saved: req.clone(),
            ed: Editors::new(&req),
            req,
        }
    }

    /// Pull editor text back into the request.
    pub fn sync(&mut self) {
        self.req.url = text_of(&self.ed.url).replace('\n', "");
        self.req.params = parse_pairs(&text_of(&self.ed.params), '=');
        self.req.headers = parse_pairs(&text_of(&self.ed.headers), ':');
        self.req.auth = parse_auth(&self.req.auth, &text_of(&self.ed.auth));
        self.req.body.content = text_of(&self.ed.body);
    }

    pub fn dirty(&self) -> bool {
        self.req != self.saved
    }
}

#[derive(Clone, Debug)]
pub enum RowKind {
    Dir {
        path: PathBuf,
        open: bool,
    },
    Req {
        path: PathBuf,
        method: Method,
    },
    HistDir {
        open: bool,
    },
    Hist(usize),
    Workspace {
        path: PathBuf,
        open: bool,
        active: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Row {
    pub depth: usize,
    pub name: String,
    pub kind: RowKind,
}

pub struct RespView {
    pub status: u16,
    pub reason: String,
    pub elapsed_ms: u128,
    pub size: usize,
    pub truncated: bool,
    pub headers: Vec<(String, String)>,
    pub raw: Vec<u8>,
    pub pretty: String,
    pub lang: Lang,
    pub lines: Vec<Line<'static>>,
    pub matches: Vec<(usize, usize, usize)>,
    pub cur_match: usize,
}

/// The `:vars` popup: an env's variables as editable text.
pub struct VarsEd {
    pub env: String,
    pub ed: EditorState,
    orig: String,
    armed: bool,
}

/// Clickable regions, refreshed on every draw.
#[derive(Default)]
pub struct Hit {
    pub main: Rect,
    pub picker: Rect,
    pub right: Rect,
    pub side_edge: Rect,
    pub split_edge: Rect,
    pub tree: Rect,
    pub req: Rect,
    pub url: Rect,
    pub tabs: Vec<(Rect, ReqTab)>,
    pub content: Rect,
    pub resp: Rect,
    pub resp_tabs: Vec<(Rect, RespTab)>,
}

pub struct Status {
    pub text: String,
    pub err: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Drag {
    Side,
    Split,
}

pub struct Picker {
    pub items: Vec<(String, PathBuf)>,
    pub sel: usize,
    pub offset: usize,
}

pub struct App {
    /// Folder Turly was launched in, and the workspaces found in it.
    pub scope_root: Option<PathBuf>,
    pub scope: Vec<(String, PathBuf)>,
    /// Expanded workspace rows (scope paths) and their loaded trees.
    pub expanded: HashSet<PathBuf>,
    pub trees: BTreeMap<PathBuf, Vec<Node>>,
    pub picker: Option<Picker>,
    /// Sidebar width in columns (None = automatic) and request pane height in percent.
    pub side_w: Option<u16>,
    pub split: u16,
    pub drag: Option<Drag>,
    pub ws: Option<Workspace>,
    pub theme: Theme,
    pub keys: KeyMap,
    pub hl: Highlighter,
    pub handler: EditorEventHandler,
    pub vars: Option<VarsEd>,
    pub vim: Vim,
    pub hit: Hit,
    pub var_cache: BTreeMap<String, String>,
    pub blink: Instant,
    pub mode: Mode,
    pub return_to: Mode,
    pub focus: Focus,
    pub req_tab: ReqTab,
    pub req_field: ReqField,
    pub resp_tab: RespTab,
    pub nodes: Vec<Node>,
    pub collapsed: HashSet<PathBuf>,
    pub hist_open: bool,
    pub history: Vec<serde_json::Value>,
    pub cursor: usize,
    pub tree_offset: usize,
    pub filter: String,
    pub open: Option<Open>,
    pub resp: Option<RespView>,
    pub resp_scroll: usize,
    pub resp_height: usize,
    pub sending: bool,
    pub error: Option<String>,
    pub input: String,
    pub sel: usize,
    pub search_focus: Focus,
    pub pending: Vec<Key>,
    pub status: Option<Status>,
    pub quit: bool,
    pub confirm: Option<String>,
    pub jar: Arc<reqwest::cookie::Jar>,
    pub tokens: crate::oauth::Cache,
    pub rt: Handle,
    pub tx: UnboundedSender<Msg>,
}

impl App {
    pub fn new(ws: Option<Workspace>, cfg: Config, rt: Handle, tx: UnboundedSender<Msg>) -> App {
        let mut warn = vec![];
        let keys = KeyMap::new(&cfg.keys).unwrap_or_else(|e| {
            warn.push(format!("config keys: {e}"));
            KeyMap::new(&Default::default()).expect("default keymap")
        });
        let name = cfg
            .theme
            .clone()
            .unwrap_or_else(|| Theme::default_name().to_string());
        let mut theme = Theme::load(&name).unwrap_or_else(|e| {
            warn.push(e.to_string());
            Theme::preset("turly").expect("preset")
        });
        theme.downgrade_if_needed();
        let mut app = App {
            ws: None,
            theme,
            keys,
            hl: Highlighter::new(),
            handler: EditorEventHandler::default(),
            vars: None,
            vim: Vim::default(),
            hit: Hit::default(),
            var_cache: BTreeMap::new(),
            blink: Instant::now(),
            mode: Mode::Normal,
            return_to: Mode::Normal,
            focus: Focus::Tree,
            req_tab: ReqTab::Params,
            req_field: ReqField::Url,
            resp_tab: RespTab::Body,
            nodes: vec![],
            collapsed: HashSet::new(),
            hist_open: false,
            history: vec![],
            cursor: 0,
            tree_offset: 0,
            side_w: None,
            split: 52,
            drag: None,
            filter: String::new(),
            open: None,
            resp: None,
            resp_scroll: 0,
            resp_height: 10,
            sending: false,
            error: None,
            input: String::new(),
            sel: 0,
            search_focus: Focus::Tree,
            pending: vec![],
            status: None,
            quit: false,
            confirm: None,
            picker: None,
            scope_root: None,
            scope: vec![],
            expanded: HashSet::new(),
            trees: BTreeMap::new(),
            jar: Arc::new(reqwest::cookie::Jar::default()),
            tokens: Default::default(),
            rt,
            tx,
        };
        if !warn.is_empty() {
            app.err(warn.join("; "));
        }
        if let Some(ws) = ws {
            app.set_workspace(ws);
        }
        app
    }

    // ---- status helpers ----

    pub fn info(&mut self, s: impl Into<String>) {
        self.status = Some(Status {
            text: s.into(),
            err: false,
        });
    }

    pub fn err(&mut self, s: impl Into<String>) {
        self.status = Some(Status {
            text: s.into(),
            err: true,
        });
    }

    pub fn need_ws(&mut self) -> Option<Workspace> {
        let ws = self.ws.clone();
        if ws.is_none() {
            self.err("No workspace open. Try :wsnew <path> or :import <spec>");
        }
        ws
    }

    // ---- workspace / tree ----

    /// Show the workspaces inside `root` as top-level tree rows (when there are several).
    pub fn set_scope(&mut self, root: &Path) {
        self.scope_root = root.canonicalize().ok();
        self.rescan_scope();
        self.reset_expanded();
        self.reload_tree();
    }

    fn rescan_scope(&mut self) {
        let Some(root) = &self.scope_root else { return };
        self.scope = crate::workspace::discover(root)
            .into_iter()
            .map(|p| {
                let name = Workspace::open(&p)
                    .map(|w| w.meta.name)
                    .unwrap_or_else(|_| "?".into());
                (name, p)
            })
            .collect();
    }

    pub fn set_workspace(&mut self, ws: Workspace) {
        let _ = registry::add(&ws.root);
        self.open = None;
        self.resp = None;
        self.error = None;
        self.collapsed.clear();
        self.filter.clear();
        self.cursor = 0;
        self.ws = Some(ws);
        self.rescan_scope();
        self.reset_expanded();
        self.refresh_vars();
        self.reload_tree();
    }

    /// Only the active workspace expanded.
    fn reset_expanded(&mut self) {
        self.expanded.clear();
        self.trees.clear();
        let active = self
            .scope
            .iter()
            .find(|(_, p)| self.is_active(p))
            .map(|(_, p)| p.clone());
        self.expanded.extend(active);
    }

    /// Make the workspace at `path` the active one, keeping the tree as it is.
    pub fn activate_workspace(&mut self, path: &Path) {
        if self.is_active(path) {
            return;
        }
        if self.open.as_mut().is_some_and(|o| {
            o.sync();
            o.dirty()
        }) {
            return self.err("Unsaved changes. Use :w to save or :discard to drop them");
        }
        match Workspace::open(path) {
            Ok(ws) => {
                let _ = registry::add(&ws.root);
                self.open = None;
                self.resp = None;
                self.error = None;
                self.ws = Some(ws);
                self.refresh_vars();
                self.reload_tree();
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    fn reload_expanded(&mut self) {
        let keep: Vec<PathBuf> = self.expanded.iter().cloned().collect();
        self.trees.retain(|p, _| keep.contains(p));
        for p in keep {
            match Workspace::open(&p).and_then(|w| w.tree()) {
                Ok(n) => {
                    self.trees.insert(p, n);
                }
                Err(e) => self.err(e.to_string()),
            }
        }
    }

    /// Workspace row at or above the cursor.
    fn cursor_workspace(&self) -> Option<PathBuf> {
        let rows = self.rows();
        rows.iter()
            .take(self.cursor + 1)
            .rev()
            .find_map(|r| match &r.kind {
                RowKind::Workspace { path, .. } => Some(path.clone()),
                _ => None,
            })
    }

    fn cursor_to_workspace(&mut self, path: &Path) {
        let at = self
            .rows()
            .iter()
            .position(|r| matches!(&r.kind, RowKind::Workspace { path: p, .. } if p == path));
        if let Some(i) = at {
            self.cursor = i;
        }
    }

    pub fn expand_workspace(&mut self, path: &Path, on: bool) {
        if on {
            self.expanded.insert(path.to_path_buf());
        } else {
            self.expanded.remove(path);
        }
        self.reload_expanded();
        self.cursor_to_workspace(path);
    }

    /// Collapse or expand every workspace row (`z M` / `z R`).
    fn set_all_expanded(&mut self, on: bool) {
        if !self.multi() {
            return;
        }
        let at = self.cursor_workspace();
        self.expanded = if on {
            self.scope.iter().map(|(_, p)| p.clone()).collect()
        } else {
            HashSet::new()
        };
        self.reload_expanded();
        if let Some(p) = at {
            self.cursor_to_workspace(&p);
        }
    }

    pub fn reload_tree(&mut self) {
        let Some(ws) = self.ws.clone() else { return };
        match ws.tree() {
            Ok(n) => self.nodes = n,
            Err(e) => self.err(e.to_string()),
        }
        self.history = ws.history(50);
        self.reload_expanded();
        let n = self.rows().len();
        self.cursor = self.cursor.min(n.saturating_sub(1));
    }

    /// True when the tree lists several workspaces (the active one is among them).
    fn multi(&self) -> bool {
        self.ws
            .as_ref()
            .is_some_and(|w| self.scope.iter().any(|(_, p)| same_dir(p, &w.root)))
            && self.scope.len() > 1
            && self.filter.is_empty()
    }

    fn is_active(&self, p: &Path) -> bool {
        self.ws.as_ref().is_some_and(|w| same_dir(p, &w.root))
    }

    pub fn rows(&self) -> Vec<Row> {
        if !self.multi() {
            return self.ws_rows();
        }
        let mut out = vec![];
        for (name, path) in &self.scope {
            let (open, active) = (self.expanded.contains(path), self.is_active(path));
            out.push(Row {
                depth: 0,
                name: name.clone(),
                kind: RowKind::Workspace {
                    path: path.clone(),
                    open,
                    active,
                },
            });
            if let Some(nodes) = self.trees.get(path).filter(|_| open) {
                out.extend(self.ws_rows_of(nodes, active).into_iter().map(|mut r| {
                    r.depth += 1;
                    r
                }));
            }
        }
        out
    }

    /// Rows of the active workspace alone.
    fn ws_rows(&self) -> Vec<Row> {
        self.ws_rows_of(&self.nodes, true)
    }

    fn ws_rows_of(&self, nodes: &[Node], with_history: bool) -> Vec<Row> {
        let mut out = vec![];
        if !self.filter.is_empty() {
            let f = self.filter.to_lowercase();
            fn walk(nodes: &[Node], f: &str, out: &mut Vec<Row>) {
                for n in nodes {
                    match n {
                        Node::Dir { children, .. } => walk(children, f, out),
                        Node::Req { path, req } => {
                            let hay = format!("{} {} {}", req.name, req.method.as_str(), req.url)
                                .to_lowercase();
                            if hay.contains(f) {
                                out.push(Row {
                                    depth: 0,
                                    name: req.name.clone(),
                                    kind: RowKind::Req {
                                        path: path.clone(),
                                        method: req.method,
                                    },
                                });
                            }
                        }
                    }
                }
            }
            walk(nodes, &f, &mut out);
            return out;
        }
        fn walk(nodes: &[Node], depth: usize, collapsed: &HashSet<PathBuf>, out: &mut Vec<Row>) {
            for n in nodes {
                match n {
                    Node::Dir {
                        name,
                        path,
                        children,
                    } => {
                        let open = !collapsed.contains(path);
                        out.push(Row {
                            depth,
                            name: name.clone(),
                            kind: RowKind::Dir {
                                path: path.clone(),
                                open,
                            },
                        });
                        if open {
                            walk(children, depth + 1, collapsed, out);
                        }
                    }
                    Node::Req { path, req } => out.push(Row {
                        depth,
                        name: req.name.clone(),
                        kind: RowKind::Req {
                            path: path.clone(),
                            method: req.method,
                        },
                    }),
                }
            }
        }
        walk(nodes, 0, &self.collapsed, &mut out);
        if !with_history {
            return out;
        }
        out.push(Row {
            depth: 0,
            name: "history".into(),
            kind: RowKind::HistDir {
                open: self.hist_open,
            },
        });
        if self.hist_open {
            for (i, h) in self.history.iter().enumerate() {
                let line = format!(
                    "{} {} {}",
                    h["method"].as_str().unwrap_or(""),
                    h["status"].as_str().unwrap_or("ERR"),
                    h["url"].as_str().unwrap_or("")
                );
                out.push(Row {
                    depth: 1,
                    name: line,
                    kind: RowKind::Hist(i),
                });
            }
        }
        out
    }

    fn selected_dir(&self) -> PathBuf {
        let Some(ws) = &self.ws else {
            return PathBuf::new();
        };
        let rows = self.rows();
        match rows.get(self.cursor).map(|r| &r.kind) {
            Some(RowKind::Dir { path, .. }) => path.clone(),
            Some(RowKind::Workspace { path, .. }) => path.join("requests"),
            Some(RowKind::Req { path, .. }) => path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| ws.requests_dir()),
            _ => ws.requests_dir(),
        }
    }

    pub fn select_path(&mut self, p: &Path) {
        if let Some(i) = self
            .rows()
            .iter()
            .position(|r| matches!(&r.kind, RowKind::Req { path, .. } if path == p))
        {
            self.cursor = i;
        }
    }

    // ---- requests ----

    pub fn load_request(&mut self, path: &Path) {
        if self.open.as_ref().is_some_and(|o| o.dirty())
            && self.open.as_ref().and_then(|o| o.path.as_deref()) != Some(path)
        {
            self.err("Unsaved changes. Use :w to save or :discard to drop them");
            return;
        }
        let owner = self
            .scope
            .iter()
            .filter(|(_, p)| path.starts_with(p))
            .max_by_key(|(_, p)| p.as_os_str().len())
            .map(|(_, p)| p.clone());
        if let Some(o) = owner.filter(|o| !self.is_active(o)) {
            self.activate_workspace(&o);
            if !self.is_active(&o) {
                return;
            }
            self.select_path(path);
        }
        let Some(ws) = &self.ws else { return };
        let text = std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_yaml_ng::from_str::<Request>(&s).map_err(|e| e.to_string()));
        match text {
            Ok(req) => {
                let _ = ws;
                self.open = Some(Open::new(Some(path.to_path_buf()), req));
                self.req_tab = ReqTab::Params;
                self.req_field = ReqField::Url;
            }
            Err(e) => self.err(format!("{}: {e}", path.display())),
        }
    }

    pub fn save_current(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let Some(open) = &mut self.open else {
            self.err("No request open");
            return;
        };
        open.sync();
        let path = open
            .path
            .clone()
            .unwrap_or_else(|| ws.new_request_path(&ws.requests_dir(), &open.req.name));
        match ws.save_request(&path, &open.req) {
            Ok(()) => {
                open.saved = open.req.clone();
                open.path = Some(path.clone());
                let msg = format!(
                    "Saved {}",
                    path.strip_prefix(&ws.root).unwrap_or(&path).display()
                );
                self.reload_tree();
                self.info(msg);
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    pub fn env_name(&self) -> Option<String> {
        self.ws.as_ref().and_then(|w| w.meta.active_env.clone())
    }

    pub fn do_send(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let env = self.env_name();
        let Some(open) = &mut self.open else {
            self.err("No request open");
            return;
        };
        open.sync();
        let vars = ws.resolved_vars(env.as_deref());
        let req = ws.effective(&open.req);
        match http::prepare(&req, &vars) {
            Ok(mut prepared) => {
                self.sending = true;
                self.error = None;
                let oauth = match &req.auth {
                    Auth::Oauth2(o) => Some(crate::oauth::resolve(o, &vars)),
                    _ => None,
                };
                let (tx, jar, timeout, tokens) = (
                    self.tx.clone(),
                    self.jar.clone(),
                    ws.meta.timeout_secs,
                    self.tokens.clone(),
                );
                self.rt.spawn(async move {
                    if let Some(o) = oauth {
                        let prompt = |url: &str| {
                            let _ = tx.send(Msg::Info(format!(
                                "Sign in in your browser (opened automatically): {url}"
                            )));
                            crate::oauth::open_browser(url);
                        };
                        match crate::oauth::bearer(&o, &tokens, prompt).await {
                            Ok(t) => http::add_bearer(&mut prepared.headers, &t),
                            Err(e) => {
                                let result = Err(format!("{e:#}"));
                                let _ = tx.send(Msg::Response { prepared, result });
                                return;
                            }
                        }
                    }
                    let result = http::send(prepared.clone(), jar, timeout)
                        .await
                        .map_err(|e| format!("{e:#}"));
                    let _ = tx.send(Msg::Response { prepared, result });
                });
            }
            Err(e) => {
                self.error = Some(format!("{e:#}"));
                self.focus = Focus::Response;
            }
        }
    }

    pub fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Response { prepared, result } => {
                self.sending = false;
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let mut entry = serde_json::json!({"ts": ts, "method": prepared.method.as_str(), "url": prepared.url});
                match result {
                    Ok(r) => {
                        entry["status"] = r.status.to_string().into();
                        entry["ms"] = (r.elapsed.as_millis() as u64).into();
                        self.set_response(r);
                    }
                    Err(e) => {
                        self.resp = None;
                        self.error = Some(e);
                    }
                }
                if let Some(ws) = &self.ws {
                    ws.push_history(&entry);
                    self.history = ws.history(50);
                }
            }
            Msg::Imported(Ok(dir)) => match Workspace::open(&dir) {
                Ok(ws) => {
                    let n = ws.meta.name.clone();
                    self.set_workspace(ws);
                    self.info(format!("Imported into workspace '{n}' ({})", dir.display()));
                }
                Err(e) => self.err(e.to_string()),
            },
            Msg::Imported(Err(e)) => self.err(format!("Import failed: {e}")),
            Msg::Info(s) => self.info(s),
        }
    }

    fn set_response(&mut self, r: Response) {
        let (text, truncated) = r.text();
        let lang = Lang::detect(r.content_type(), &text);
        let pretty = Highlighter::prepare(lang, &text);
        let lines = self.hl.lines(lang, &pretty, &self.theme);
        self.resp = Some(RespView {
            status: r.status,
            reason: r.reason.clone(),
            elapsed_ms: r.elapsed.as_millis(),
            size: r.raw.len(),
            truncated,
            headers: r.headers.clone(),
            raw: r.raw,
            pretty,
            lang,
            lines,
            matches: vec![],
            cur_match: 0,
        });
        self.resp_scroll = 0;
        self.resp_tab = RespTab::Body;
    }

    pub fn set_theme(&mut self, name: &str) {
        match Theme::load(name) {
            Ok(mut t) => {
                t.downgrade_if_needed();
                self.theme = t;
                if let Some(r) = &mut self.resp {
                    r.lines = self.hl.lines(r.lang, &r.pretty, &self.theme);
                }
                self.info(format!("Theme: {name}"));
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    // ---- input ----

    /// Cursor blink phase; any key press restarts it solid.
    pub fn cursor_on(&self) -> bool {
        (self.blink.elapsed().as_millis() / 530) & 1 == 0
    }

    /// Refresh the variable values shown inline in editors.
    pub fn refresh_vars(&mut self) {
        self.var_cache = match &self.ws {
            Some(ws) => ws.resolved_vars(ws.meta.active_env.as_deref()),
            None => BTreeMap::new(),
        };
    }

    pub fn on_key(&mut self, ke: KeyEvent) {
        self.blink = Instant::now();
        self.dispatch_key(ke);
        self.refresh_vars();
    }

    fn dispatch_key(&mut self, ke: KeyEvent) {
        if self.mode != Mode::Edit {
            self.status = None;
        }
        match self.mode {
            Mode::Command | Mode::Search => self.popup_key(ke),
            Mode::Confirm => self.confirm_key(ke),
            Mode::Help => self.mode = Mode::Normal,
            Mode::Picker => self.picker_key(ke),
            Mode::Vars => self.vars_key(ke),
            Mode::Edit => self.edit_key(ke),
            Mode::Normal => self.normal_key(ke),
        }
    }

    pub fn on_paste(&mut self, s: &str) {
        match self.mode {
            Mode::Command | Mode::Search => {
                self.input.push_str(s.lines().next().unwrap_or(""));
                if self.mode == Mode::Search {
                    self.apply_search();
                }
            }
            Mode::Vars => {
                if let Some(v) = &mut self.vars {
                    let ev = crossterm::event::Event::Paste(s.to_string());
                    self.handler.on_event(ev, &mut v.ed);
                }
            }
            Mode::Edit => {
                let ev = crossterm::event::Event::Paste(s.to_string());
                self.with_editor(|h, ed| h.on_event(ev, ed));
                self.after_edit();
            }
            _ => {}
        }
    }

    fn normal_key(&mut self, ke: KeyEvent) {
        let k = Key::from(ke);
        self.pending.push(k);
        match self.keys.lookup(&self.pending) {
            Lookup::Prefix => {}
            Lookup::Action(a) => {
                self.pending.clear();
                self.act(a);
            }
            Lookup::None => {
                let retry = self.pending.len() > 1;
                self.pending.clear();
                if retry && let Lookup::Action(a) = self.keys.lookup(&[k]) {
                    self.act(a);
                }
            }
        }
    }

    #[cfg(test)]
    pub fn action_for_test(&mut self, a: Action) {
        self.act(a);
    }

    fn act(&mut self, a: Action) {
        match a {
            Action::Up
            | Action::Down
            | Action::Left
            | Action::Right
            | Action::Top
            | Action::Bottom
            | Action::HalfUp
            | Action::HalfDown => self.motion(a),
            Action::FocusLeft => self.focus = Focus::Tree,
            Action::FocusRight => {
                if self.focus == Focus::Tree {
                    self.focus = Focus::Request;
                }
            }
            Action::FocusUp => {
                if self.focus == Focus::Response {
                    self.focus = Focus::Request;
                }
            }
            Action::FocusDown => {
                if self.focus == Focus::Request {
                    self.focus = Focus::Response;
                }
            }
            Action::FocusNext => {
                self.focus = match self.focus {
                    Focus::Tree => Focus::Request,
                    Focus::Request => Focus::Response,
                    Focus::Response => Focus::Tree,
                }
            }
            Action::NextTab | Action::PrevTab => self.cycle_tab(a == Action::NextTab),
            Action::Open => self.open_selected(),
            Action::Edit => self.start_edit(),
            Action::Send => self.do_send(),
            Action::Save => self.save_current(),
            Action::Command => self.open_popup(Mode::Command, ""),
            Action::Search => self.open_popup(Mode::Search, ""),
            Action::SearchNext => self.search_step(true),
            Action::SearchPrev => self.search_step(false),
            Action::Cancel => self.cancel(),
            Action::Quit => self.try_quit(false),
            Action::Help => self.mode = Mode::Help,
            Action::Workspaces => self.open_picker(),
            Action::CollapseAll => self.set_all_expanded(false),
            Action::ExpandAll => self.set_all_expanded(true),
            Action::NewRequest => self.open_popup(Mode::Command, "new "),
            Action::DeleteRequest => self.ask_delete(),
            Action::CycleMethod => {
                if let Some(o) = &mut self.open {
                    o.req.method = o.req.method.next();
                }
            }
            Action::CycleKind => self.cycle_kind(),
            Action::NextEnv => self.cycle_env(),
            Action::CopyCurl => self.copy_curl(),
            Action::CopyResponse => self.copy_response(),
            Action::PaneWider | Action::PaneNarrower => {
                let w = (self.hit.side_edge.x + 1).saturating_sub(self.hit.main.x);
                let grow_tree = (self.focus == Focus::Tree) == (a == Action::PaneWider);
                self.set_side_w(if grow_tree {
                    w + 2
                } else {
                    w.saturating_sub(2)
                });
            }
            Action::PaneTaller | Action::PaneShorter => match self.focus {
                Focus::Tree => self.info("The sidebar spans the full height"),
                f => {
                    let up = (f == Focus::Request) == (a == Action::PaneTaller);
                    self.split = (if up {
                        self.split + 4
                    } else {
                        self.split.saturating_sub(4)
                    })
                    .clamp(15, 85);
                }
            },
            Action::PaneReset => {
                self.side_w = None;
                self.split = 52;
            }
        }
    }

    fn cancel(&mut self) {
        if !self.filter.is_empty() {
            self.filter.clear();
            self.cursor = 0;
        } else if let Some(r) = &mut self.resp {
            r.matches.clear();
        }
    }

    pub fn try_quit(&mut self, force: bool) {
        if !force
            && self.open.as_mut().is_some_and(|o| {
                o.sync();
                o.dirty()
            })
        {
            self.err("Unsaved changes. :w to save, :q! to discard");
        } else {
            self.quit = true;
        }
    }

    fn motion(&mut self, a: Action) {
        let half = (self.resp_height / 2).max(1);
        match self.focus {
            Focus::Tree => self.tree_motion(a),
            Focus::Response => {
                let max = self.resp_len().saturating_sub(1);
                match a {
                    Action::Down => self.resp_scroll = (self.resp_scroll + 1).min(max),
                    Action::Up => self.resp_scroll = self.resp_scroll.saturating_sub(1),
                    Action::HalfDown => self.resp_scroll = (self.resp_scroll + half).min(max),
                    Action::HalfUp => self.resp_scroll = self.resp_scroll.saturating_sub(half),
                    Action::Top => self.resp_scroll = 0,
                    Action::Bottom => self.resp_scroll = max,
                    Action::Left | Action::Right => self.cycle_tab(a == Action::Right),
                    _ => {}
                }
            }
            Focus::Request => match a {
                Action::Down => self.req_field = ReqField::Content,
                Action::Up => self.req_field = ReqField::Url,
                Action::Left | Action::Right => self.cycle_tab(a == Action::Right),
                _ => {}
            },
        }
    }

    pub fn resp_len(&self) -> usize {
        match (&self.resp, self.resp_tab) {
            (Some(r), RespTab::Body) => r.lines.len(),
            (Some(r), RespTab::Headers) => r.headers.len(),
            _ => 0,
        }
    }

    fn tree_motion(&mut self, a: Action) {
        let rows = self.rows();
        let max = rows.len().saturating_sub(1);
        match a {
            Action::Down => self.cursor = (self.cursor + 1).min(max),
            Action::Up => self.cursor = self.cursor.saturating_sub(1),
            Action::HalfDown => self.cursor = (self.cursor + 8).min(max),
            Action::HalfUp => self.cursor = self.cursor.saturating_sub(8),
            Action::Top => self.cursor = 0,
            Action::Bottom => self.cursor = max,
            Action::Right => match rows.get(self.cursor).map(|r| r.kind.clone()) {
                Some(RowKind::Dir { path, open: false }) => {
                    self.collapsed.remove(&path);
                }
                Some(RowKind::HistDir { open: false }) => self.hist_open = true,
                Some(RowKind::Req { .. }) => self.open_selected(),
                Some(RowKind::Workspace {
                    path, open: false, ..
                }) => self.expand_workspace(&path, true),
                _ => {}
            },
            Action::Left => match rows.get(self.cursor).map(|r| r.kind.clone()) {
                Some(RowKind::Dir { path, open: true }) => {
                    self.collapsed.insert(path);
                }
                Some(RowKind::HistDir { open: true }) => self.hist_open = false,
                Some(RowKind::Workspace {
                    path, open: true, ..
                }) => self.expand_workspace(&path, false),
                _ => {
                    // Jump to the parent row.
                    if let Some(d) = rows.get(self.cursor).map(|r| r.depth).filter(|d| *d > 0)
                        && let Some(i) = rows[..self.cursor].iter().rposition(|r| r.depth < d)
                    {
                        self.cursor = i;
                    }
                }
            },
            _ => {}
        }
    }

    fn open_selected(&mut self) {
        match self.focus {
            Focus::Request => return self.do_send(),
            Focus::Response => return,
            Focus::Tree => {}
        }
        let rows = self.rows();
        match rows.get(self.cursor).map(|r| r.kind.clone()) {
            Some(RowKind::Dir { path, open }) => {
                if open {
                    self.collapsed.insert(path);
                } else {
                    self.collapsed.remove(&path);
                }
            }
            Some(RowKind::HistDir { open }) => self.hist_open = !open,
            Some(RowKind::Workspace { path, open, active }) => {
                // Collapsed: expand and activate. Expanded but idle: activate. Active: collapse.
                if open && active {
                    self.expand_workspace(&path, false);
                } else {
                    self.expand_workspace(&path, true);
                    self.activate_workspace(&path);
                    self.cursor_to_workspace(&path);
                }
            }
            Some(RowKind::Hist(i)) => {
                if let Some(h) = self.history.get(i) {
                    let s = format!(
                        "{} {} {} ({} ms)",
                        h["method"].as_str().unwrap_or(""),
                        h["status"].as_str().unwrap_or("ERR"),
                        h["url"].as_str().unwrap_or(""),
                        h["ms"].as_u64().unwrap_or(0)
                    );
                    self.info(s);
                }
            }
            Some(RowKind::Req { path, .. }) => {
                self.load_request(&path);
                if self.open.as_ref().and_then(|o| o.path.as_deref()) == Some(path.as_path()) {
                    self.do_send();
                }
            }
            None => {}
        }
    }

    fn cycle_tab(&mut self, fwd: bool) {
        fn step<T: Copy + PartialEq>(all: &[T], cur: T, fwd: bool) -> T {
            let i = all.iter().position(|t| *t == cur).unwrap_or(0);
            all[if fwd {
                (i + 1) % all.len()
            } else {
                (i + all.len() - 1) % all.len()
            }]
        }
        match self.focus {
            Focus::Request => self.req_tab = step(&ReqTab::ALL, self.req_tab, fwd),
            Focus::Response => {
                self.resp_tab = step(&[RespTab::Body, RespTab::Headers], self.resp_tab, fwd);
                self.resp_scroll = 0;
            }
            Focus::Tree => {}
        }
    }

    fn cycle_kind(&mut self) {
        let tab = self.req_tab;
        let Some(o) = &mut self.open else { return };
        o.sync();
        match tab {
            ReqTab::Auth => {
                o.req.auth = o.req.auth.cycle();
                o.ed.auth = editor(&fmt_auth(&o.req.auth));
            }
            ReqTab::Body => o.req.body.kind = o.req.body.kind.next(),
            _ => {}
        }
    }

    fn cycle_env(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let envs = ws.envs().unwrap_or_default();
        if envs.is_empty() {
            return self.err("No environments");
        }
        let cur = self.env_name();
        let i = cur
            .and_then(|c| envs.iter().position(|e| *e == c))
            .map(|i| (i + 1) % envs.len())
            .unwrap_or(0);
        self.set_env(&envs[i].clone());
    }

    pub fn set_env(&mut self, name: &str) {
        let Some(ws) = &mut self.ws else { return };
        ws.meta.active_env = Some(name.to_string());
        match ws.save_meta() {
            Ok(()) => self.info(format!("Environment: {name}")),
            Err(e) => self.err(e.to_string()),
        }
    }

    fn set_side_w(&mut self, w: u16) {
        let max = self.hit.main.width.saturating_sub(20).max(12);
        self.side_w = Some(w.clamp(12, max));
    }

    /// Resize a pane while its border is being dragged.
    fn drag_to(&mut self, p: Position) {
        match self.drag {
            Some(Drag::Side) => self.set_side_w((p.x + 1).saturating_sub(self.hit.main.x)),
            Some(Drag::Split) => {
                let r = self.hit.right;
                if r.height > 0 {
                    let rows = (p.y + 1).saturating_sub(r.y) as u32;
                    self.split = ((rows * 100 / r.height as u32) as u16).clamp(15, 85);
                }
            }
            None => {}
        }
    }

    fn copy_response(&mut self) {
        let Some(r) = &self.resp else {
            return self.err("No response yet");
        };
        let text = match self.resp_tab {
            RespTab::Headers => r
                .headers
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect::<Vec<_>>()
                .join("\n"),
            RespTab::Body => r.pretty.clone(),
        };
        let what = if self.resp_tab == RespTab::Headers {
            "headers"
        } else {
            "body"
        };
        copy_to_clipboard(&text);
        self.info(format!("Copied response {what} ({} bytes)", text.len()));
    }

    fn copy_curl(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let env = self.env_name();
        let Some(open) = &mut self.open else {
            return self.err("No request open");
        };
        open.sync();
        let vars = ws.resolved_vars(env.as_deref());
        match http::prepare(&ws.effective(&open.req), &vars) {
            Ok(p) => {
                copy_to_clipboard(&crate::curl::to_curl(&p));
                self.info("Copied curl command to clipboard");
            }
            Err(e) => self.err(format!("{e:#}")),
        }
    }

    fn ask_delete(&mut self) {
        let rows = self.rows();
        if let Some(Row {
            kind: RowKind::Req { .. },
            name,
            ..
        }) = rows.get(self.cursor)
        {
            self.confirm = Some(format!("Delete request '{name}'?"));
            self.mode = Mode::Confirm;
        }
    }

    /// Popup listing the known workspaces (registry plus the current one).
    pub fn open_picker(&mut self) {
        let mut paths = registry::load();
        if let Some(ws) = &self.ws
            && !paths.iter().any(|p| same_dir(p, &ws.root))
        {
            paths.push(ws.root.clone());
        }
        let items: Vec<(String, PathBuf)> = paths
            .into_iter()
            .map(|p| {
                let name = Workspace::open(&p)
                    .map(|w| w.meta.name)
                    .unwrap_or_else(|_| "?".into());
                (name, p)
            })
            .collect();
        if items.is_empty() {
            return self.err("No workspaces yet (:wsnew <path> or turly init <path>)");
        }
        let sel = self
            .ws
            .as_ref()
            .and_then(|w| items.iter().position(|(_, p)| same_dir(p, &w.root)))
            .unwrap_or(0);
        self.picker = Some(Picker {
            items,
            sel,
            offset: 0,
        });
        self.mode = Mode::Picker;
    }

    fn picker_key(&mut self, ke: KeyEvent) {
        let Some(p) = &mut self.picker else {
            self.mode = Mode::Normal;
            return;
        };
        let last = p.items.len().saturating_sub(1);
        match ke.code {
            KeyCode::Char('j') | KeyCode::Down => p.sel = (p.sel + 1).min(last),
            KeyCode::Char('k') | KeyCode::Up => p.sel = p.sel.saturating_sub(1),
            KeyCode::Char('g') | KeyCode::Home => p.sel = 0,
            KeyCode::Char('G') | KeyCode::End => p.sel = last,
            KeyCode::Enter => self.pick(None),
            KeyCode::Esc | KeyCode::Char('q' | 'W') => {
                self.picker = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn picker_mouse(&mut self, me: MouseEvent) {
        let (r, Some(p)) = (self.hit.picker, &mut self.picker) else {
            return;
        };
        let pos = Position::new(me.column, me.row);
        match me.kind {
            MouseEventKind::ScrollUp => p.sel = p.sel.saturating_sub(1),
            MouseEventKind::ScrollDown => p.sel = (p.sel + 1).min(p.items.len().saturating_sub(1)),
            MouseEventKind::Down(MouseButton::Left) if r.contains(pos) => {
                let i = p.offset + (pos.y - r.y) as usize;
                if i < p.items.len() {
                    self.pick(Some(i));
                }
            }
            MouseEventKind::Down(_) if !r.contains(pos) => {
                self.picker = None;
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    fn pick(&mut self, i: Option<usize>) {
        let Some(p) = self.picker.take() else { return };
        self.mode = Mode::Normal;
        if let Some((_, path)) = p.items.get(i.unwrap_or(p.sel)) {
            self.switch_workspace(path);
        }
    }

    fn confirm_key(&mut self, ke: KeyEvent) {
        self.mode = Mode::Normal;
        if ke.code != KeyCode::Char('y') {
            return;
        }
        let rows = self.rows();
        if let Some(Row {
            kind: RowKind::Req { path, .. },
            ..
        }) = rows.get(self.cursor)
        {
            match std::fs::remove_file(path) {
                Ok(()) => {
                    if self.open.as_ref().and_then(|o| o.path.as_deref()) == Some(path.as_path()) {
                        self.open = None;
                    }
                    self.reload_tree();
                    self.info("Deleted");
                }
                Err(e) => self.err(e.to_string()),
            }
        }
    }

    // ---- editing ----

    pub fn cur_editor(&mut self) -> Option<&mut EditorState> {
        let (field, tab) = (self.req_field, self.req_tab);
        let o = self.open.as_mut()?;
        match field {
            ReqField::Url => Some(&mut o.ed.url),
            ReqField::Content => o.ed.tab_mut(tab),
        }
    }

    fn with_editor<R>(
        &mut self,
        f: impl FnOnce(&mut EditorEventHandler, &mut EditorState) -> R,
    ) -> Option<R> {
        let (field, tab) = (self.req_field, self.req_tab);
        let o = self.open.as_mut()?;
        let ed = match field {
            ReqField::Url => Some(&mut o.ed.url),
            ReqField::Content => o.ed.tab_mut(tab),
        }?;
        Some(f(&mut self.handler, ed))
    }

    fn start_edit(&mut self) {
        if self.focus != Focus::Request {
            return self.err("Focus the request pane first (Ctrl-w l)");
        }
        if self.open.is_none() {
            return self.err("No request open. Press o to create one");
        }
        match self.cur_editor() {
            Some(ed) => {
                ed.mode = EditorMode::Insert;
                self.mode = Mode::Edit;
            }
            None => self.err("This tab is read-only"),
        }
    }

    fn after_edit(&mut self) {
        if let Some(o) = &mut self.open {
            o.sync();
        }
    }

    fn edit_key(&mut self, ke: KeyEvent) {
        let key = Key::from(ke);
        if key.ctrl {
            match self.keys.single(key) {
                Some(Action::Send) => {
                    self.after_edit();
                    return self.do_send();
                }
                Some(Action::Save) => return self.save_current(),
                _ => {}
            }
        }
        let editing_url = self.req_field == ReqField::Url;
        let Some(ed) = self.cur_editor() else {
            self.mode = Mode::Normal;
            return;
        };
        let em = ed.mode;
        if em == EditorMode::Normal && self.vim.pending() {
            if ke.code == KeyCode::Esc {
                self.vim.reset();
            } else {
                let mut vim = std::mem::take(&mut self.vim);
                self.with_editor(|h, ed| vim.key(h, ed, ke));
                self.vim = vim;
                self.after_edit();
            }
            return;
        }
        if em == EditorMode::Normal {
            match ke.code {
                KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    return self.after_edit();
                }
                KeyCode::Char(':') => return self.open_popup(Mode::Command, ""),
                KeyCode::Char('/') => return self.open_popup(Mode::Search, ""),
                _ => {}
            }
        }
        if editing_url && em == EditorMode::Insert && ke.code == KeyCode::Enter {
            self.mode = Mode::Normal;
            self.after_edit();
            return self.do_send();
        }
        let mut vim = std::mem::take(&mut self.vim);
        self.with_editor(|h, ed| {
            if em != EditorMode::Normal || !vim.key(h, ed, ke) {
                h.on_key_event(ke, ed);
            }
        });
        self.vim = vim;
        self.after_edit();
    }

    // ---- mouse ----

    pub fn on_mouse(&mut self, me: MouseEvent) {
        match self.mode {
            Mode::Vars => {
                if let Some(v) = &mut self.vars {
                    self.handler.on_mouse_event(me, &mut v.ed);
                }
            }
            Mode::Help => {
                if matches!(me.kind, MouseEventKind::Down(_)) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::Picker => self.picker_mouse(me),
            Mode::Normal | Mode::Edit => {
                self.blink = Instant::now();
                let p = Position::new(me.column, me.row);
                match me.kind {
                    MouseEventKind::Down(MouseButton::Left) if self.hit.side_edge.contains(p) => {
                        self.drag = Some(Drag::Side)
                    }
                    MouseEventKind::Down(MouseButton::Left) if self.hit.split_edge.contains(p) => {
                        self.drag = Some(Drag::Split)
                    }
                    MouseEventKind::Drag(MouseButton::Left) if self.drag.is_some() => {
                        self.drag_to(p)
                    }
                    MouseEventKind::Up(MouseButton::Left) if self.drag.is_some() => {
                        self.drag = None
                    }
                    MouseEventKind::Down(MouseButton::Left) => self.click(p, me),
                    MouseEventKind::ScrollUp => self.wheel(p, me, false),
                    MouseEventKind::ScrollDown => self.wheel(p, me, true),
                    MouseEventKind::Drag(MouseButton::Left)
                    | MouseEventKind::Up(MouseButton::Left)
                        if self.mode == Mode::Edit =>
                    {
                        self.with_editor(|h, ed| h.on_mouse_event(me, ed));
                        self.after_edit();
                    }
                    _ => {}
                }
                self.refresh_vars();
            }
            _ => {}
        }
    }

    fn leave_edit(&mut self) {
        if self.mode == Mode::Edit {
            self.mode = Mode::Normal;
            self.after_edit();
        }
    }

    fn click(&mut self, p: Position, me: MouseEvent) {
        let h = &self.hit;
        let tab = h.tabs.iter().find(|(r, _)| r.contains(p)).map(|(_, t)| *t);
        let resp_tab = h
            .resp_tabs
            .iter()
            .find(|(r, _)| r.contains(p))
            .map(|(_, t)| *t);
        let (in_tree, in_url, in_content, in_resp) = (
            h.tree.contains(p),
            h.url.contains(p),
            h.content.contains(p),
            h.resp.contains(p),
        );
        self.status = None;
        if in_tree {
            self.leave_edit();
            self.focus = Focus::Tree;
            let i = self.tree_offset + (p.y - self.hit.tree.y) as usize;
            let rows = self.rows();
            let Some(row) = rows.get(i) else { return };
            self.cursor = i;
            match &row.kind {
                RowKind::Req { path, .. } => self.load_request(path),
                _ => self.open_selected(),
            }
        } else if let Some(t) = tab {
            self.leave_edit();
            self.focus = Focus::Request;
            self.req_field = ReqField::Content;
            self.req_tab = t;
        } else if in_url || in_content {
            self.focus = Focus::Request;
            self.req_field = if in_url {
                ReqField::Url
            } else {
                ReqField::Content
            };
            self.start_edit();
            self.with_editor(|h, ed| h.on_mouse_event(me, ed));
            self.after_edit();
        } else if in_resp {
            self.leave_edit();
            self.focus = Focus::Response;
            if let Some(t) = resp_tab {
                self.resp_tab = t;
                self.resp_scroll = 0;
            }
        } else if self.hit.req.contains(p) {
            self.leave_edit();
            self.focus = Focus::Request;
        }
    }

    fn wheel(&mut self, p: Position, me: MouseEvent, down: bool) {
        const STEP: usize = 3;
        if self.hit.tree.contains(p) {
            let max = self.rows().len().saturating_sub(1);
            self.cursor = if down {
                (self.cursor + STEP).min(max)
            } else {
                self.cursor.saturating_sub(STEP)
            };
        } else if self.hit.resp.contains(p) {
            let max = self.resp_len().saturating_sub(1);
            self.resp_scroll = if down {
                (self.resp_scroll + STEP).min(max)
            } else {
                self.resp_scroll.saturating_sub(STEP)
            };
        } else if self.hit.content.contains(p) {
            let tab = self.req_tab;
            if let Some(o) = &mut self.open
                && let Some(ed) = o.ed.tab_mut(tab)
            {
                self.handler.on_mouse_event(me, ed);
            }
        }
    }

    // ---- :vars editor ----

    pub fn open_vars(&mut self) {
        let Some(ws) = self.need_ws() else { return };
        let Some(env) = self.env_name() else {
            return self.err("No active environment (:envnew <name>)");
        };
        match ws.vars_text(&env) {
            Ok(text) => {
                self.vars = Some(VarsEd {
                    env,
                    ed: editor(&text),
                    orig: text,
                    armed: false,
                });
                self.mode = Mode::Vars;
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    fn vars_key(&mut self, ke: KeyEvent) {
        let Some(v) = &mut self.vars else {
            self.mode = Mode::Normal;
            return;
        };
        let key = Key::from(ke);
        if key.ctrl && self.keys.single(key) == Some(Action::Save) {
            return self.save_vars(false);
        }
        let normal = v.ed.mode == EditorMode::Normal;
        if normal && self.vim.pending() {
            if ke.code == KeyCode::Esc {
                self.vim.reset();
            } else {
                self.vim.key(&mut self.handler, &mut v.ed, ke);
            }
            return;
        }
        if normal {
            match ke.code {
                KeyCode::Esc => return self.close_vars(false),
                KeyCode::Char(':') => return self.open_popup(Mode::Command, ""),
                _ => {}
            }
        }
        v.armed = false;
        if !normal || !self.vim.key(&mut self.handler, &mut v.ed, ke) {
            self.handler.on_key_event(ke, &mut v.ed);
        }
    }

    /// Write the `:vars` buffer to disk; `close` also leaves the popup.
    pub fn save_vars(&mut self, close: bool) {
        let (Some(v), Some(ws)) = (&mut self.vars, &self.ws) else {
            return;
        };
        let text = text_of(&v.ed);
        match ws.save_vars_text(&v.env, &text) {
            Ok(()) => {
                let env = v.env.clone();
                v.orig = text;
                if close {
                    self.vars = None;
                    self.mode = Mode::Normal;
                }
                self.info(format!("Saved variables for '{env}'"));
            }
            Err(e) => self.err(e.to_string()),
        }
    }

    /// Leave `:vars`; unsaved edits need `force` or a second request.
    pub fn close_vars(&mut self, force: bool) {
        if let Some(v) = &mut self.vars
            && !force
            && !v.armed
            && text_of(&v.ed) != v.orig
        {
            v.armed = true;
            return self.err("Unsaved changes: Ctrl-s or :w saves, :q! or Esc again discards");
        }
        self.vars = None;
        self.mode = Mode::Normal;
    }

    // ---- popups: command & search ----

    pub fn open_popup(&mut self, mode: Mode, prefill: &str) {
        self.return_to = match self.mode {
            Mode::Edit | Mode::Vars => self.mode,
            _ => Mode::Normal,
        };
        self.mode = mode;
        self.input = prefill.to_string();
        self.sel = 0;
        if mode == Mode::Search {
            self.search_focus = if self.return_to == Mode::Edit {
                Focus::Request
            } else {
                self.focus
            };
            if self.search_focus == Focus::Request
                && let Some(ed) = self.cur_editor()
            {
                ed.execute(StartSearch);
            }
        }
    }

    fn close_popup(&mut self) {
        self.mode = self.return_to;
        self.input.clear();
    }

    fn popup_key(&mut self, ke: KeyEvent) {
        let ctrl = ke
            .modifiers
            .contains(crossterm::event::KeyModifiers::CONTROL);
        let searching = self.mode == Mode::Search;
        match ke.code {
            KeyCode::Esc => {
                if searching {
                    self.input.clear();
                    self.apply_search();
                    if self.search_focus == Focus::Request
                        && let Some(ed) = self.cur_editor()
                    {
                        ed.execute(StopSearch);
                    }
                }
                self.close_popup();
            }
            KeyCode::Enter => {
                if searching {
                    if self.search_focus == Focus::Request
                        && let Some(ed) = self.cur_editor()
                    {
                        ed.execute(SelectCurrentSearch);
                    }
                    self.mode = self.return_to;
                } else {
                    let line = self.resolve_command_line();
                    self.close_popup();
                    self.run_command(&line);
                }
            }
            KeyCode::Tab => self.complete(),
            KeyCode::Up => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down => self.sel += 1,
            KeyCode::Char('p') if ctrl => self.sel = self.sel.saturating_sub(1),
            KeyCode::Char('n') if ctrl => self.sel += 1,
            KeyCode::Char('u') if ctrl => {
                self.input.clear();
                self.apply_search_if();
            }
            KeyCode::Backspace => {
                if self.input.pop().is_none() && !searching {
                    self.close_popup();
                } else {
                    self.sel = 0;
                    self.apply_search_if();
                }
            }
            KeyCode::Char(c) if !ctrl => {
                self.input.push(c);
                self.sel = 0;
                self.apply_search_if();
            }
            _ => {}
        }
    }

    fn apply_search_if(&mut self) {
        if self.mode == Mode::Search {
            self.apply_search();
        }
    }

    /// Live search over the pane that had focus when `/` was pressed.
    fn apply_search(&mut self) {
        match self.search_focus {
            Focus::Tree => {
                self.filter = self.input.clone();
                self.cursor = 0;
            }
            Focus::Response => {
                let q = self.input.to_lowercase();
                if let Some(r) = &mut self.resp {
                    r.matches.clear();
                    r.cur_match = 0;
                    if !q.is_empty() && self.resp_tab == RespTab::Body {
                        let qlen = q.chars().count();
                        for (li, l) in r.pretty.lines().enumerate() {
                            let lower: Vec<char> = l.to_lowercase().chars().collect();
                            if lower.len() != l.chars().count() {
                                continue;
                            }
                            let qc: Vec<char> = q.chars().collect();
                            let mut i = 0;
                            while i + qlen <= lower.len() {
                                if lower[i..i + qlen] == qc[..] {
                                    r.matches.push((li, i, qlen));
                                    i += qlen;
                                } else {
                                    i += 1;
                                }
                            }
                        }
                    }
                }
                self.jump_match();
            }
            Focus::Request => {
                let input = self.input.clone();
                if let Some(ed) = self.cur_editor() {
                    ed.execute(StopSearch);
                    ed.execute(StartSearch);
                    for c in input.chars() {
                        ed.execute(AppendCharToSearch(c));
                    }
                }
            }
        }
    }

    fn jump_match(&mut self) {
        if let Some(r) = &self.resp
            && let Some((line, _, _)) = r.matches.get(r.cur_match)
        {
            self.resp_scroll = line.saturating_sub(self.resp_height / 2);
        }
    }

    fn search_step(&mut self, fwd: bool) {
        match self.focus {
            Focus::Response => {
                if let Some(r) = &mut self.resp
                    && !r.matches.is_empty()
                {
                    let n = r.matches.len();
                    r.cur_match = if fwd {
                        (r.cur_match + 1) % n
                    } else {
                        (r.cur_match + n - 1) % n
                    };
                }
                self.jump_match();
            }
            Focus::Request => {
                if let Some(ed) = self.cur_editor() {
                    if fwd {
                        ed.execute(FindNext)
                    } else {
                        ed.execute(FindPrevious)
                    }
                }
            }
            Focus::Tree => {}
        }
    }

    pub fn mode_label(&mut self) -> &'static str {
        match self.mode {
            Mode::Command => "COMMAND",
            Mode::Search => "SEARCH",
            Mode::Confirm => "CONFIRM",
            Mode::Help => "HELP",
            Mode::Picker => "WORKSPACES",
            Mode::Vars => match self.vars.as_ref().map(|v| v.ed.mode) {
                Some(EditorMode::Insert) => "VARS INSERT",
                Some(EditorMode::Visual) => "VARS VISUAL",
                _ => "VARS",
            },
            Mode::Normal => "NORMAL",
            Mode::Edit => match self.cur_editor().map(|e| e.mode) {
                Some(EditorMode::Insert) => "INSERT",
                Some(EditorMode::Visual) => "VISUAL",
                _ => "EDIT",
            },
        }
    }
}

/// Copy via a platform clipboard tool, falling back to OSC 52.
pub fn same_dir(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

pub fn copy_to_clipboard(text: &str) {
    use std::io::Write;
    use std::process::{Command, Stdio};
    for (bin, args) in [
        ("pbcopy", vec![]),
        ("wl-copy", vec![]),
        ("xclip", vec!["-selection", "clipboard"]),
        ("xsel", vec!["-ib"]),
        ("clip.exe", vec![]),
    ] {
        if let Ok(mut c) = Command::new(bin)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            if let Some(mut si) = c.stdin.take() {
                let _ = si.write_all(text.as_bytes());
            }
            if c.wait().is_ok_and(|s| s.success()) {
                return;
            }
        }
    }
    use base64::Engine;
    let b = base64::engine::general_purpose::STANDARD.encode(text);
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{b}\x07");
    let _ = out.flush();
}

impl App {
    /// Hand the focused editor to `$EDITOR` when edtui requested it (Ctrl-e).
    pub fn run_system_editor(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        if self.mode != Mode::Edit {
            return Ok(());
        }
        if let Some(ed) = self.cur_editor()
            && edtui::actions::system_editor::is_pending(ed)
        {
            edtui::actions::system_editor::open(ed, terminal)?;
            crossterm::execute!(std::io::stdout(), crossterm::event::EnableBracketedPaste)?;
            self.after_edit();
        }
        Ok(())
    }
}
