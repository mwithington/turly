//! Key bindings: Vim-style defaults, overridable from `config.toml`.

use anyhow::{Result, bail};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Key {
    pub code: KeyCode,
    pub ctrl: bool,
}

impl From<KeyEvent> for Key {
    fn from(e: KeyEvent) -> Key {
        Key {
            code: e.code,
            ctrl: e.modifiers.contains(KeyModifiers::CONTROL),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Action {
    Up,
    Down,
    Left,
    Right,
    Top,
    Bottom,
    HalfDown,
    HalfUp,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    FocusNext,
    NextTab,
    PrevTab,
    Open,
    Edit,
    Send,
    Save,
    Command,
    Search,
    SearchNext,
    SearchPrev,
    Cancel,
    Quit,
    Help,
    NewRequest,
    DeleteRequest,
    CycleMethod,
    CycleKind,
    NextEnv,
    CopyCurl,
    CopyResponse,
    PaneWider,
    PaneNarrower,
    PaneTaller,
    PaneShorter,
    PaneReset,
    Workspaces,
    CollapseAll,
    ExpandAll,
}

const NAMES: &[(&str, Action)] = &[
    ("up", Action::Up),
    ("down", Action::Down),
    ("left", Action::Left),
    ("right", Action::Right),
    ("top", Action::Top),
    ("bottom", Action::Bottom),
    ("half_down", Action::HalfDown),
    ("half_up", Action::HalfUp),
    ("focus_left", Action::FocusLeft),
    ("focus_right", Action::FocusRight),
    ("focus_up", Action::FocusUp),
    ("focus_down", Action::FocusDown),
    ("focus_next", Action::FocusNext),
    ("next_tab", Action::NextTab),
    ("prev_tab", Action::PrevTab),
    ("open", Action::Open),
    ("edit", Action::Edit),
    ("send", Action::Send),
    ("save", Action::Save),
    ("command", Action::Command),
    ("search", Action::Search),
    ("search_next", Action::SearchNext),
    ("search_prev", Action::SearchPrev),
    ("cancel", Action::Cancel),
    ("quit", Action::Quit),
    ("help", Action::Help),
    ("new_request", Action::NewRequest),
    ("delete_request", Action::DeleteRequest),
    ("cycle_method", Action::CycleMethod),
    ("cycle_kind", Action::CycleKind),
    ("next_env", Action::NextEnv),
    ("copy_curl", Action::CopyCurl),
    ("copy_response", Action::CopyResponse),
    ("pane_wider", Action::PaneWider),
    ("pane_narrower", Action::PaneNarrower),
    ("pane_taller", Action::PaneTaller),
    ("pane_shorter", Action::PaneShorter),
    ("pane_reset", Action::PaneReset),
    ("workspaces", Action::Workspaces),
    ("collapse_all", Action::CollapseAll),
    ("expand_all", Action::ExpandAll),
];

impl Action {
    pub fn parse(s: &str) -> Option<Action> {
        NAMES.iter().find(|(n, _)| *n == s).map(|(_, a)| *a)
    }
}

/// Default Vim-style bindings. Chords are space-separated.
const DEFAULTS: &[(&str, Action)] = &[
    ("j", Action::Down),
    ("down", Action::Down),
    ("k", Action::Up),
    ("up", Action::Up),
    ("h", Action::Left),
    ("left", Action::Left),
    ("l", Action::Right),
    ("right", Action::Right),
    ("g g", Action::Top),
    ("G", Action::Bottom),
    ("ctrl-d", Action::HalfDown),
    ("ctrl-u", Action::HalfUp),
    ("ctrl-w h", Action::FocusLeft),
    ("ctrl-w l", Action::FocusRight),
    ("ctrl-w k", Action::FocusUp),
    ("ctrl-w j", Action::FocusDown),
    ("ctrl-w w", Action::FocusNext),
    ("ctrl-w ctrl-w", Action::FocusNext),
    ("ctrl-w >", Action::PaneWider),
    ("ctrl-w <", Action::PaneNarrower),
    ("ctrl-w +", Action::PaneTaller),
    ("ctrl-w -", Action::PaneShorter),
    ("ctrl-w =", Action::PaneReset),
    ("ctrl-h", Action::FocusLeft),
    ("ctrl-l", Action::FocusRight),
    ("ctrl-k", Action::FocusUp),
    ("ctrl-j", Action::FocusDown),
    ("tab", Action::NextTab),
    ("backtab", Action::PrevTab),
    ("enter", Action::Open),
    ("i", Action::Edit),
    ("a", Action::Edit),
    ("ctrl-r", Action::Send),
    ("ctrl-s", Action::Save),
    (":", Action::Command),
    ("/", Action::Search),
    ("n", Action::SearchNext),
    ("N", Action::SearchPrev),
    ("esc", Action::Cancel),
    ("q", Action::Quit),
    ("ctrl-c", Action::Quit),
    ("?", Action::Help),
    ("o", Action::NewRequest),
    ("d d", Action::DeleteRequest),
    ("m", Action::CycleMethod),
    ("t", Action::CycleKind),
    ("e", Action::NextEnv),
    ("y", Action::CopyCurl),
    ("Y", Action::CopyResponse),
    ("W", Action::Workspaces),
    ("z M", Action::CollapseAll),
    ("z R", Action::ExpandAll),
];

pub fn parse_key(s: &str) -> Result<Key> {
    let (ctrl, rest) = match s.strip_prefix("ctrl-") {
        Some(r) => (true, r),
        None => (false, s),
    };
    let code = match rest {
        "enter" => KeyCode::Enter,
        "esc" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "backspace" => KeyCode::Backspace,
        r if r.chars().count() == 1 => KeyCode::Char(r.chars().next().unwrap()),
        _ => bail!("unknown key '{s}'"),
    };
    Ok(Key { code, ctrl })
}

pub fn parse_chord(s: &str) -> Result<Vec<Key>> {
    s.split_whitespace().map(parse_key).collect()
}

pub enum Lookup {
    None,
    Prefix,
    Action(Action),
}

pub struct KeyMap {
    map: HashMap<Vec<Key>, Action>,
}

impl KeyMap {
    pub fn new(overrides: &std::collections::BTreeMap<String, String>) -> Result<KeyMap> {
        let mut map = HashMap::new();
        for (k, a) in DEFAULTS {
            map.insert(parse_chord(k)?, *a);
        }
        for (k, a) in overrides {
            let Some(action) = Action::parse(a) else {
                bail!("unknown action '{a}' for key '{k}'")
            };
            map.insert(parse_chord(k)?, action);
        }
        Ok(KeyMap { map })
    }

    pub fn lookup(&self, seq: &[Key]) -> Lookup {
        if let Some(a) = self.map.get(seq) {
            return Lookup::Action(*a);
        }
        if self
            .map
            .keys()
            .any(|k| k.len() > seq.len() && k.starts_with(seq))
        {
            return Lookup::Prefix;
        }
        Lookup::None
    }

    /// Action bound to a single key press (used for chord-free lookups in edit mode).
    pub fn single(&self, k: Key) -> Option<Action> {
        self.map.get(&vec![k]).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn km() -> KeyMap {
        KeyMap::new(&Default::default()).unwrap()
    }

    #[test]
    fn chords_and_prefixes() {
        let m = km();
        let g = parse_chord("g").unwrap();
        assert!(matches!(m.lookup(&g), Lookup::Prefix));
        let gg = parse_chord("g g").unwrap();
        assert!(matches!(m.lookup(&gg), Lookup::Action(Action::Top)));
        assert!(matches!(m.lookup(&parse_chord("x").unwrap()), Lookup::None));
    }

    #[test]
    fn resize_and_copy_defaults() {
        let m = km();
        for (k, a) in [
            ("ctrl-w >", Action::PaneWider),
            ("ctrl-w =", Action::PaneReset),
            ("Y", Action::CopyResponse),
        ] {
            assert!(matches!(m.lookup(&parse_chord(k).unwrap()), Lookup::Action(x) if x == a));
        }
    }

    #[test]
    fn overrides_apply_and_validate() {
        let mut o = std::collections::BTreeMap::new();
        o.insert("x".to_string(), "quit".to_string());
        assert!(matches!(
            KeyMap::new(&o).unwrap().lookup(&parse_chord("x").unwrap()),
            Lookup::Action(Action::Quit)
        ));
        o.insert("y".to_string(), "bogus".to_string());
        assert!(KeyMap::new(&o).is_err());
    }
}
