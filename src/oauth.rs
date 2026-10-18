//! OAuth2 token acquisition: client credentials, password and authorization code (PKCE).

use crate::model::{Grant, OAuth};
use crate::vars::substitute;
use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(120);
const DEFAULT_PORT: u16 = 8765;

#[derive(Clone, Debug)]
pub struct Token {
    access: String,
    expires: Option<Instant>,
    refresh: Option<String>,
}

impl Token {
    fn fresh(&self) -> bool {
        self.expires
            .is_none_or(|e| Instant::now() + Duration::from_secs(30) < e)
    }
}

/// In-memory tokens for the session, keyed by grant + endpoint + client + scope + user.
pub type Cache = Arc<Mutex<HashMap<String, Token>>>;

/// `o` with `{{vars}}` substituted in every field.
pub fn resolve(o: &OAuth, vars: &BTreeMap<String, String>) -> OAuth {
    let sub = |s: &str| substitute(s, vars);
    OAuth {
        grant: o.grant,
        token_url: sub(&o.token_url),
        auth_url: sub(&o.auth_url),
        client_id: sub(&o.client_id),
        client_secret: sub(&o.client_secret),
        scope: sub(&o.scope),
        username: sub(&o.username),
        password: sub(&o.password),
        port: o.port,
    }
}

fn key(o: &OAuth) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        o.grant.label(),
        o.token_url,
        o.client_id,
        o.scope,
        o.username
    )
}

/// Access token for `o`: cached, refreshed, or freshly granted. `prompt` gets the sign-in URL (code grant).
pub async fn bearer(o: &OAuth, cache: &Cache, prompt: impl Fn(&str)) -> Result<String> {
    if o.token_url.is_empty() {
        bail!("oauth2: token_url is empty");
    }
    let k = key(o);
    let cached = cache.lock().unwrap().get(&k).cloned();
    if let Some(t) = cached.as_ref().filter(|t| t.fresh()) {
        return Ok(t.access.clone());
    }
    let mut tok = None;
    if let Some(r) = cached.and_then(|t| t.refresh) {
        let form = [("grant_type", "refresh_token"), ("refresh_token", &r)];
        tok = token_request(o, &form).await.ok().map(|mut t| {
            t.refresh.get_or_insert(r);
            t
        });
    }
    let tok = match tok {
        Some(t) => t,
        None => match o.grant {
            Grant::ClientCredentials => {
                token_request(o, &[("grant_type", "client_credentials")]).await?
            }
            Grant::Password => {
                let form = [
                    ("grant_type", "password"),
                    ("username", &o.username),
                    ("password", &o.password),
                ];
                token_request(o, &form).await?
            }
            Grant::Code => code_flow(o, prompt).await?,
        },
    };
    cache.lock().unwrap().insert(k, tok.clone());
    Ok(tok.access)
}

async fn token_request(o: &OAuth, form: &[(&str, &str)]) -> Result<Token> {
    let mut form: Vec<(&str, &str)> = form.to_vec();
    for (k, v) in [
        ("client_id", &o.client_id),
        ("client_secret", &o.client_secret),
        ("scope", &o.scope),
    ] {
        if !v.is_empty() {
            form.push((k, v));
        }
    }
    let body = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(&form)
        .finish();
    let resp = reqwest::Client::builder()
        .user_agent(concat!("turly/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?
        .post(&o.token_url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .with_context(|| format!("token request to {}", o.token_url))?;
    let status = resp.status();
    let text = resp.text().await?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    let Some(access) = v["access_token"].as_str().filter(|_| status.is_success()) else {
        let why = v["error_description"]
            .as_str()
            .or(v["error"].as_str())
            .unwrap_or(text.trim());
        bail!("token endpoint returned {status}: {why}");
    };
    Ok(Token {
        access: access.into(),
        expires: v["expires_in"]
            .as_u64()
            .map(|s| Instant::now() + Duration::from_secs(s)),
        refresh: v["refresh_token"].as_str().map(Into::into),
    })
}

fn random_b64(n: usize) -> Result<String> {
    let mut b = vec![0u8; n];
    getrandom::fill(&mut b).map_err(|e| anyhow::anyhow!("random: {e}"))?;
    Ok(B64URL.encode(b))
}

/// Authorization code + PKCE with a one-shot loopback redirect listener.
async fn code_flow(o: &OAuth, prompt: impl Fn(&str)) -> Result<Token> {
    if o.auth_url.is_empty() {
        bail!("oauth2: auth_url is empty");
    }
    let port = if o.port == 0 { DEFAULT_PORT } else { o.port };
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .with_context(|| format!("cannot listen on 127.0.0.1:{port} (set `port:`)"))?;
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let verifier = random_b64(32)?;
    let challenge = B64URL.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_b64(16)?;

    let mut url = url::Url::parse(&o.auth_url).context("invalid auth_url")?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code")
            .append_pair("client_id", &o.client_id)
            .append_pair("redirect_uri", &redirect)
            .append_pair("state", &state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        if !o.scope.is_empty() {
            q.append_pair("scope", &o.scope);
        }
    }
    prompt(url.as_str());

    let code = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_code(&listener, &state))
        .await
        .context("timed out waiting for the browser sign-in")??;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", &redirect),
        ("code_verifier", &verifier),
    ];
    token_request(o, &form).await
}

async fn wait_for_code(listener: &TcpListener, state: &str) -> Result<String> {
    loop {
        let (mut sock, _) = listener.accept().await?;
        let mut buf = vec![0u8; 8192];
        let n = sock.read(&mut buf).await?;
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req
            .lines()
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .unwrap_or("");
        let Ok(u) = url::Url::parse(&format!("http://localhost{target}")) else {
            continue;
        };
        if u.path() != "/callback" {
            let _ = sock
                .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                .await;
            continue;
        }
        let q: HashMap<_, _> = u.query_pairs().collect();
        let (title, res) = if let Some(e) = q.get("error") {
            let d = q.get("error_description").map(|d| d.as_ref()).unwrap_or("");
            (
                "Sign-in failed",
                Err(anyhow::anyhow!("authorization failed: {e} {d}")),
            )
        } else if q.get("state").map(|s| s.as_ref()) != Some(state) {
            (
                "Sign-in failed",
                Err(anyhow::anyhow!("authorization state mismatch")),
            )
        } else if let Some(c) = q.get("code") {
            ("Signed in", Ok(c.to_string()))
        } else {
            (
                "Sign-in failed",
                Err(anyhow::anyhow!("no code in redirect")),
            )
        };
        let html = format!(
            "<!doctype html><title>Turly</title><h3>{title}</h3><p>You can close this tab and return to the terminal.</p>"
        );
        let _ = sock
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{html}",
                    html.len()
                )
                .as_bytes(),
            )
            .await;
        return res;
    }
}

/// Best-effort: open `url` in the default browser.
pub fn open_browser(url: &str) {
    let mut c = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut c = std::process::Command::new("rundll32");
        c.arg("url.dll,FileProtocolHandler");
        c
    } else {
        std::process::Command::new("xdg-open")
    };
    let _ = c
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Token endpoint answering every POST with `body`; returns its URL and the request bodies seen.
    async fn mock(body: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/token", l.local_addr().unwrap());
        let seen = Arc::new(Mutex::new(vec![]));
        let s2 = seen.clone();
        tokio::spawn(async move {
            loop {
                let (mut sock, _) = l.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap();
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                s2.lock()
                    .unwrap()
                    .push(req.split("\r\n\r\n").nth(1).unwrap_or("").into());
                let r = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(r.as_bytes()).await;
            }
        });
        (url, seen)
    }

    #[tokio::test]
    async fn client_credentials_cached() {
        let (url, seen) = mock(r#"{"access_token":"tok1","expires_in":3600}"#).await;
        let o = OAuth {
            token_url: url,
            client_id: "id".into(),
            client_secret: "sec".into(),
            scope: "a b".into(),
            ..Default::default()
        };
        let cache = Cache::default();
        assert_eq!(bearer(&o, &cache, |_| {}).await.unwrap(), "tok1");
        assert_eq!(bearer(&o, &cache, |_| {}).await.unwrap(), "tok1");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].contains("grant_type=client_credentials"));
        assert!(seen[0].contains("client_secret=sec") && seen[0].contains("scope=a+b"));
    }

    #[tokio::test]
    async fn expired_token_refreshes() {
        let (url, seen) =
            mock(r#"{"access_token":"new","expires_in":3600,"refresh_token":"r2"}"#).await;
        let o = OAuth {
            grant: Grant::Password,
            token_url: url,
            ..Default::default()
        };
        let cache = Cache::default();
        cache.lock().unwrap().insert(
            key(&o),
            Token {
                access: "old".into(),
                expires: Some(Instant::now()),
                refresh: Some("r1".into()),
            },
        );
        assert_eq!(bearer(&o, &cache, |_| {}).await.unwrap(), "new");
        assert!(seen.lock().unwrap()[0].contains("grant_type=refresh_token&refresh_token=r1"));
    }

    #[tokio::test]
    async fn token_errors_surface() {
        let (url, _) = mock(r#"{"error":"invalid_client","error_description":"bad secret"}"#).await;
        let o = OAuth {
            token_url: url,
            ..Default::default()
        };
        let e = bearer(&o, &Cache::default(), |_| {}).await.unwrap_err();
        assert!(e.to_string().contains("bad secret"));
    }

    #[tokio::test]
    async fn code_flow_with_pkce() {
        let (url, seen) = mock(r#"{"access_token":"codetok"}"#).await;
        let o = OAuth {
            grant: Grant::Code,
            token_url: url,
            auth_url: "http://127.0.0.1:1/authorize".into(),
            client_id: "id".into(),
            port: 18765,
            ..Default::default()
        };
        // Stand in for the browser: follow the sign-in URL's redirect with a code.
        let tok = bearer(&o, &Cache::default(), |u| {
            let u = url::Url::parse(u).unwrap();
            let q: HashMap<_, _> = u.query_pairs().into_owned().collect();
            assert_eq!(q["code_challenge_method"], "S256");
            let cb = format!("{}?code=abc&state={}", q["redirect_uri"], q["state"]);
            tokio::spawn(async move {
                reqwest::get(cb).await.unwrap();
            });
        })
        .await
        .unwrap();
        assert_eq!(tok, "codetok");
        let body = seen.lock().unwrap()[0].clone();
        assert!(body.contains("grant_type=authorization_code") && body.contains("code=abc"));
        assert!(body.contains("code_verifier="));
    }
}
