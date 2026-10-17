//! OpenAPI 3.x / Swagger 2.0 import. Swagger 2.0 is upconverted to 3.0 in memory,
//! then a single mapper turns operations into requests.

use crate::model::{Auth, Body, BodyKind, Env, Method, Param, Request};
use crate::workspace::{Workspace, slug};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const METHODS: [&str; 7] = ["get", "put", "post", "delete", "patch", "head", "options"];

pub struct Imported {
    pub title: String,
    /// (folder, request)
    pub requests: Vec<(String, Request)>,
    pub envs: Vec<(String, Env)>,
}

/// Load a spec from a file path or http(s) URL.
pub async fn fetch(src: &str) -> Result<String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        let r = reqwest::get(src).await?.error_for_status()?;
        Ok(r.text().await?)
    } else {
        std::fs::read_to_string(src).with_context(|| format!("reading {src}"))
    }
}

pub fn parse(text: &str) -> Result<Value> {
    match serde_json::from_str(text) {
        Ok(v) => Ok(v),
        Err(_) => serde_yaml_ng::from_str(text).context("spec is neither valid JSON nor YAML"),
    }
}

fn rewrite_refs(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, val) in m.iter_mut() {
                if k == "$ref"
                    && let Value::String(s) = val
                {
                    *s = s
                        .replace("#/definitions/", "#/components/schemas/")
                        .replace("#/parameters/", "#/components/parameters/");
                } else {
                    rewrite_refs(val);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(rewrite_refs),
        _ => {}
    }
}

/// Follow `$ref` chains (local refs only).
fn resolve<'a>(root: &'a Value, mut v: &'a Value) -> &'a Value {
    for _ in 0..10 {
        match v
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix('#'))
        {
            Some(ptr) => match root.pointer(ptr) {
                Some(t) => v = t,
                None => return v,
            },
            None => return v,
        }
    }
    v
}

fn upconvert(mut doc: Value) -> Value {
    rewrite_refs(&mut doc);
    let host = doc["host"].as_str().unwrap_or("localhost").to_string();
    let base = doc["basePath"]
        .as_str()
        .unwrap_or("")
        .trim_end_matches('/')
        .to_string();
    let schemes: Vec<String> = doc["schemes"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .filter(|v: &Vec<String>| !v.is_empty())
        .unwrap_or_else(|| vec!["https".into()]);
    let servers: Vec<Value> = schemes
        .iter()
        .map(|s| json!({ "url": format!("{s}://{host}{base}") }))
        .collect();

    let mut components = Map::new();
    if let Some(d) = doc.get("definitions") {
        components.insert("schemas".into(), d.clone());
    }
    if let Some(p) = doc.get("parameters") {
        components.insert("parameters".into(), p.clone());
    }
    if let Some(sd) = doc.get("securityDefinitions").and_then(Value::as_object) {
        let mut out = Map::new();
        for (k, v) in sd {
            match v["type"].as_str() {
                Some("basic") => out.insert(k.clone(), json!({"type": "http", "scheme": "basic"})),
                Some("apiKey") => out.insert(k.clone(), v.clone()),
                _ => None,
            };
        }
        components.insert("securitySchemes".into(), Value::Object(out));
    }

    let global_consumes = doc["consumes"].clone();
    let snapshot = doc.clone();
    if let Some(paths) = doc.get_mut("paths").and_then(Value::as_object_mut) {
        for item in paths.values_mut() {
            let path_params = item.get("parameters").cloned();
            let Some(item) = item.as_object_mut() else {
                continue;
            };
            for m in METHODS {
                let Some(op) = item.get_mut(m).and_then(Value::as_object_mut) else {
                    continue;
                };
                let consumes = op.get("consumes").unwrap_or(&global_consumes).clone();
                let ct = consumes
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_str)
                    .unwrap_or("application/json")
                    .to_string();
                let mut keep = vec![];
                let mut body_schema = None;
                let mut form = Map::new();
                let all: Vec<Value> = op
                    .get("parameters")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                // Path-level params are merged by the mapper, but body/form params can live there too.
                let extra: Vec<Value> = path_params
                    .as_ref()
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                for p in all.iter().chain(extra.iter()) {
                    let rp = resolve(&snapshot, p).clone();
                    match rp["in"].as_str() {
                        Some("body") => body_schema = Some(rp["schema"].clone()),
                        Some("formData") => {
                            let mut s = rp.clone();
                            s.as_object_mut()
                                .map(|o| (o.remove("in"), o.remove("name"), o.remove("required")));
                            form.insert(rp["name"].as_str().unwrap_or("field").to_string(), s);
                        }
                        _ if all.contains(p) => keep.push(p.clone()),
                        _ => {}
                    }
                }
                op.insert("parameters".into(), Value::Array(keep));
                if let Some(s) = body_schema {
                    op.insert(
                        "requestBody".into(),
                        json!({ "content": { ct: { "schema": s } } }),
                    );
                } else if !form.is_empty() {
                    let ct = if ct.contains("form") {
                        ct
                    } else {
                        "application/x-www-form-urlencoded".into()
                    };
                    op.insert("requestBody".into(), json!({ "content": { ct: { "schema": { "type": "object", "properties": form } } } }));
                }
            }
            // Path-level non-body params stay at path level; drop body/form ones already moved.
            if let Some(Value::Array(a)) = item.get_mut("parameters") {
                a.retain(|p| {
                    !matches!(
                        resolve(&snapshot, p)["in"].as_str(),
                        Some("body" | "formData")
                    )
                });
            }
        }
    }

    let o = doc.as_object_mut().unwrap();
    o.insert("openapi".into(), json!("3.0.0"));
    o.insert("servers".into(), Value::Array(servers));
    o.insert("components".into(), Value::Object(components));
    doc
}

fn scalar(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(_) | Value::Number(_) => v.to_string(),
        _ => v.to_string(),
    }
}

fn sample(root: &Value, schema: &Value, depth: u8) -> Value {
    let s = resolve(root, schema);
    if depth > 5 {
        return Value::Null;
    }
    for k in ["example", "x-example", "default"] {
        if let Some(v) = s.get(k) {
            return v.clone();
        }
    }
    if let Some(v) = s.get("enum").and_then(|e| e.get(0)) {
        return v.clone();
    }
    if let Some(all) = s.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        for sub in all {
            if let Value::Object(m) = sample(root, sub, depth + 1) {
                merged.extend(m);
            }
        }
        return Value::Object(merged);
    }
    for k in ["oneOf", "anyOf"] {
        if let Some(first) = s.get(k).and_then(|a| a.get(0)) {
            return sample(root, first, depth + 1);
        }
    }
    let ty = s["type"]
        .as_str()
        .unwrap_or(if s.get("properties").is_some() {
            "object"
        } else {
            ""
        });
    match ty {
        "object" => {
            let mut m = Map::new();
            if let Some(props) = s["properties"].as_object() {
                for (k, v) in props {
                    m.insert(k.clone(), sample(root, v, depth + 1));
                }
            }
            Value::Object(m)
        }
        "array" => Value::Array(vec![sample(root, &s["items"], depth + 1)]),
        "integer" => json!(0),
        "number" => json!(0.0),
        "boolean" => json!(false),
        "string" => json!(match s["format"].as_str() {
            Some("date-time") => "2020-01-01T00:00:00Z",
            Some("date") => "2020-01-01",
            Some("uuid") => "00000000-0000-0000-0000-000000000000",
            Some("email") => "user@example.com",
            _ => "string",
        }),
        _ => Value::Null,
    }
}

fn param_value(root: &Value, p: &Value) -> String {
    for k in ["example", "x-example", "default"] {
        if let Some(v) = p.get(k) {
            return scalar(v);
        }
    }
    match p.get("schema") {
        Some(s) => scalar(&sample(root, s, 0)),
        None => scalar(&sample(root, p, 0)),
    }
}

fn media_example(root: &Value, media: &Value) -> Value {
    if let Some(e) = media.get("example") {
        return e.clone();
    }
    if let Some(first) = media
        .get("examples")
        .and_then(Value::as_object)
        .and_then(|m| m.values().next())
    {
        let first = resolve(root, first);
        if let Some(v) = first.get("value") {
            return v.clone();
        }
    }
    sample(root, &media["schema"], 0)
}

fn body_from(root: &Value, rb: &Value) -> Body {
    let rb = resolve(root, rb);
    let Some(content) = rb["content"].as_object().filter(|c| !c.is_empty()) else {
        return Body::default();
    };
    let (ct, media) = content
        .get_key_value("application/json")
        .or_else(|| content.iter().find(|(k, _)| k.contains("json")))
        .or_else(|| content.iter().next())
        .unwrap();
    let ex = media_example(root, media);
    if ct.contains("json") {
        return Body {
            kind: BodyKind::Json,
            content: serde_json::to_string_pretty(&ex).unwrap_or_default(),
        };
    }
    if ct.contains("form") {
        let lines = match &ex {
            Value::Object(m) => m
                .iter()
                .map(|(k, v)| format!("{k}={}", scalar(v)))
                .collect::<Vec<_>>()
                .join("\n"),
            _ => String::new(),
        };
        return Body {
            kind: BodyKind::Form,
            content: lines,
        };
    }
    Body {
        kind: BodyKind::Text,
        content: scalar(&ex),
    }
}

/// Auth for an operation plus the secret var names it needs.
fn auth_from(root: &Value, op: &Value) -> (Auth, Vec<&'static str>) {
    let sec = op.get("security").or_else(|| root.get("security"));
    let Some(name) = sec
        .and_then(|s| s.get(0))
        .and_then(Value::as_object)
        .and_then(|m| m.keys().next())
    else {
        return (Auth::None, vec![]);
    };
    let Some(scheme) = root.pointer(&format!("/components/securitySchemes/{name}")) else {
        return (Auth::None, vec![]);
    };
    let scheme = resolve(root, scheme);
    match (
        scheme["type"].as_str(),
        scheme["scheme"]
            .as_str()
            .map(str::to_ascii_lowercase)
            .as_deref(),
        scheme["in"].as_str(),
    ) {
        (Some("http"), Some("bearer"), _) => (
            Auth::Bearer {
                token: "{{token}}".into(),
            },
            vec!["token"],
        ),
        (Some("http"), Some("basic"), _) => (
            Auth::Basic {
                username: "{{username}}".into(),
                password: "{{password}}".into(),
            },
            vec!["username", "password"],
        ),
        (Some("apiKey"), _, loc) => (
            Auth::Apikey {
                key: scheme["name"].as_str().unwrap_or("X-API-Key").into(),
                value: "{{api_key}}".into(),
                query: loc == Some("query"),
            },
            vec!["api_key"],
        ),
        _ => (Auth::None, vec![]),
    }
}

fn server_url(server: &Value) -> String {
    let mut url = server["url"].as_str().unwrap_or("").to_string();
    if let Some(vars) = server["variables"].as_object() {
        for (k, v) in vars {
            url = url.replace(&format!("{{{k}}}"), &scalar(&v["default"]));
        }
    }
    let url = url.trim_end_matches('/').to_string();
    if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("http://localhost{url}")
    }
}

pub fn import(text: &str) -> Result<Imported> {
    let mut doc = parse(text)?;
    let is_v2 = doc
        .get("swagger")
        .and_then(Value::as_str)
        .is_some_and(|s| s.starts_with('2'));
    let is_v3 = doc
        .get("openapi")
        .and_then(Value::as_str)
        .is_some_and(|s| s.starts_with('3'));
    if !is_v2 && !is_v3 {
        bail!("not an OpenAPI 3.x or Swagger 2.0 document");
    }
    if is_v2 {
        doc = upconvert(doc);
    }
    let root = &doc;
    let title = root["info"]["title"].as_str().unwrap_or("api").to_string();

    let mut path_vars: BTreeMap<String, String> = BTreeMap::new();
    let mut secret_vars: Vec<&'static str> = vec![];
    let mut requests = vec![];
    let mut order = 0u32;

    let empty = Map::new();
    for (path, item) in root["paths"].as_object().unwrap_or(&empty) {
        let item = resolve(root, item);
        for m in METHODS {
            let Some(op) = item.get(m) else { continue };
            let method = Method::parse(m).unwrap();

            // Operation params override path-level ones with the same (in, name).
            let mut params: Vec<Value> = vec![];
            for p in op["parameters"]
                .as_array()
                .into_iter()
                .chain(item["parameters"].as_array())
                .flatten()
            {
                let p = resolve(root, p).clone();
                let id = (p["in"].clone(), p["name"].clone());
                if !params
                    .iter()
                    .any(|q| (q["in"].clone(), q["name"].clone()) == id)
                {
                    params.push(p);
                }
            }

            let mut req = Request::new(
                op["operationId"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| format!("{} {path}", m.to_uppercase()))
                    .as_str(),
            );
            req.method = method;
            let mut url_path = String::new();
            let mut rest = path.as_str();
            while let Some(s) = rest.find('{') {
                let Some(e) = rest[s..].find('}') else { break };
                url_path.push_str(&rest[..s]);
                url_path.push_str(&format!("{{{{{}}}}}", &rest[s + 1..s + e]));
                rest = &rest[s + e + 1..];
            }
            url_path.push_str(rest);
            req.url = format!("{{{{base_url}}}}{url_path}");

            for p in &params {
                let name = p["name"].as_str().unwrap_or("").to_string();
                let value = param_value(root, p);
                let required = p["required"].as_bool().unwrap_or(false);
                match p["in"].as_str() {
                    Some("path") => {
                        path_vars.entry(name).or_insert(value);
                    }
                    Some("query") => req.params.push(Param {
                        key: name,
                        value,
                        enabled: required,
                    }),
                    Some("header") => req.headers.push(Param {
                        key: name,
                        value,
                        enabled: required,
                    }),
                    _ => {}
                }
            }

            if let Some(rb) = op.get("requestBody") {
                req.body = body_from(root, rb);
                req.example = (req.body.kind != BodyKind::None).then(|| req.body.clone());
            }
            let (auth, secrets) = auth_from(root, op);
            req.auth = auth;
            for s in secrets {
                if !secret_vars.contains(&s) {
                    secret_vars.push(s);
                }
            }

            let mut docs_parts = vec![];
            for d in [op["summary"].as_str(), op["description"].as_str()]
                .into_iter()
                .flatten()
            {
                if !docs_parts.contains(&d) {
                    docs_parts.push(d);
                }
            }
            let docs = docs_parts.join("\n\n");
            req.docs = (!docs.is_empty()).then_some(docs);
            order += 1;
            req.order = Some(order);

            let folder = op["tags"]
                .get(0)
                .and_then(Value::as_str)
                .map(slug)
                .unwrap_or_else(|| "default".into());
            requests.push((folder, req));
        }
    }

    let servers: Vec<Value> = root["servers"].as_array().cloned().unwrap_or_default();
    let servers = if servers.is_empty() {
        vec![json!({"url": "http://localhost"})]
    } else {
        servers
    };
    let mut envs: Vec<(String, Env)> = vec![];
    for (i, s) in servers.iter().enumerate() {
        let mut name = if i == 0 {
            "default".to_string()
        } else {
            s["description"]
                .as_str()
                .map(slug)
                .unwrap_or_else(|| format!("server-{}", i + 1))
        };
        while envs.iter().any(|(n, _)| *n == name) {
            name = format!("{name}-{}", i + 1);
        }
        let mut env = Env::default();
        env.vars.insert("base_url".into(), server_url(s));
        env.vars.extend(path_vars.clone());
        env.secrets = secret_vars.iter().map(|s| s.to_string()).collect();
        envs.push((name, env));
    }

    Ok(Imported {
        title,
        requests,
        envs,
    })
}

/// Create a workspace in `dir` from an import.
pub fn build_workspace(dir: &Path, imp: &Imported) -> Result<Workspace> {
    let ws = Workspace::init(dir, &imp.title)?;
    for (name, env) in &imp.envs {
        ws.save_env(name, env)?;
    }
    let mut ws = ws;
    ws.meta.active_env = imp.envs.first().map(|(n, _)| n.clone());
    ws.save_meta()?;
    for (folder, req) in &imp.requests {
        let d = ws.requests_dir().join(folder);
        std::fs::create_dir_all(&d)?;
        let p = ws.new_request_path(&d, &req.name);
        ws.save_request(&p, req)?;
    }
    Ok(ws)
}

#[cfg(test)]
mod tests {
    use super::*;

    const V3: &str = r##"{
      "openapi": "3.0.3", "info": {"title": "Pets"},
      "servers": [{"url": "https://{env}.pets.test/v1", "variables": {"env": {"default": "api"}}}],
      "security": [{"bearer": []}],
      "paths": {
        "/pet/{petId}": {
          "parameters": [{"name": "petId", "in": "path", "required": true, "schema": {"type": "integer", "example": 7}}],
          "get": {"operationId": "getPet", "tags": ["pet"], "summary": "Get one",
                  "parameters": [{"name": "verbose", "in": "query", "schema": {"type": "boolean"}}]},
          "post": {"tags": ["pet"], "requestBody": {"content": {"application/json": {"schema": {"$ref": "#/components/schemas/Pet"}}}}}
        }
      },
      "components": {
        "securitySchemes": {"bearer": {"type": "http", "scheme": "bearer"}},
        "schemas": {"Pet": {"type": "object", "properties": {"id": {"type": "integer"}, "name": {"type": "string", "example": "rex"}}}}
      }
    }"##;

    const V2: &str = r##"
swagger: "2.0"
info: {title: Legacy}
host: api.legacy.test
basePath: /v2
schemes: [https]
consumes: [application/json]
paths:
  /pet:
    post:
      tags: [pet]
      parameters:
        - {name: body, in: body, schema: {$ref: "#/definitions/Pet"}}
definitions:
  Pet: {type: object, properties: {name: {type: string, example: tom}}}
"##;

    #[test]
    fn imports_v3() {
        let i = import(V3).unwrap();
        assert_eq!(i.title, "Pets");
        assert_eq!(i.requests.len(), 2);
        let (folder, get) = &i.requests[0];
        assert_eq!(folder, "pet");
        assert_eq!(get.name, "getPet");
        assert_eq!(get.url, "{{base_url}}/pet/{{petId}}");
        assert_eq!(
            get.params,
            vec![Param {
                key: "verbose".into(),
                value: "false".into(),
                enabled: false
            }]
        );
        assert_eq!(
            get.auth,
            Auth::Bearer {
                token: "{{token}}".into()
            }
        );
        let post = &i.requests[1].1;
        assert_eq!(post.name, "POST /pet/{petId}");
        assert_eq!(post.body.kind, BodyKind::Json);
        assert!(post.body.content.contains("\"name\": \"rex\""));
        let env = &i.envs[0].1;
        assert_eq!(env.vars["base_url"], "https://api.pets.test/v1");
        assert_eq!(env.vars["petId"], "7");
        assert_eq!(env.secrets, vec!["token"]);
    }

    #[test]
    fn imports_v2_via_upconvert() {
        let i = import(V2).unwrap();
        assert_eq!(i.envs[0].1.vars["base_url"], "https://api.legacy.test/v2");
        let r = &i.requests[0].1;
        assert_eq!(r.body.kind, BodyKind::Json);
        assert!(r.body.content.contains("\"name\": \"tom\""));
    }

    #[test]
    fn rejects_other_documents() {
        assert!(import("{\"hello\": 1}").is_err());
    }

    #[test]
    fn builds_workspace_on_disk() {
        let d = tempfile::tempdir().unwrap();
        let ws = build_workspace(&d.path().join("pets"), &import(V3).unwrap()).unwrap();
        assert_eq!(ws.meta.active_env.as_deref(), Some("default"));
        let tree = ws.tree().unwrap();
        assert_eq!(tree.len(), 1);
    }

    fn summary(i: &Imported) -> String {
        let mut out = format!(
            "{} | envs: {}\n",
            i.title,
            i.envs
                .iter()
                .map(|(n, e)| format!("{n}={}", e.vars["base_url"]))
                .collect::<Vec<_>>()
                .join(", ")
        );
        for (folder, r) in &i.requests {
            out.push_str(&format!(
                "{folder}/{} {} {} auth={} body={}\n",
                r.name,
                r.method.as_str(),
                r.url,
                r.auth.label(),
                r.body.kind.label()
            ));
        }
        out
    }

    #[test]
    fn snapshot_petstore_openapi3() {
        insta::assert_snapshot!(summary(
            &import(include_str!("../tests/fixtures/petstore-openapi3.json")).unwrap()
        ));
    }

    #[test]
    fn snapshot_petstore_swagger2() {
        insta::assert_snapshot!(summary(
            &import(include_str!("../tests/fixtures/petstore-swagger2.json")).unwrap()
        ));
    }
}
