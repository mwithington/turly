//! Response body highlighting via syntect, driven by the active Turly theme.

use crate::theme::Theme;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use std::str::FromStr;
use syntect::easy::HighlightLines;
use syntect::highlighting::{
    Color as SColor, ScopeSelectors, StyleModifier, Theme as SynTheme, ThemeItem, ThemeSettings,
};
use syntect::parsing::SyntaxSet;

/// Highlighting beyond this many lines is skipped.
const MAX_LINES: usize = 20_000;

pub struct Highlighter {
    ss: SyntaxSet,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Json,
    Xml,
    Html,
    Plain,
}

impl Lang {
    pub fn detect(content_type: Option<&str>, body: &str) -> Lang {
        let ct = content_type.unwrap_or("").to_ascii_lowercase();
        if ct.contains("json") {
            Lang::Json
        } else if ct.contains("html") {
            Lang::Html
        } else if ct.contains("xml") {
            Lang::Xml
        } else if matches!(body.trim_start().chars().next(), Some('{' | '['))
            && serde_json::from_str::<serde_json::Value>(body).is_ok()
        {
            Lang::Json
        } else {
            Lang::Plain
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Lang::Json => "json",
            Lang::Xml => "xml",
            Lang::Html => "html",
            Lang::Plain => "txt",
        }
    }
}

fn sc(c: Color) -> SColor {
    match c {
        Color::Rgb(r, g, b) => SColor { r, g, b, a: 255 },
        _ => SColor {
            r: 200,
            g: 200,
            b: 200,
            a: 255,
        },
    }
}

fn item(scope: &str, c: Color) -> ThemeItem {
    ThemeItem {
        scope: ScopeSelectors::from_str(scope).expect("static scope"),
        style: StyleModifier {
            foreground: Some(sc(c)),
            background: None,
            font_style: None,
        },
    }
}

fn syn_theme(t: &Theme) -> SynTheme {
    SynTheme {
        name: Some(t.name.clone()),
        author: None,
        settings: ThemeSettings {
            foreground: Some(sc(t.fg)),
            ..Default::default()
        },
        scopes: vec![
            item("comment", t.comment),
            item("string", t.string),
            item("constant.numeric", t.number),
            item("constant.language, keyword, storage", t.keyword),
            item(
                "meta.structure.dictionary.key string, meta.structure.dictionary.key, support.type.property-name",
                t.key,
            ),
            item("entity.name.tag", t.key),
            item("entity.other.attribute-name", t.number),
            item("punctuation.separator, punctuation.section", t.dim),
        ],
    }
}

/// Re-map syntect's RGB output to the theme color (keeps `Reset`/indexed themes working).
fn map_color(c: SColor, t: &Theme) -> Color {
    let candidates = [t.fg, t.dim, t.key, t.string, t.number, t.keyword, t.comment];
    for cand in candidates {
        if let Color::Rgb(r, g, b) = cand
            && (r, g, b) == (c.r, c.g, c.b)
        {
            return cand;
        }
    }
    Color::Rgb(c.r, c.g, c.b)
}

impl Highlighter {
    pub fn new() -> Highlighter {
        Highlighter {
            ss: SyntaxSet::load_defaults_newlines(),
        }
    }

    /// Pretty-print JSON; leave everything else as is.
    pub fn prepare(lang: Lang, body: &str) -> String {
        if lang == Lang::Json
            && let Ok(v) = serde_json::from_str::<serde_json::Value>(body)
            && let Ok(s) = serde_json::to_string_pretty(&v)
        {
            return s;
        }
        body.to_string()
    }

    pub fn lines(&self, lang: Lang, text: &str, theme: &Theme) -> Vec<Line<'static>> {
        let plain = |l: &str| {
            Line::from(Span::styled(
                l.replace('\t', "    "),
                Style::default().fg(theme.fg),
            ))
        };
        let syntax = self.ss.find_syntax_by_extension(lang.ext());
        let (Some(syntax), true) = (syntax, lang != Lang::Plain) else {
            return text.lines().map(plain).collect();
        };
        let st = syn_theme(theme);
        let mut hl = HighlightLines::new(syntax, &st);
        let mut out = Vec::new();
        for (i, l) in text.lines().enumerate() {
            if i >= MAX_LINES {
                out.push(plain(l));
                continue;
            }
            let line = format!("{l}\n");
            match hl.highlight_line(&line, &self.ss) {
                Ok(ranges) => out.push(Line::from(
                    ranges
                        .into_iter()
                        .map(|(st, s)| {
                            Span::styled(
                                s.trim_end_matches('\n').replace('\t', "    "),
                                Style::default().fg(map_color(st.foreground, theme)),
                            )
                        })
                        .collect::<Vec<_>>(),
                )),
                Err(_) => out.push(plain(l)),
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keys_and_strings_get_distinct_colors() {
        let t = Theme::preset("nord").unwrap();
        let h = Highlighter::new();
        let lines = h.lines(Lang::Json, "{\"a\": \"b\"}", &t);
        let colors: Vec<_> = lines[0]
            .spans
            .iter()
            .map(|s| (s.content.to_string(), s.style.fg))
            .collect();
        let key = colors.iter().find(|(c, _)| c == "a").unwrap().1;
        let val = colors.iter().find(|(c, _)| c == "b").unwrap().1;
        assert_eq!(key, Some(t.key));
        assert_eq!(val, Some(t.string));
    }

    #[test]
    fn detects_lang() {
        assert_eq!(Lang::detect(Some("application/json"), ""), Lang::Json);
        assert_eq!(Lang::detect(None, "[1]"), Lang::Json);
        assert_eq!(Lang::detect(Some("text/html"), "<p>"), Lang::Html);
        assert_eq!(Lang::detect(None, "hi"), Lang::Plain);
    }
}
