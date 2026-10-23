//! A workspace is a plain directory: `turly.yaml`, `requests/**.yaml`, `environments/*.yaml`.

use crate::model::{Auth, Env, Meta, Request};
use anyhow::{Context, Result, bail};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const META_FILE: &str = "turly.yaml";

#[derive(Clone, Debug)]
pub enum Node {
    Dir {
        name: String,
        path: PathBuf,
        children: Vec<Node>,
    },
    Req {
        path: PathBuf,
        req: Box<Request>,
    },
}

impl Node {
    pub fn name(&self) -> &str {
        match self {
            Node::Dir { name, .. } => name,
            Node::Req { req, .. } => &req.name,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub meta: Meta,
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() {
        "untitled".into()
    } else {
        out
    }
}

fn read_yaml<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T> {
    let s = fs::read_to_string(p).with_context(|| format!("reading {}", p.display()))?;
    serde_yaml_ng::from_str(&s).with_context(|| format!("parsing {}", p.display()))
}

fn write_yaml<T: serde::Serialize>(p: &Path, v: &T) -> Result<()> {
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(p, serde_yaml_ng::to_string(v)?).with_context(|| format!("writing {}", p.display()))
}

impl Workspace {
    pub fn is_workspace(root: &Path) -> bool {
        root.join(META_FILE).is_file()
    }

    /// Create a workspace skeleton (idempotent on an existing directory without `turly.yaml`).
    pub fn init(root: &Path, name: &str) -> Result<Workspace> {
        if Self::is_workspace(root) {
            bail!("{} is already a Turly workspace", root.display());
        }
        fs::create_dir_all(root.join("requests"))?;
        fs::create_dir_all(root.join("environments"))?;
        fs::create_dir_all(root.join(".turly"))?;
        let gi = root.join(".gitignore");
        let mut existing = fs::read_to_string(&gi).unwrap_or_default();
        if !existing.lines().any(|l| l.trim() == ".turly/") {
            if !existing.is_empty() && !existing.ends_with('\n') {
                existing.push('\n');
            }
            existing.push_str(".turly/\n");
            fs::write(&gi, existing)?;
        }
        let meta = Meta {
            name: name.into(),
            timeout_secs: 30,
            active_env: None,
            auth: Default::default(),
        };
        write_yaml(&root.join(META_FILE), &meta)?;
        let mut ws = Workspace {
            root: root.to_path_buf(),
            meta,
        };
        if ws.envs()?.is_empty() {
            let mut env = Env::default();
            env.vars
                .insert("base_url".into(), "http://localhost:8080".into());
            ws.save_env("default", &env)?;
            ws.meta.active_env = Some("default".into());
            ws.save_meta()?;
        }
        Ok(ws)
    }

    pub fn open(root: &Path) -> Result<Workspace> {
        let meta = read_yaml(&root.join(META_FILE))?;
        Ok(Workspace {
            root: root.to_path_buf(),
            meta,
        })
    }

    pub fn save_meta(&self) -> Result<()> {
        write_yaml(&self.root.join(META_FILE), &self.meta)
    }

    /// `req` with `inherit` auth replaced by the workspace default.
    pub fn effective(&self, req: &Request) -> Request {
        let mut r = req.clone();
        if r.auth == Auth::Inherit {
            r.auth = match &self.meta.auth {
                Auth::Inherit => Auth::None,
                a => a.clone(),
            };
        }
        r
    }

    pub fn requests_dir(&self) -> PathBuf {
        self.root.join("requests")
    }

    pub fn tree(&self) -> Result<Vec<Node>> {
        scan(&self.requests_dir())
    }

    pub fn save_request(&self, path: &Path, req: &Request) -> Result<()> {
        write_yaml(path, req)
    }

    /// First free `<dir>/<slug>.yaml`.
    pub fn new_request_path(&self, dir: &Path, name: &str) -> PathBuf {
        let base = slug(name);
        let mut p = dir.join(format!("{base}.yaml"));
        let mut n = 2;
        while p.exists() {
            p = dir.join(format!("{base}-{n}.yaml"));
            n += 1;
        }
        p
    }

    pub fn envs(&self) -> Result<Vec<String>> {
        let mut out = vec![];
        if let Ok(rd) = fs::read_dir(self.root.join("environments")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "yaml") {
                    out.push(p.file_stem().unwrap().to_string_lossy().into_owned());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    fn env_path(&self, name: &str) -> PathBuf {
        self.root.join("environments").join(format!("{name}.yaml"))
    }

    fn secrets_path(&self) -> PathBuf {
        self.root.join(".turly").join("secrets.yaml")
    }

    pub fn load_env(&self, name: &str) -> Result<Env> {
        read_yaml(&self.env_path(name))
    }

    pub fn save_env(&self, name: &str, env: &Env) -> Result<()> {
        write_yaml(&self.env_path(name), env)
    }

    fn load_secrets(&self) -> BTreeMap<String, BTreeMap<String, String>> {
        read_yaml(&self.secrets_path()).unwrap_or_default()
    }

    /// Env vars with secret values overlaid.
    pub fn resolved_vars(&self, env: Option<&str>) -> BTreeMap<String, String> {
        let Some(name) = env else {
            return BTreeMap::new();
        };
        let mut vars = self.load_env(name).map(|e| e.vars).unwrap_or_default();
        if let Some(s) = self.load_secrets().remove(name) {
            vars.extend(s);
        }
        vars
    }

    pub fn set_var(&self, env: &str, key: &str, value: &str, secret: bool) -> Result<()> {
        let mut e = self.load_env(env)?;
        if secret {
            if !e.secrets.iter().any(|k| k == key) {
                e.secrets.push(key.into());
            }
            e.vars.remove(key);
            let mut all = self.load_secrets();
            all.entry(env.into())
                .or_default()
                .insert(key.into(), value.into());
            write_yaml(&self.secrets_path(), &all)?;
        } else {
            e.vars.insert(key.into(), value.into());
        }
        self.save_env(env, &e)
    }

    /// Env as editable text: `key=value` lines, `secret key=value` for secrets.
    pub fn vars_text(&self, env: &str) -> Result<String> {
        let e = self.load_env(env)?;
        let mut sec = self.load_secrets().remove(env).unwrap_or_default();
        let mut out: Vec<String> = e.vars.iter().map(|(k, v)| format!("{k}={v}")).collect();
        for k in &e.secrets {
            out.push(format!("secret {k}={}", sec.remove(k).unwrap_or_default()));
        }
        Ok(out.join("\n"))
    }

    /// Replace an env's variables from `vars_text` format. Blank and `#` lines are ignored.
    pub fn save_vars_text(&self, env: &str, text: &str) -> Result<()> {
        let mut e = self.load_env(env)?;
        let (mut vars, mut secrets) = (BTreeMap::new(), BTreeMap::new());
        for line in text.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (secret, kv) = match line.strip_prefix("secret ") {
                Some(r) => (true, r),
                None => (false, line),
            };
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            if k.is_empty() {
                continue;
            }
            if secret {
                secrets.insert(k, v);
            } else {
                vars.insert(k, v);
            }
        }
        e.vars = vars;
        e.secrets = secrets.keys().cloned().collect();
        self.save_env(env, &e)?;
        let mut all = self.load_secrets();
        all.insert(env.into(), secrets);
        write_yaml(&self.secrets_path(), &all)
    }

    pub fn history_path(&self) -> PathBuf {
        self.root.join(".turly").join("history.jsonl")
    }

    pub fn push_history(&self, line: &serde_json::Value) {
        use std::io::Write;
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.history_path())
        {
            let _ = writeln!(f, "{line}");
        }
    }

    pub fn history(&self, limit: usize) -> Vec<serde_json::Value> {
        let s = fs::read_to_string(self.history_path()).unwrap_or_default();
        let mut v: Vec<_> = s
            .lines()
            .filter_map(|l| serde_json::from_str(l).ok())
            .collect();
        v.reverse();
        v.truncate(limit);
        v
    }
}

fn scan(dir: &Path) -> Result<Vec<Node>> {
    let mut nodes: Vec<(Option<u32>, bool, Node)> = vec![];
    let Ok(rd) = fs::read_dir(dir) else {
        return Ok(vec![]);
    };
    for e in rd.flatten() {
        let p = e.path();
        let fname = e.file_name().to_string_lossy().into_owned();
        if fname.starts_with('.') {
            continue;
        }
        if p.is_dir() {
            let children = scan(&p)?;
            nodes.push((
                None,
                true,
                Node::Dir {
                    name: fname,
                    path: p,
                    children,
                },
            ));
        } else if p.extension().is_some_and(|x| x == "yaml") {
            // Skip unparsable files rather than failing the whole tree.
            if let Ok(req) = read_yaml::<Request>(&p) {
                nodes.push((
                    req.order,
                    false,
                    Node::Req {
                        path: p,
                        req: Box::new(req),
                    },
                ));
            }
        }
    }
    // Dirs first; then explicit `order`; then name.
    nodes.sort_by(|a, b| {
        b.1.cmp(&a.1)
            .then(a.0.unwrap_or(u32::MAX).cmp(&b.0.unwrap_or(u32::MAX)))
            .then(a.2.name().to_lowercase().cmp(&b.2.name().to_lowercase()))
    });
    Ok(nodes.into_iter().map(|n| n.2).collect())
}

/// Workspaces at `root` or in its subfolders (2 levels deep), including ones nested inside a workspace.
pub fn discover(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<PathBuf>) {
        let is_ws = Workspace::is_workspace(dir);
        if is_ws {
            out.push(dir.to_path_buf());
        }
        if depth == 0 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut subs: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.is_dir()
                    && !p.file_name().is_some_and(|n| {
                        let n = n.to_string_lossy();
                        n.starts_with('.')
                            || n == "target"
                            || n == "node_modules"
                            || (is_ws && (n == "requests" || n == "environments"))
                    })
            })
            .collect();
        subs.sort();
        for s in subs {
            walk(&s, depth - 1, out);
        }
    }
    let mut out = vec![];
    walk(root, 2, &mut out);
    out
}

/// Global workspace registry at `<config>/turly/workspaces.yaml`.
pub mod registry {
    use super::*;

    fn path() -> PathBuf {
        crate::config::config_dir().join("workspaces.yaml")
    }

    pub fn load() -> Vec<PathBuf> {
        read_yaml::<Vec<PathBuf>>(&path())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| Workspace::is_workspace(p))
            .collect()
    }

    pub fn add(root: &Path) -> Result<()> {
        let root = root.canonicalize()?;
        let mut all = load();
        all.retain(|p| *p != root);
        all.push(root);
        write_yaml(&path(), &all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_roundtrip_and_secrets() {
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::init(d.path(), "demo").unwrap();
        assert!(d.path().join(".gitignore").exists());
        let p = ws.new_request_path(&ws.requests_dir(), "Get Thing");
        ws.save_request(&p, &Request::new("Get Thing")).unwrap();
        assert_eq!(ws.tree().unwrap().len(), 1);
        ws.save_vars_text("default", "a=1\n# note\nsecret tok=x=y\n\nb = 2")
            .unwrap();
        assert_eq!(ws.vars_text("default").unwrap(), "a=1\nb=2\nsecret tok=x=y");
        ws.set_var("default", "token", "s3", true).unwrap();
        let v = ws.resolved_vars(Some("default"));
        assert_eq!(v["token"], "s3");
        assert!(
            !fs::read_to_string(ws.env_path("default"))
                .unwrap()
                .contains("s3")
        );
    }

    #[test]
    fn inherit_uses_workspace_auth() {
        let d = tempfile::tempdir().unwrap();
        let mut ws = Workspace::init(d.path(), "demo").unwrap();
        let req = Request::new("x");
        assert_eq!(ws.effective(&req).auth, Auth::None);
        ws.meta.auth = Auth::Oauth2(crate::model::OAuth {
            token_url: "https://t".into(),
            ..Default::default()
        });
        ws.save_meta().unwrap();
        let ws = Workspace::open(d.path()).unwrap();
        assert_eq!(ws.effective(&req).auth, ws.meta.auth);
        let mut own = req.clone();
        own.auth = Auth::None;
        assert_eq!(ws.effective(&own).auth, Auth::None);
    }

    #[test]
    fn discovers_nested_workspaces() {
        let d = tempfile::tempdir().unwrap();
        let r = d.path();
        for p in ["a", "b/c", "target/x", ".hidden/y", "d/e/f"] {
            Workspace::init(&r.join(p), "w").unwrap();
        }
        let got: Vec<_> = discover(r)
            .iter()
            .map(|p| p.strip_prefix(r).unwrap().to_path_buf())
            .collect();
        assert_eq!(got, [PathBuf::from("a"), PathBuf::from("b/c")]);
        assert_eq!(discover(&r.join("a")), [r.join("a")]);
        Workspace::init(&r.join("a/inner"), "i").unwrap();
        assert_eq!(discover(&r.join("a")), [r.join("a"), r.join("a/inner")]);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("GET /pet/{id}"), "get-pet-id");
        assert_eq!(slug("!!"), "untitled");
    }
}
