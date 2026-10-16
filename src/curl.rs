//! Export a prepared request as a curl command.

use crate::http::Prepared;

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

pub fn to_curl(p: &Prepared) -> String {
    let mut parts = vec![format!("curl -X {}", p.method.as_str()), q(&p.url)];
    for (k, v) in &p.headers {
        parts.push(format!("-H {}", q(&format!("{k}: {v}"))));
    }
    if let Some(b) = &p.body {
        parts.push(format!("--data-raw {}", q(b)));
    }
    if p.follow_redirects {
        parts.push("-L".into());
    }
    if p.insecure {
        parts.push("-k".into());
    }
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Method;

    #[test]
    fn quotes_single_quotes() {
        let p = Prepared {
            method: Method::Post,
            url: "http://x/".into(),
            headers: vec![("A".into(), "b".into())],
            body: Some("it's".into()),
            follow_redirects: false,
            insecure: false,
        };
        assert_eq!(
            to_curl(&p),
            "curl -X POST 'http://x/' -H 'A: b' --data-raw 'it'\\''s'"
        );
    }
}
