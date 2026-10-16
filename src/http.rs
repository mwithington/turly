//! Request preparation (variable substitution, auth, body) and sending.

use crate::model::{Auth, BodyKind, Method, Request};
use crate::vars::substitute;
use anyhow::{Context, Result};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Bodies beyond this are truncated in the viewer (the full body stays available for `:save`).
pub const VIEW_LIMIT: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct Prepared {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<String>,
    pub follow_redirects: bool,
    pub insecure: bool,
}

#[derive(Clone, Debug)]
pub struct Response {
    pub status: u16,
    pub reason: String,
    pub headers: Vec<(String, String)>,
    pub raw: Vec<u8>,
    pub elapsed: Duration,
}

impl Response {
    pub fn content_type(&self) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("content-type"))
            .map(|(_, v)| v.as_str())
    }

    /// Lossy text of the body, truncated to `VIEW_LIMIT`.
    pub fn text(&self) -> (String, bool) {
        let cut = self.raw.len() > VIEW_LIMIT;
        let slice = if cut {
            &self.raw[..VIEW_LIMIT]
        } else {
            &self.raw[..]
        };
        (String::from_utf8_lossy(slice).into_owned(), cut)
    }
}

pub fn prepare(req: &Request, vars: &BTreeMap<String, String>) -> Result<Prepared> {
    let sub = |s: &str| substitute(s, vars);
    let mut url = url::Url::parse(&sub(&req.url))
        .with_context(|| format!("invalid URL '{}'", sub(&req.url)))?;
    {
        let mut q = url.query_pairs_mut();
        for p in req.params.iter().filter(|p| p.enabled && !p.key.is_empty()) {
            q.append_pair(&sub(&p.key), &sub(&p.value));
        }
        if let Auth::Apikey {
            key,
            value,
            query: true,
        } = &req.auth
        {
            q.append_pair(&sub(key), &sub(value));
        }
    }
    if url.query() == Some("") {
        url.set_query(None);
    }

    let mut headers: Vec<(String, String)> = req
        .headers
        .iter()
        .filter(|h| h.enabled && !h.key.is_empty())
        .map(|h| (sub(&h.key), sub(&h.value)))
        .collect();
    let has = |hs: &[(String, String)], k: &str| hs.iter().any(|(n, _)| n.eq_ignore_ascii_case(k));
    match &req.auth {
        Auth::Bearer { token } if !has(&headers, "authorization") => {
            headers.push(("Authorization".into(), format!("Bearer {}", sub(token))));
        }
        Auth::Basic { username, password } if !has(&headers, "authorization") => {
            let cred = B64.encode(format!("{}:{}", sub(username), sub(password)));
            headers.push(("Authorization".into(), format!("Basic {cred}")));
        }
        Auth::Apikey {
            key,
            value,
            query: false,
        } => headers.push((sub(key), sub(value))),
        _ => {}
    }

    let body = match req.body.kind {
        BodyKind::None => None,
        BodyKind::Json | BodyKind::Text => {
            let ct = if req.body.kind == BodyKind::Json {
                "application/json"
            } else {
                "text/plain"
            };
            if !has(&headers, "content-type") {
                headers.push(("Content-Type".into(), ct.into()));
            }
            Some(sub(&req.body.content))
        }
        BodyKind::Form => {
            if !has(&headers, "content-type") {
                headers.push((
                    "Content-Type".into(),
                    "application/x-www-form-urlencoded".into(),
                ));
            }
            let mut ser = url::form_urlencoded::Serializer::new(String::new());
            for line in req.body.content.lines().filter(|l| !l.trim().is_empty()) {
                let (k, v) = line.split_once('=').unwrap_or((line, ""));
                ser.append_pair(&sub(k.trim()), &sub(v.trim()));
            }
            Some(ser.finish())
        }
    };

    Ok(Prepared {
        method: req.method,
        url: url.into(),
        headers,
        body,
        follow_redirects: req.follow_redirects,
        insecure: req.insecure,
    })
}

pub async fn send(
    p: Prepared,
    jar: Arc<reqwest::cookie::Jar>,
    timeout_secs: u64,
) -> Result<Response> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("turly/", env!("CARGO_PKG_VERSION")))
        .cookie_provider(jar)
        .timeout(Duration::from_secs(timeout_secs))
        .redirect(if p.follow_redirects {
            reqwest::redirect::Policy::limited(10)
        } else {
            reqwest::redirect::Policy::none()
        })
        .danger_accept_invalid_certs(p.insecure)
        .build()?;
    let method = reqwest::Method::from_bytes(p.method.as_str().as_bytes())?;
    let mut rb = client.request(method, &p.url);
    for (k, v) in &p.headers {
        rb = rb.header(k, v);
    }
    if let Some(b) = p.body {
        rb = rb.body(b);
    }
    let start = Instant::now();
    let resp = rb.send().await?;
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| {
            (
                k.to_string(),
                String::from_utf8_lossy(v.as_bytes()).into_owned(),
            )
        })
        .collect();
    let raw = resp.bytes().await?.to_vec();
    Ok(Response {
        status: status.as_u16(),
        reason: status.canonical_reason().unwrap_or("").into(),
        headers,
        raw,
        elapsed: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Param;

    #[test]
    fn prepare_builds_url_auth_and_form() {
        let mut vars = BTreeMap::new();
        vars.insert("base_url".to_string(), "http://x.test".to_string());
        vars.insert("token".to_string(), "abc".to_string());
        let mut r = Request::new("t");
        r.url = "{{base_url}}/p".into();
        r.params = vec![
            Param::new("a b", "1&2"),
            Param {
                enabled: false,
                ..Param::new("off", "1")
            },
        ];
        r.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        r.body.kind = BodyKind::Form;
        r.body.content = "k=v w\nz=1".into();
        let p = prepare(&r, &vars).unwrap();
        assert_eq!(p.url, "http://x.test/p?a+b=1%262");
        assert!(
            p.headers
                .contains(&("Authorization".into(), "Bearer abc".into()))
        );
        assert_eq!(p.body.as_deref(), Some("k=v+w&z=1"));
    }

    #[test]
    fn bad_url_errors() {
        let mut r = Request::new("t");
        r.url = "{{missing}}/x".into();
        assert!(prepare(&r, &BTreeMap::new()).is_err());
    }
}
