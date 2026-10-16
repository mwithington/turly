//! On-disk data model (YAML).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Method {
    pub const ALL: [Method; 7] = [
        Method::Get,
        Method::Post,
        Method::Put,
        Method::Patch,
        Method::Delete,
        Method::Head,
        Method::Options,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
        }
    }

    /// Short label used in the sidebar.
    pub fn short(self) -> &'static str {
        match self {
            Method::Delete => "DEL",
            Method::Options => "OPT",
            Method::Patch => "PTCH",
            m => m.as_str(),
        }
    }

    pub fn next(self) -> Method {
        let i = Method::ALL.iter().position(|m| *m == self).unwrap_or(0);
        Method::ALL[(i + 1) % Method::ALL.len()]
    }

    pub fn parse(s: &str) -> Option<Method> {
        Method::ALL
            .into_iter()
            .find(|m| m.as_str().eq_ignore_ascii_case(s))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Param {
    pub key: String,
    #[serde(default)]
    pub value: String,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[cfg(test)]
impl Param {
    pub fn new(key: &str, value: &str) -> Self {
        Param {
            key: key.into(),
            value: value.into(),
            enabled: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Auth {
    #[default]
    None,
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: String,
    },
    Apikey {
        key: String,
        value: String,
        #[serde(default)]
        query: bool,
    },
}

impl Auth {
    pub fn label(&self) -> &'static str {
        match self {
            Auth::None => "none",
            Auth::Bearer { .. } => "bearer",
            Auth::Basic { .. } => "basic",
            Auth::Apikey { .. } => "apikey",
        }
    }

    /// Cycle none -> bearer -> basic -> apikey.
    pub fn cycle(&self) -> Auth {
        match self {
            Auth::None => Auth::Bearer {
                token: "{{token}}".into(),
            },
            Auth::Bearer { .. } => Auth::Basic {
                username: String::new(),
                password: String::new(),
            },
            Auth::Basic { .. } => Auth::Apikey {
                key: "X-API-Key".into(),
                value: "{{api_key}}".into(),
                query: false,
            },
            Auth::Apikey { .. } => Auth::None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum BodyKind {
    #[default]
    None,
    Json,
    Text,
    /// `key=value` per line, sent url-encoded.
    Form,
}

impl BodyKind {
    pub fn next(self) -> BodyKind {
        match self {
            BodyKind::None => BodyKind::Json,
            BodyKind::Json => BodyKind::Text,
            BodyKind::Text => BodyKind::Form,
            BodyKind::Form => BodyKind::None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BodyKind::None => "none",
            BodyKind::Json => "json",
            BodyKind::Text => "text",
            BodyKind::Form => "form",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Body {
    #[serde(rename = "type", default)]
    pub kind: BodyKind,
    #[serde(default)]
    pub content: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Request {
    pub name: String,
    #[serde(default)]
    pub method: Method,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<Param>,
    #[serde(default)]
    pub auth: Auth,
    #[serde(default)]
    pub body: Body,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order: Option<u32>,
    #[serde(default = "yes")]
    pub follow_redirects: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub insecure: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl Request {
    pub fn new(name: &str) -> Self {
        Request {
            name: name.into(),
            method: Method::Get,
            url: "{{base_url}}/".into(),
            params: vec![],
            headers: vec![],
            auth: Auth::None,
            body: Body::default(),
            docs: None,
            order: None,
            follow_redirects: true,
            insecure: false,
        }
    }
}

/// Environment file: plain vars plus the names of vars whose values live in secrets.yaml.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Env {
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub secrets: Vec<String>,
}

/// `turly.yaml` at the workspace root.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Meta {
    pub name: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_env: Option<String>,
}

fn default_timeout() -> u64 {
    30
}
