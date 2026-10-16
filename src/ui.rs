//! Rendering.

use crate::app::{App, Focus, Mode, ReqField, ReqTab, RespTab, RowKind};
use crate::model::{Auth, Method};
use crate::theme::Theme;
use edtui::{EditorTheme, EditorView};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};

const LOGO: &str = r"      ▄███████▄
    ▄█▀ ▄▀▀▀▄ ▀█▄
   ██  █ >_  █  ██▄▄
    ▀█▄ ▀▄▄▄▀ ▄█▀  ▀
      ▀███████▀
        ▀▀ ▀▀";

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
        let w = (main.width / 4).clamp(24, 44);
        let [side, right] =
            Layout::horizontal([Constraint::Length(w), Constraint::Min(10)]).areas(main);
        let [req, resp] =
            Layout::vertical([Constraint::Percentage(52), Constraint::Percentage(48)]).areas(right);
        draw_tree(f, app, &th, side);
        draw_request(f, app, &th, req);
        draw_response(f, app, &th, resp);
    }
    draw_bottom(f, app, &th, bot);
    match app.mode {
        Mode::Command | Mode::Search => draw_popup(f, app, &th, area),
        Mode::Confirm => draw_confirm(f, app, &th, area),
        Mode::Help => draw_help(f, &th, area),
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
    let [url_row, tabs_row, content] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(1),
    ])
    .areas(inner);

    let (url_edit, content_edit) = (editing(app, ReqField::Url), editing(app, ReqField::Content));
    let (field, tab) = (app.req_field, app.req_tab);
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
    f.render_widget(
        EditorView::new(&mut o.ed.url).theme(editor_theme(th, url_edit)),
        urlr,
    );

    // Tabs
    let mut spans = vec![];
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
            ReqTab::Docs => String::new(),
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
        ReqTab::Docs => {
            let t = o.req.docs.clone().unwrap_or_default();
            f.render_widget(Paragraph::new(t).wrap(Wrap { trim: false }), body_area);
        }
        ReqTab::Auth if matches!(o.req.auth, Auth::None) => {
            f.render_widget(
                Paragraph::new(Span::styled("No auth. Press t to pick a type.", fg(th.dim))),
                body_area,
            );
        }
        _ => {
            if let Some(ed) = o.ed.tab_mut(tab) {
                f.render_widget(
                    EditorView::new(ed)
                        .theme(editor_theme(th, content_edit))
                        .wrap(true),
                    body_area,
                );
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

fn draw_help(f: &mut Frame, th: &Theme, area: Rect) {
    const ROWS: &[(&str, &str)] = &[
        ("h j k l", "move / collapse / expand / switch tab"),
        ("gg  G", "top / bottom"),
        ("Ctrl-d  Ctrl-u", "half page down / up"),
        ("Ctrl-w hjkl", "focus pane (Ctrl-hjkl also works)"),
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
        ("y", "copy request as curl"),
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
