//! `{{name}}` substitution; `{{$NAME}}` reads the OS environment.

use std::collections::BTreeMap;

pub fn substitute(input: &str, vars: &BTreeMap<String, String>) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let name = after[..end].trim();
        let value = match name.strip_prefix('$') {
            Some(env) => std::env::var(env).ok(),
            None => vars.get(name).cloned(),
        };
        match value {
            Some(v) => out.push_str(&v),
            None => out.push_str(&rest[start..start + 2 + end + 2]),
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitutes_known_and_keeps_unknown() {
        let mut v = BTreeMap::new();
        v.insert("host".to_string(), "api.test".to_string());
        assert_eq!(
            substitute("https://{{host}}/{{ id }}", &v),
            "https://api.test/{{ id }}"
        );
        assert_eq!(substitute("{{host", &v), "{{host");
    }

    #[test]
    fn reads_os_env() {
        // SAFETY: test-only, single var name unique to this test.
        unsafe { std::env::set_var("TURLY_TEST_VAR", "x") };
        assert_eq!(substitute("{{$TURLY_TEST_VAR}}", &BTreeMap::new()), "x");
    }
}
