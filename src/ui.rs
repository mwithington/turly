//! Rendering.

use crate::app::{App, Focus, Mode, ReqField, ReqTab, RespTab, RowKind, text_of};
use crate::highlight::Lang;
use crate::model::{Auth, BodyKind, Method};
use crate::theme::Theme;
use edtui::{EditorState, EditorTheme, EditorView, Highlight, Index2};

use ratatui::Frame;
use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use std::collections::BTreeMap;

const LOGO: &str = r"  ✦      ▄▄███▄▄      · 
       ▄███▀▀ ▀▀▀       
  ·   ▐███▀       ✦     
      ████    ·         
      ▐███▄             
   ✧   ▀███▄▄ ▄▄▄  ·    
 ·       ▀▀███▀▀    ✦   ";

fn fg(c: Color) -> Style {
    Style::default().fg(c)
}

fn method_color(m: Method, t: &Theme) -> Color {
    match m {
        Method::Get => t.ok,
        Method::Post => t.warn,
        Method::Put | Method::Patch => t.info,
        Method::Delete => t.err,
        Method::Head | Method::Options => t.dim,
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hit = Default::default();
    let th = app.theme.clone();
    let area = f.area();
    f.render_widget(
        Block::default().style(Style::default().bg(th.bg).fg(th.fg)),
        area,
    );
    let [top, main, bot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);
    draw_top(f, app, &th, top);
    if app.ws.is_none() {
        draw_splash(f, &th, main);
    } else {
        let auto = (main.width / 4).clamp(24, 44);
        let w = app
            .side_w
            .unwrap_or(auto)
            .clamp(12, main.width.saturating_sub(20).max(12));
        let split = app.split.clamp(15, 85);
        let [side, right] =
            Layout::horizontal([Constraint::Length(w), Constraint::Min(10)]).areas(main);
        let [req, resp] = Layout::vertical([
            Constraint::Percentage(split),
            Constraint::Percentage(100 - split),
        ])
        .areas(right);
        app.hit.main = main;
        app.hit.right = right;
        app.hit.side_edge = Rect::new(side.right().saturating_sub(1), side.y, 2, side.height);
        app.hit.split_edge = Rect::new(right.x, req.bottom().saturating_sub(1), right.width, 2);
        draw_tree(f, app, &th, side);
        draw_request(f, app, &th, req);
        draw_response(f, app, &th, resp);
    }
    draw_bottom(f, app, &th, bot);
    if app.vars.is_some() {
        draw_vars(f, app, &th, area);
    }
    match app.mode {
        Mode::Command | Mode::Search => draw_popup(f, app, &th, area),
        Mode::Confirm => draw_confirm(f, app, &th, area),
        Mode::Help => draw_help(f, &th, area),
        Mode::Picker => draw_picker(f, app, &th, area),
        _ => {}
    }
}

fn draw_top(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let bar = Style::default().bg(th.selection).fg(th.fg);
    let mut left = vec![Span::styled(
        " turly ",
        bar.fg(th.accent).add_modifier(Modifier::BOLD),
    )];
    let mut right = String::new();
    if let Some(ws) = &app.ws {
        let vars = ws.resolved_vars(ws.meta.active_env.as_deref()).len();
        right = format!(
            "env: {} · {vars} vars ",
            ws.meta.active_env.as_deref().unwrap_or("none")
        );
        let name = format!("· workspace {} ", ws.meta.name);
        let path = format!("({})", ws.root.display());
        let used = 7 + name.chars().count() + right.chars().count();
        left.push(Span::styled(name, bar));
        if used + path.chars().count() < area.width as usize {
            left.push(Span::styled(path, bar.fg(th.dim)));
        }
    }
    let [l, r] = Layout::horizontal([
        Constraint::Min(1),
        Constraint::Length(right.chars().count() as u16),
    ])
    .areas(area);
    f.render_widget(Paragraph::new(Line::from(left)).style(bar), l);
    f.render_widget(
        Paragraph::new(Span::styled(right, bar.fg(th.info))).style(bar),
        r,
    );
}

fn draw_splash(f: &mut Frame, th: &Theme, area: Rect) {
    let mut lines: Vec<Line> = LOGO
        .lines()
        .map(|l| Line::from(Span::styled(l, fg(th.accent))))
        .collect();
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "Turly",
        fg(th.fg).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(Span::styled(
        "a terminal API client",
        fg(th.dim),
    )));
    lines.push(Line::raw(""));
    for l in [
        ":wsnew <path>   create a workspace",
        ":import <spec>   build one from an OpenAPI spec",
        ":help           key bindings",
        ":q              quit",
    ] {
        lines.push(Line::from(Span::styled(l, fg(th.fg))));
    }
    let h = lines.len() as u16;
    let r = Rect {
        y: area.y + area.height.saturating_sub(h) / 2,
        height: h.min(area.height),
        ..area
    };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), r);
}

fn pane<'a>(th: &Theme, title: String, focused: bool) -> Block<'a> {
    let c = if focused { th.accent } else { th.border };
    Block::bordered()
        .border_style(fg(c))
        .title(Span::styled(
            title,
            fg(if focused { th.accent } else { th.dim }),
        ))
        .style(Style::default().bg(th.bg).fg(th.fg))
}

fn draw_tree(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let title = if app.filter.is_empty() {
        " Requests ".to_string()
    } else {
        format!(" Requests /{} ", app.filter)
    };
    let focused = app.focus == Focus::Tree;
    let block = pane(th, title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.hit.tree = inner;
    let rows = app.rows();
    let h = inner.height as usize;
    if app.cursor < app.tree_offset {
        app.tree_offset = app.cursor;
    } else if h > 0 && app.cursor >= app.tree_offset + h {
        app.tree_offset = app.cursor + 1 - h;
    }
    let open_path = app.open.as_ref().and_then(|o| o.path.clone());
    let mut lines = vec![];
    for (i, r) in rows.iter().enumerate().skip(app.tree_offset).take(h) {
        let indent = "  ".repeat(r.depth);
        let mut spans = vec![Span::raw(indent)];
        match &r.kind {
            RowKind::Dir { open, .. } | RowKind::HistDir { open } => {
                spans.push(Span::styled(
                    format!("{} {}", if *open { "▾" } else { "▸" }, r.name),
                    fg(th.dim),
                ));
            }
            RowKind::Workspace { open, active, .. } => {
                let st = if *active {
                    fg(th.accent).add_modifier(Modifier::BOLD)
                } else {
                    fg(th.fg)
                };
                spans.push(Span::styled(
                    format!("{} {}", if *open { "▾" } else { "▸" }, r.name),
                    st,
                ));
                if *active {
                    spans.push(Span::styled(" ●", fg(th.ok)));
                }
            }
            RowKind::Req { path, method } => {
                spans.push(Span::styled(
                    format!("{:<5}", method.short()),
                    fg(method_color(*method, th)).add_modifier(Modifier::BOLD),
                ));
                let st = if open_path.as_ref() == Some(path) {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                spans.push(Span::styled(r.name.clone(), st));
            }
            RowKind::Hist(_) => spans.push(Span::styled(r.name.clone(), fg(th.dim))),
        }
        let mut line = Line::from(spans);
        if i == app.cursor {
            line = line.style(Style::default().bg(th.selection).fg(if focused {
                th.accent
            } else {
                th.fg
            }));
        }
        lines.push(line);
    }
    if rows.is_empty() {
        lines.push(Line::from(Span::styled("(no matches)", fg(th.dim))));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn editing(app: &App, field: ReqField) -> bool {
    let active = matches!(app.mode, Mode::Edit)
        || (matches!(app.mode, Mode::Command | Mode::Search) && app.return_to == Mode::Edit);
    active && app.req_field == field
}

fn editor_theme(th: &Theme, show_cursor: bool) -> EditorTheme<'static> {
    let t = EditorTheme::default()
        .base(Style::default().bg(th.bg).fg(th.fg))
        .selection_style(Style::default().bg(th.search).fg(th.fg))
        .hide_status_line();
    if show_cursor {
        t.cursor_style(Style::default().bg(th.accent).fg(th.bg))
    } else {
        t.hide_cursor()
    }
}

/// `{{name}}` references on a line: (start, inclusive end, resolved value), in chars.
fn refs(line: &str, vars: &BTreeMap<String, String>) -> Vec<(usize, usize, Option<String>)> {
    let cs: Vec<char> = line.chars().collect();
    let mut out = vec![];
    let mut i = 0;
    while i + 1 < cs.len() {
        if cs[i] == '{'
            && cs[i + 1] == '{'
            && let Some(j) = (i + 2..cs.len() - 1).find(|&j| cs[j] == '}' && cs[j + 1] == '}')
        {
            let name: String = cs[i + 2..j].iter().collect();
            let name = name.trim();
            let val = match name.strip_prefix('$') {
                Some(e) => std::env::var(e).ok(),
                None => vars.get(name).cloned(),
            };
            out.push((i, j + 1, val));
            i = j + 2;
        } else {
            i += 1;
        }
    }
    out
}

fn hl(row: usize, from: usize, to: usize, style: Style) -> Highlight {
    Highlight::new(Index2::new(row, from), Index2::new(row, to), style)
}

/// Draw an editor whose `{{vars}}` are colored, and shown as their values off the cursor row.
fn render_editor(
    f: &mut Frame,
    ed: &mut EditorState,
    area: Rect,
    th: &Theme,
    vars: &BTreeMap<String, String>,
    editing: bool,
    blink: bool,
) {
    let text = text_of(ed);
    let mut marks = vec![];
    for (row, line) in text.lines().enumerate() {
        for (s, e, v) in refs(line, vars) {
            marks.push(hl(
                row,
                s,
                e,
                fg(if v.is_some() { th.err } else { th.warn }),
            ));
        }
    }
    ed.set_highlights(marks);
    f.render_widget(
        EditorView::new(ed)
            .theme(editor_theme(th, editing && blink))
            .wrap(false),
        area,
    );
    let reveal = editing.then_some(ed.cursor.row);
    let (ox, oy) = ed.viewport_offset();
    let buf = f.buffer_mut();
    for (i, line) in text.lines().skip(oy).take(area.height as usize).enumerate() {
        if reveal == Some(oy + i) {
            continue;
        }
        let rs = refs(line, vars);
        if rs.iter().all(|r| r.2.is_none()) {
            continue;
        }
        let cs: Vec<char> = line.chars().collect();
        let (mut cells, mut k, mut ri) = (vec![], 0, 0);
        while k < cs.len() {
            match rs.get(ri) {
                Some((s, e, v)) if *s == k => {
                    match v {
                        Some(v) => cells.extend(v.chars().map(|c| (c, fg(th.err)))),
                        None => cells.extend(cs[*s..=*e].iter().map(|&c| (c, fg(th.warn)))),
                    }
                    k = e + 1;
                    ri += 1;
                }
                _ => {
                    cells.push((cs[k], fg(th.fg)));
                    k += 1;
                }
            }
        }
        put_row(buf, area, i as u16, &cells[ox.min(cells.len())..], th);
    }
}

/// Overwrite one editor row with styled chars.
fn put_row(buf: &mut Buffer, area: Rect, i: u16, cells: &[(char, Style)], th: &Theme) {
    let y = area.y + i;
    for x in area.x..area.right() {
        let c = &mut buf[(x, y)];
        c.set_symbol(" ");
        c.set_style(Style::default().bg(th.bg).fg(th.fg));
    }
    let mut x = area.x;
    for (ch, st) in cells {
        let w = Span::raw(ch.to_string()).width() as u16;
        if x + w > area.right() {
            break;
        }
        buf.set_string(x, y, ch.to_string(), st.bg(th.bg));
        x += w;
    }
}

/// Values longer than this are shown cut off on the :vars page, except on the cursor row.
const VALUE_MAX: usize = 20;

/// `key=value` coloring for a :vars line as inclusive (from, to, color) char ranges.
fn vars_spans(line: &str, th: &Theme) -> Vec<(usize, usize, Color)> {
    let n = line.chars().count();
    if n == 0 {
        return vec![];
    }
    if line.trim_start().starts_with('#') {
        return vec![(0, n - 1, th.dim)];
    }
    let mut out = vec![];
    let kw = if line.starts_with("secret ") { 7 } else { 0 };
    if kw > 0 {
        out.push((0, 5, th.warn));
    }
    match line.chars().position(|c| c == '=') {
        Some(eq) => {
            if eq > kw {
                out.push((kw, eq - 1, th.ok));
            }
            out.push((eq, eq, th.dim));
            if eq + 1 < n {
                out.push((eq + 1, n - 1, th.fg));
            }
        }
        None if n > kw => out.push((kw, n - 1, th.ok)),
        None => {}
    }
    out
}

fn tint_vars_page(ed: &mut EditorState, th: &Theme) {
    let mut marks = vec![];
    for (row, line) in text_of(ed).lines().enumerate() {
        for (from, to, c) in vars_spans(line, th) {
            marks.push(hl(row, from, to, fg(c)));
        }
    }
    ed.set_highlights(marks);
}

/// A :vars line as styled cells, its value cut to `VALUE_MAX` chars plus `…`; None if it already fits.
fn vars_cut_cells(line: &str, th: &Theme) -> Option<Vec<(char, Style)>> {
    let eq = line.chars().position(|c| c == '=')?;
    let n = line.chars().count();
    if n - eq - 1 <= VALUE_MAX {
        return None;
    }
    let spans = vars_spans(line, th);
    let mut cells: Vec<(char, Style)> = line
        .chars()
        .take(eq + 1 + VALUE_MAX)
        .enumerate()
        .map(|(i, c)| {
            let col = spans
                .iter()
                .find(|(f, t, _)| (*f..=*t).contains(&i))
                .map_or(th.fg, |s| s.2);
            (c, fg(col))
        })
        .collect();
    cells.push(('…', fg(th.dim)));
    Some(cells)
}

fn draw_request(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let focused = app.focus == Focus::Request;
    let Some(name_dirty) = app.open.as_ref().map(|o| (o.req.name.clone(), o.dirty())) else {
        let block = pane(th, " Request ".into(), focused);
        let inner = block.inner(area);
        f.render_widget(block, area);
        let msg = Paragraph::new(vec![
            Line::from(Span::styled("No request open", fg(th.dim))),
            Line::from(Span::styled(
                "Select one in the sidebar, or press o to create one",
                fg(th.dim),
            )),
        ])
        .alignment(Alignment::Center);
        let r = Rect {
            y: inner.y + inner.height / 2 - inner.height.min(2) / 2,
            height: 2.min(inner.height),
            ..inner
        };
        f.render_widget(msg, r);
        return;
    };
    let title = format!(
        " Request · {}{} ",
        name_dirty.0,
        if name_dirty.1 { " ●" } else { "" }
    );
    let block = pane(th, title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.hit.req = inner;
    let [url_row, tabs_row, content] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(inner);
    let example_lines = app
        .open
        .as_ref()
        .and_then(|o| o.req.example.as_ref())
        .map(|ex| {
            let lang = match ex.kind {
                BodyKind::Json => Lang::Json,
                _ => Lang::Plain,
            };
            app.hl.lines(lang, &ex.content, th)
        });

    let (url_edit, content_edit) = (editing(app, ReqField::Url), editing(app, ReqField::Content));
    let (field, tab) = (app.req_field, app.req_tab);
    let (vars, blink) = (app.var_cache.clone(), app.cursor_on());
    let o = app.open.as_mut().expect("checked");

    let marker = if focused && field == ReqField::Url {
        "▸"
    } else {
        " "
    };
    let label = format!("{marker}{:<7} ", o.req.method.as_str());
    let [lbl, urlr] = Layout::horizontal([
        Constraint::Length(label.chars().count() as u16),
        Constraint::Min(1),
    ])
    .areas(url_row);
    f.render_widget(
        Paragraph::new(Span::styled(
            label,
            fg(method_color(o.req.method, th)).add_modifier(Modifier::BOLD),
        )),
        lbl,
    );
    app.hit.url = urlr;
    render_editor(f, &mut o.ed.url, urlr, th, &vars, url_edit, blink);

    // Tabs
    let mut spans = vec![];
    let mut x = tabs_row.x;
    for t in ReqTab::ALL {
        let extra = match t {
            ReqTab::Params => o
                .req
                .params
                .iter()
                .filter(|p| p.enabled)
                .count()
                .to_string(),
            ReqTab::Headers => o
                .req
                .headers
                .iter()
                .filter(|p| p.enabled)
                .count()
                .to_string(),
            ReqTab::Auth => o.req.auth.label().to_string(),
            ReqTab::Body => o.req.body.kind.label().to_string(),
            ReqTab::Example | ReqTab::Docs => String::new(),
        };
        if t == ReqTab::Docs && o.req.docs.is_none() {
            continue;
        }
        let text = if extra.is_empty() || extra == "0" || extra == "none" {
            t.label().to_string()
        } else {
            format!("{} ({extra})", t.label())
        };
        let st = if t == tab {
            fg(if focused && field == ReqField::Content {
                th.accent
            } else {
                th.fg
            })
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            fg(th.dim)
        };
        let w = text.chars().count() as u16;
        app.hit.tabs.push((Rect::new(x, tabs_row.y, w, 1), t));
        x += w + 2;
        spans.push(Span::styled(text, st));
        spans.push(Span::raw("  "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), tabs_row);

    // Content
    let hint = match tab {
        ReqTab::Params => Some("key=value per line · # disables a line".to_string()),
        ReqTab::Headers => Some("Name: value per line · # disables a line".to_string()),
        ReqTab::Auth => Some(format!("type: {} · t to change", o.req.auth.label())),
        ReqTab::Body => Some(format!("type: {} · t to change", o.req.body.kind.label())),
        ReqTab::Example => Some("spec sample · :example copies it into the body".to_string()),
        ReqTab::Docs => None,
    };
    let body_area = if let Some(h) = hint {
        let [hr, rest] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(content);
        f.render_widget(Paragraph::new(Span::styled(h, fg(th.dim))), hr);
        rest
    } else {
        content
    };
    match tab {
        ReqTab::Example => {
            let p = match example_lines {
                Some(l) => Paragraph::new(l),
                None => Paragraph::new(Span::styled(
                    "No spec example. Imported requests with a body get one.",
                    fg(th.dim),
                )),
            };
            f.render_widget(p, body_area);
        }
        ReqTab::Docs => {
            let t = o.req.docs.clone().unwrap_or_default();
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), body_area);
        }
        ReqTab::Auth if matches!(o.req.auth, Auth::None | Auth::Inherit) => {
            let msg = match (&o.req.auth, &app.ws) {
                (Auth::Inherit, Some(ws)) if ws.meta.auth != Auth::None => format!(
                    "Inherits the workspace auth ({}). Press t to pick a type.",
                    ws.meta.auth.label()
                ),
                (Auth::Inherit, _) => {
                    "Inherits the workspace auth (none set). :authws sets one; t picks a type."
                        .into()
                }
                _ => "No auth. Press t to pick a type.".into(),
            };
            f.render_widget(Paragraph::new(Span::styled(msg, fg(th.dim))), body_area);
        }
        _ => {
            if let Some(ed) = o.ed.tab_mut(tab) {
                app.hit.content = body_area;
                render_editor(f, ed, body_area, th, &vars, content_edit, blink);
            }
        }
    }
}

/// Re-style character ranges (line-relative) on a line; `(start, len, current)`.
fn overlay(line: Line<'static>, ranges: &[(usize, usize, bool)], th: &Theme) -> Line<'static> {
    let mut chars: Vec<(char, Style)> = vec![];
    for s in &line.spans {
        chars.extend(s.content.chars().map(|c| (c, s.style)));
    }
    for (start, len, cur) in ranges {
        for (_, st) in chars.iter_mut().skip(*start).take(*len) {
            *st = if *cur {
                Style::default().bg(th.accent).fg(th.bg)
            } else {
                Style::default().bg(th.search).fg(th.fg)
            };
        }
    }
    let mut spans: Vec<Span<'static>> = vec![];
    for (c, st) in chars {
        match spans.last_mut() {
            Some(last) if last.style == st => last.content.to_mut().push(c),
            _ => spans.push(Span::styled(c.to_string(), st)),
        }
    }
    Line::from(spans)
}

fn draw_response(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let focused = app.focus == Focus::Response;
    let title = if app.sending {
        " Response · sending… ".to_string()
    } else {
        " Response ".to_string()
    };
    let block = pane(th, title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.hit.resp = inner;
    if let Some(e) = &app.error {
        f.render_widget(
            Paragraph::new(Span::styled(e.clone(), fg(th.err))).wrap(Wrap { trim: false }),
            inner,
        );
        return;
    }
    let Some(r) = &app.resp else {
        let msg = if app.sending {
            "Sending…"
        } else {
            "Press Enter on a request, or Ctrl-r, to send"
        };
        f.render_widget(Paragraph::new(Span::styled(msg, fg(th.dim))), inner);
        return;
    };
    let [head, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(inner);
    app.resp_height = body.height as usize;
    let sc = match r.status {
        200..=299 => th.ok,
        300..=399 => th.info,
        400..=499 => th.warn,
        _ => th.err,
    };
    let size = if r.size >= 1024 * 1024 {
        format!("{:.1} MB", r.size as f64 / 1048576.0)
    } else if r.size >= 1024 {
        format!("{:.1} KB", r.size as f64 / 1024.0)
    } else {
        format!("{} B", r.size)
    };
    let tab = |t: RespTab, label: &str| {
        Span::styled(
            label.to_string(),
            if app.resp_tab == t {
                fg(th.accent).add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
            } else {
                fg(th.dim)
            },
        )
    };
    let head_line = Line::from(vec![
        Span::styled(
            format!("{} {}", r.status, r.reason),
            fg(sc).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!("  {} ms  {size}  ", r.elapsed_ms)),
        tab(RespTab::Body, "Body"),
        Span::raw("  "),
        tab(RespTab::Headers, "Headers"),
        Span::styled(
            if r.matches.is_empty() {
                String::new()
            } else {
                format!("  [{}/{}]", r.cur_match + 1, r.matches.len())
            },
            fg(th.info),
        ),
    ]);
    let lead: usize = head_line.spans[..2].iter().map(Span::width).sum();
    let bx = head.x + lead as u16;
    app.hit.resp_tabs = vec![
        (Rect::new(bx, head.y, 4, 1), RespTab::Body),
        (Rect::new(bx + 6, head.y, 7, 1), RespTab::Headers),
    ];
    f.render_widget(Paragraph::new(head_line), head);

    let h = body.height as usize;
    let mut lines: Vec<Line<'static>> = vec![];
    match app.resp_tab {
        RespTab::Body => {
            let w = r.lines.len().to_string().len();
            if r.truncated && app.resp_scroll == 0 {
                lines.push(Line::from(Span::styled(
                    "(body truncated at 10 MB; :save <path> writes the full body)",
                    fg(th.warn),
                )));
            }
            for (i, l) in r.lines.iter().enumerate().skip(app.resp_scroll).take(h) {
                let ranges: Vec<_> = r
                    .matches
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.0 == i)
                    .map(|(mi, m)| (m.1, m.2, mi == r.cur_match))
                    .collect();
                let l = if ranges.is_empty() {
                    l.clone()
                } else {
                    overlay(l.clone(), &ranges, th)
                };
                let mut spans = vec![Span::styled(format!("{:>w$} ", i + 1), fg(th.dim))];
                spans.extend(l.spans);
                lines.push(Line::from(spans));
            }
        }
        RespTab::Headers => {
            for (k, v) in r.headers.iter().skip(app.resp_scroll).take(h) {
                lines.push(Line::from(vec![
                    Span::styled(format!("{k}: "), fg(th.key)),
                    Span::styled(v.clone(), fg(th.string)),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), body);
}

fn draw_bottom(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let label = format!(" {} ", app.mode_label());
    let pending: String = app
        .pending
        .iter()
        .map(|k| {
            format!(
                "{}{}",
                if k.ctrl { "^" } else { "" },
                match k.code {
                    crossterm::event::KeyCode::Char(c) => c.to_string(),
                    c => format!("{c:?}"),
                }
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let hints = match (app.mode, app.focus) {
        (Mode::Vars, _) => "Ctrl-s save · Esc close · i insert",
        (Mode::Picker, _) => "j/k move · Enter open · click to open · Esc close",
        (Mode::Edit, _) => "Esc leave · Ctrl-r send · Ctrl-s save · : command",
        (_, Focus::Tree) => "Enter open+send · o new · dd delete · / filter · : command · ? help",
        (_, Focus::Request) => {
            "i edit · h/l tab · j/k field · m method · t type · Ctrl-r send · : command"
        }
        (_, Focus::Response) => {
            "j/k scroll · h/l Body/Headers · / search · n/N next · y curl · : command"
        }
    };
    let (msg, st) = match &app.status {
        Some(s) => (s.text.clone(), fg(if s.err { th.err } else { th.ok })),
        None => (hints.to_string(), fg(th.dim)),
    };
    let bar = Style::default().bg(th.selection);
    let spans = vec![
        Span::styled(
            label,
            Style::default()
                .bg(th.accent)
                .fg(th.bg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {pending}{}", if pending.is_empty() { "" } else { " " }),
            bar.fg(th.warn),
        ),
        Span::styled(msg, bar.patch(st)),
    ];
    f.render_widget(Paragraph::new(Line::from(spans)).style(bar), area);
}

fn popup_rect(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width.saturating_sub(2));
    let h = h.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height / 5).min(area.height.saturating_sub(h)),
        width: w,
        height: h,
    }
}

fn draw_popup(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let searching = app.mode == Mode::Search;
    let comps = if searching { vec![] } else { app.completions() };
    app.sel = app.sel.min(comps.len().saturating_sub(1));
    let shown = comps.len().min(8);
    let title = if searching {
        match app.search_focus {
            Focus::Tree => " Search · Requests ",
            Focus::Response => " Search · Response ",
            Focus::Request => " Search · Editor ",
        }
    } else {
        " Command "
    };
    let rect = popup_rect(area, 66, 4 + shown as u16 + u16::from(searching));
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(fg(th.accent))
        .title(Span::styled(
            title,
            fg(th.accent).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(th.bg).fg(th.fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let prompt = if searching { "/" } else { ":" };
    let mut lines = vec![Line::from(vec![
        Span::styled(prompt, fg(th.accent)),
        Span::raw(app.input.clone()),
        Span::styled("█", fg(th.accent)),
    ])];
    if searching {
        let n = match app.search_focus {
            Focus::Tree => format!("{} matching requests", app.rows().len()),
            Focus::Response => app
                .resp
                .as_ref()
                .map(|r| format!("{} matches", r.matches.len()))
                .unwrap_or_else(|| "no response".into()),
            Focus::Request => String::new(),
        };
        lines.push(Line::from(Span::styled(n, fg(th.dim))));
    }
    let off = app.sel.saturating_sub(shown.saturating_sub(1));
    for (i, c) in comps.iter().enumerate().skip(off).take(shown) {
        let st = if i == app.sel {
            Style::default().bg(th.selection)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::styled(
                format!("{:<26}", c.label),
                st.fg(if i == app.sel { th.accent } else { th.fg }),
            ),
            Span::styled(c.desc.clone(), st.fg(th.dim)),
        ]));
    }
    if !searching && comps.is_empty() {
        lines.push(Line::from(Span::styled("no matching command", fg(th.dim))));
    }
    f.render_widget(Paragraph::new(lines), inner);
    let hint = if searching {
        "Enter confirm · Esc cancel · n/N next"
    } else {
        "Tab complete · ↑↓ select · Enter run · Esc cancel"
    };
    let hr = Rect {
        y: rect.y + rect.height - 1,
        x: rect.x + 2,
        width: rect.width.saturating_sub(4),
        height: 1,
    };
    f.render_widget(
        Paragraph::new(Span::styled(format!(" {hint} "), fg(th.dim)))
            .style(Style::default().bg(th.bg)),
        hr,
    );
}

fn draw_confirm(f: &mut Frame, app: &App, th: &Theme, area: Rect) {
    let rect = popup_rect(area, 50, 3);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(fg(th.warn))
        .title(" Confirm ")
        .style(Style::default().bg(th.bg).fg(th.fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    f.render_widget(
        Paragraph::new(format!("{} (y/n)", app.confirm.clone().unwrap_or_default())),
        inner,
    );
}

fn draw_picker(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let cur = app.ws.as_ref().map(|w| w.root.clone());
    let Some(p) = &mut app.picker else { return };
    let h = (p.items.len() as u16 + 2)
        .min(area.height.saturating_sub(4))
        .max(3);
    let rect = popup_rect(area, 76, h);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(fg(th.accent))
        .title(Span::styled(
            " Workspaces ",
            fg(th.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(" Enter open · Esc close ", fg(th.dim)))
        .style(Style::default().bg(th.bg).fg(th.fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let rows = inner.height as usize;
    p.offset = p.offset.min(p.sel).max((p.sel + 1).saturating_sub(rows));
    let width = inner.width as usize;
    let lines: Vec<Line> = p
        .items
        .iter()
        .enumerate()
        .skip(p.offset)
        .take(rows)
        .map(|(i, (name, path))| {
            let here = cur
                .as_deref()
                .is_some_and(|c| crate::app::same_dir(c, path));
            let mark = if here { "● " } else { "  " };
            let used = mark.chars().count() + name.chars().count() + 2;
            let mut path = path.display().to_string();
            let room = width.saturating_sub(used);
            if path.chars().count() > room {
                let keep = room.saturating_sub(1);
                path = format!(
                    "…{}",
                    path.chars()
                        .skip(path.chars().count() - keep)
                        .collect::<String>()
                );
            }
            let row = if i == p.sel {
                Style::default().bg(th.selection)
            } else {
                Style::default()
            };
            Line::from(vec![
                Span::styled(mark, fg(th.ok)),
                Span::styled(name.clone(), fg(th.accent).add_modifier(Modifier::BOLD)),
                Span::styled(format!("  {path}"), fg(th.dim)),
            ])
            .style(row)
        })
        .collect();
    app.hit.picker = inner;
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_vars(f: &mut Frame, app: &mut App, th: &Theme, area: Rect) {
    let blink = app.cursor_on();
    let Some(v) = &mut app.vars else { return };
    let text = text_of(&v.ed);
    // Widen for the cursor row so a long value can be read in full.
    let need = text
        .lines()
        .nth(v.ed.cursor.row)
        .map_or(0, |l| l.chars().count() + 3);
    let rect = popup_rect(area, 80.max(need as u16), area.height * 3 / 5);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(fg(th.accent))
        .title(Span::styled(
            format!(" Variables: {} ", v.env),
            fg(th.accent).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Span::styled(
            " key=value · secret key=value · Ctrl-s save · Esc close ",
            fg(th.dim),
        ))
        .style(Style::default().bg(th.bg).fg(th.fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    tint_vars_page(&mut v.ed, th);
    f.render_widget(
        EditorView::new(&mut v.ed)
            .theme(editor_theme(th, blink))
            .wrap(false),
        inner,
    );
    let (ox, oy) = v.ed.viewport_offset();
    let buf = f.buffer_mut();
    for (i, line) in text
        .lines()
        .skip(oy)
        .take(inner.height as usize)
        .enumerate()
    {
        if oy + i == v.ed.cursor.row {
            continue;
        }
        if let Some(cells) = vars_cut_cells(line, th) {
            put_row(buf, inner, i as u16, &cells[ox.min(cells.len())..], th);
        }
    }
}

fn draw_help(f: &mut Frame, th: &Theme, area: Rect) {
    const ROWS: &[(&str, &str)] = &[
        ("h j k l", "move / collapse / expand / switch tab"),
        ("gg  G", "top / bottom"),
        ("Ctrl-d  Ctrl-u", "half page down / up"),
        ("Ctrl-w hjkl", "focus pane (Ctrl-hjkl also works)"),
        ("Ctrl-w w", "next pane"),
        ("Tab  Shift-Tab", "next / previous tab"),
        ("Enter", "open and send the request"),
        ("i  a", "edit the URL or the open tab (Vim editor)"),
        ("Esc", "leave editing / clear filter"),
        ("Ctrl-r", "send request"),
        ("Ctrl-s", "save request"),
        (":", "command box (Tab completes, shows what each does)"),
        ("/  n  N", "search the focused pane, next, previous"),
        ("o", "new request"),
        ("dd", "delete request"),
        ("m  t", "cycle method / auth or body type"),
        ("e", "next environment"),
        ("y  Y", "copy request as curl / copy the response"),
        ("W", "pick a workspace (also :ws with no argument)"),
        ("l  h  zR  zM", "expand / collapse workspace rows / all"),
        (
            "Ctrl-w < > + -",
            "resize panes (= resets); drag borders too",
        ),
        ("q  Ctrl-c", "quit"),
    ];
    let rect = popup_rect(area, 70, ROWS.len() as u16 + 4);
    f.render_widget(Clear, rect);
    let block = Block::bordered()
        .border_style(fg(th.accent))
        .title(Span::styled(
            " Key bindings ",
            fg(th.accent).add_modifier(Modifier::BOLD),
        ))
        .style(Style::default().bg(th.bg).fg(th.fg));
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    let mut lines: Vec<Line> = ROWS
        .iter()
        .map(|(k, d)| {
            Line::from(vec![
                Span::styled(format!("{k:<16}"), fg(th.accent)),
                Span::raw(*d),
            ])
        })
        .collect();
    lines.push(Line::from(Span::styled(
        "Remap in ~/.config/turly/config.toml under [keys]. Any key closes this.",
        fg(th.dim),
    )));
    f.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::model::Request;
    use crate::workspace::Workspace;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn long_values_are_cut() {
        let th = Theme::preset("turly").unwrap();
        let cut =
            |l: &str| vars_cut_cells(l, &th).map(|c| c.iter().map(|x| x.0).collect::<String>());
        assert_eq!(cut("a=short"), None);
        assert_eq!(cut("a=01234567890123456789"), None);
        assert_eq!(
            cut("secret tok=0123456789012345678901234"),
            Some("secret tok=01234567890123456789…".into())
        );
    }

    fn render(app: &mut App) -> String {
        let mut t = Terminal::new(TestBackend::new(110, 30)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app_with_request() -> (App, tempfile::TempDir, tokio::runtime::Runtime) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let d = tempfile::tempdir().unwrap();
        let ws = Workspace::init(d.path(), "demo").unwrap();
        let p = ws.new_request_path(&ws.requests_dir(), "List pets");
        ws.save_request(&p, &Request::new("List pets")).unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let app = App::new(Some(ws), Default::default(), rt.handle().clone(), tx);
        (app, d, rt)
    }

    #[test]
    fn renders_panes_and_command_box() {
        let (mut app, _d, _rt) = app_with_request();
        let s = render(&mut app);
        assert!(
            s.contains("Requests") && s.contains("List pets") && s.contains("workspace demo"),
            "{s}"
        );
        assert!(s.contains("No request open"));

        app.open_popup(Mode::Command, "w");
        let s = render(&mut app);
        assert!(
            s.contains("Command") && s.contains("Save current request") && !s.contains("Quit out"),
            "{s}"
        );
        app.input = "qa".into();
        assert!(render(&mut app).contains("Quit out"));
    }

    #[test]
    fn several_workspaces_expand_independently() {
        let d = tempfile::tempdir().unwrap();
        for n in ["a", "b"] {
            let ws = Workspace::init(&d.path().join(n), n).unwrap();
            let p = ws.new_request_path(&ws.requests_dir(), &format!("req-{n}"));
            ws.save_request(&p, &Request::new(&format!("req-{n}")))
                .unwrap();
        }
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let ws = Workspace::open(&d.path().join("a")).unwrap();
        let mut app = App::new(Some(ws), Default::default(), rt.handle().clone(), tx);
        app.set_scope(d.path());
        let names = |app: &App| app.rows().into_iter().map(|r| r.name).collect::<Vec<_>>();
        assert_eq!(names(&app), ["a", "req-a", "history", "b"]);
        let b = app.scope[1].1.clone();
        app.expand_workspace(&b, true);
        assert_eq!(names(&app), ["a", "req-a", "history", "b", "req-b"]);
        app.action_for_test(crate::keymap::Action::CollapseAll);
        assert_eq!(names(&app), ["a", "b"]);
        app.action_for_test(crate::keymap::Action::ExpandAll);
        assert_eq!(names(&app), ["a", "req-a", "history", "b", "req-b"]);
    }

    #[test]
    fn command_enter_completes_and_theme_previews() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(None, Default::default(), rt.handle().clone(), tx);
        let key = |app: &mut App, c| app.on_key(KeyEvent::new(c, KeyModifiers::NONE));
        app.open_popup(Mode::Command, "set");
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.input, "set ");
        assert_eq!(app.mode, Mode::Command);

        let before = app.theme.bg;
        app.open_popup(Mode::Command, "theme ");
        let names = Theme::all_names();
        let other = names
            .iter()
            .find(|n| Theme::load(n).unwrap().bg != before)
            .unwrap();
        app.input = format!("theme {other}");
        key(&mut app, KeyCode::Char('x'));
        key(&mut app, KeyCode::Backspace);
        assert_ne!(app.theme.bg, before);
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.theme.bg, before);
    }

    #[test]
    fn renders_splash_without_workspace() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = App::new(None, Default::default(), rt.handle().clone(), tx);
        let s = render(&mut app);
        assert!(
            s.contains("a terminal API client") && s.contains(":import"),
            "{s}"
        );
    }
}
