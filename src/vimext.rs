//! Vim motions edtui lacks: `F T ; , ^ C` and `d`/`c`/`y` + `$ 0 ^ e b f F t T ; ,`.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use edtui::{EditorEventHandler, EditorMode, EditorState};

#[derive(Default)]
pub struct Vim {
    buf: String,
    /// Last `f F t T`: (char, forward, till).
    last: Option<(char, bool, bool)>,
}

#[derive(Clone, Copy)]
enum Motion {
    Find(char, bool, bool),
    Repeat(bool),
    End,
    Start,
    FirstNonBlank,
    WordEnd,
    WordBack,
}

fn class(c: char) -> u8 {
    if c.is_whitespace() {
        0
    } else if c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

/// Target column of `m` from `col`, or None when it does not apply.
fn target(
    cs: &[char],
    col: usize,
    m: Motion,
    n: usize,
    last: Option<(char, bool, bool)>,
) -> Option<usize> {
    let (m, repeat) = match m {
        Motion::Repeat(rev) => {
            let (ch, fwd, till) = last?;
            (Motion::Find(ch, fwd != rev, till), true)
        }
        m => (m, false),
    };
    match m {
        Motion::Find(ch, fwd, till) => {
            // Repeating a till must not stay stuck on the adjacent match.
            let skip = usize::from(repeat && till);
            let hits: Vec<usize> = if fwd {
                (col + 1 + skip..cs.len())
                    .filter(|&i| cs[i] == ch)
                    .collect()
            } else {
                (0..col.saturating_sub(skip))
                    .rev()
                    .filter(|&i| cs[i] == ch)
                    .collect()
            };
            let p = *hits.get(n - 1)?;
            Some(match (fwd, till) {
                (true, true) => p - 1,
                (false, true) => p + 1,
                _ => p,
            })
        }
        Motion::Repeat(_) => None,
        Motion::End => cs.len().checked_sub(1),
        Motion::Start => Some(0),
        Motion::FirstNonBlank => Some(
            cs.iter()
                .position(|c| !c.is_whitespace())
                .unwrap_or(cs.len().saturating_sub(1)),
        ),
        Motion::WordEnd => {
            let mut i = col;
            for _ in 0..n {
                i += 1;
                while cs.get(i).is_some_and(|c| c.is_whitespace()) {
                    i += 1;
                }
                let k = class(*cs.get(i)?);
                while cs.get(i + 1).is_some_and(|&c| class(c) == k) {
                    i += 1;
                }
            }
            Some(i)
        }
        Motion::WordBack => {
            let mut i = col;
            for _ in 0..n {
                i = i.checked_sub(1)?;
                while cs[i].is_whitespace() {
                    i = i.checked_sub(1)?;
                }
                let k = class(cs[i]);
                while i > 0 && class(cs[i - 1]) == k {
                    i -= 1;
                }
            }
            Some(i)
        }
    }
}

/// Inclusive char range an operator covers, vim-style (backward motions exclude the cursor char).
fn range(
    col: usize,
    t: usize,
    m: Motion,
    last: Option<(char, bool, bool)>,
) -> Option<(usize, usize)> {
    let forward = match m {
        Motion::Find(_, fwd, _) => fwd,
        Motion::Repeat(rev) => last.is_some_and(|(_, fwd, _)| fwd != rev),
        Motion::End | Motion::WordEnd => true,
        Motion::Start | Motion::FirstNonBlank | Motion::WordBack => false,
    };
    if forward {
        (t >= col).then_some((col, t))
    } else {
        (t < col).then_some((t, col - 1))
    }
}

fn press(h: &mut EditorEventHandler, ed: &mut EditorState, c: char) {
    let m = if c.is_uppercase() {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    h.on_key_event(KeyEvent::new(KeyCode::Char(c), m), ed);
}

impl Vim {
    pub fn pending(&self) -> bool {
        !self.buf.is_empty()
    }

    pub fn reset(&mut self) {
        self.buf.clear();
    }

    /// Feed a key in editor Normal mode; false means edtui should handle it.
    pub fn key(&mut self, h: &mut EditorEventHandler, ed: &mut EditorState, ke: KeyEvent) -> bool {
        let KeyCode::Char(c) = ke.code else {
            return false;
        };
        if ke
            .modifiers
            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
        {
            return false;
        }
        let starts = matches!(
            c,
            '1'..='9' | 'F' | 'T' | 'f' | 't' | ';' | ',' | '^' | 'd' | 'c' | 'y' | 'C'
        );
        if !self.pending() && !starts {
            return false;
        }
        self.buf.push(c);
        let digits = self.buf.chars().take_while(char::is_ascii_digit).count();
        let n: usize = self.buf[..digits].parse().unwrap_or(1).max(1);
        let rest: Vec<char> = self.buf[digits..].chars().collect();
        match self.parse(&rest) {
            Parse::Wait => {}
            Parse::Pass => self.replay(h, ed),
            Parse::Go(op, m) => {
                self.buf.clear();
                self.run(h, ed, op, m, n);
            }
        }
        true
    }

    fn replay(&mut self, h: &mut EditorEventHandler, ed: &mut EditorState) {
        for c in std::mem::take(&mut self.buf).chars() {
            press(h, ed, c);
        }
    }

    fn parse(&self, r: &[char]) -> Parse {
        let motion = |r: &[char]| -> Option<Option<Motion>> {
            Some(Some(match *r.first()? {
                '$' => Motion::End,
                '0' => Motion::Start,
                '^' => Motion::FirstNonBlank,
                'e' => Motion::WordEnd,
                'b' => Motion::WordBack,
                ';' => Motion::Repeat(false),
                ',' => Motion::Repeat(true),
                k @ ('f' | 'F' | 't' | 'T') => {
                    let ch = *r.get(1)?;
                    Motion::Find(ch, k.is_ascii_lowercase(), k.eq_ignore_ascii_case(&'t'))
                }
                _ => return Some(None),
            }))
        };
        match r.first() {
            None => Parse::Wait,
            Some('d' | 'c' | 'y') => match motion(&r[1..]) {
                None if r.len() == 1 || r[1..].len() == 1 && "fFtT".contains(r[1]) => Parse::Wait,
                Some(Some(m)) => Parse::Go(Some(r[0]), m),
                _ => Parse::Pass,
            },
            Some('C') => Parse::Go(Some('c'), Motion::End),
            _ => match motion(r) {
                None => Parse::Wait,
                Some(Some(m)) => Parse::Go(None, m),
                Some(None) => Parse::Pass,
            },
        }
    }

    fn run(
        &mut self,
        h: &mut EditorEventHandler,
        ed: &mut EditorState,
        op: Option<char>,
        m: Motion,
        n: usize,
    ) {
        if let Motion::Find(ch, fwd, till) = m {
            self.last = Some((ch, fwd, till));
        }
        let line: Vec<char> = String::from(ed.lines.clone())
            .split('\n')
            .nth(ed.cursor.row)
            .unwrap_or("")
            .chars()
            .collect();
        let col = ed.cursor.col.min(line.len().saturating_sub(1));
        let Some(t) = target(&line, col, m, n, self.last) else {
            return;
        };
        let Some(op) = op else {
            ed.cursor.col = t;
            return;
        };
        let Some((lo, hi)) = range(col, t, m, self.last) else {
            return;
        };
        ed.cursor.col = lo;
        press(h, ed, 'v');
        for _ in lo..hi {
            press(h, ed, 'l');
        }
        press(h, ed, op);
        if op == 'c' && ed.mode != EditorMode::Insert {
            press(h, ed, 'i');
        }
    }
}

enum Parse {
    Wait,
    Pass,
    Go(Option<char>, Motion),
}

#[cfg(test)]
mod tests {
    use super::*;
    use edtui::Lines;

    const T: &str = "foo bar Baz qux.quux";

    fn run(start: usize, keys: &str) -> (String, usize) {
        let mut ed = EditorState::new(Lines::from(T));
        ed.cursor.col = start;
        let (mut h, mut v) = (EditorEventHandler::default(), Vim::default());
        for c in keys.chars() {
            let m = if c.is_uppercase() {
                KeyModifiers::SHIFT
            } else {
                KeyModifiers::NONE
            };
            let code = if c == '\x1b' {
                KeyCode::Esc
            } else {
                KeyCode::Char(c)
            };
            let ke = KeyEvent::new(code, m);
            if !v.key(&mut h, &mut ed, ke) {
                h.on_key_event(ke, &mut ed);
            }
        }
        (String::from(ed.lines.clone()), ed.cursor.col)
    }

    #[test]
    fn find_motions() {
        assert_eq!(run(19, "Fq").1, 16);
        assert_eq!(run(19, "FB").1, 8);
        assert_eq!(run(19, "TB").1, 9);
        assert_eq!(run(0, "fq;").1, 16);
        assert_eq!(run(0, "tq;").1, 15);
        assert_eq!(run(19, "Fq;").1, 12);
        assert_eq!(run(4, "^").1, 0);
    }

    #[test]
    fn operators() {
        assert_eq!(run(4, "d$").0, "foo ");
        assert_eq!(run(4, "d0").0, "bar Baz qux.quux");
        assert_eq!(run(8, "db").0, "foo Baz qux.quux");
        assert_eq!(run(0, "de").0, " bar Baz qux.quux");
        assert_eq!(run(0, "dtB").0, "Baz qux.quux");
        assert_eq!(run(0, "dfB").0, "az qux.quux");
        assert_eq!(
            run(19, "dFq").0,
            "foo bar Baz qux.quux"[..16].to_string() + "x"
        );
        assert_eq!(run(19, "dTq").0, "foo bar Baz qux.qx");
        assert_eq!(run(4, "C!\x1b").0, "foo !");
        assert_eq!(run(0, "ctBX\x1b").0, "XBaz qux.quux");
    }

    #[test]
    fn falls_through_to_edtui() {
        assert_eq!(run(0, "dw").0, "bar Baz qux.quux");
        assert_eq!(run(0, "2dw").0, "bar Baz qux.quux");
        assert_eq!(run(0, "dd").0, "");
    }
}
