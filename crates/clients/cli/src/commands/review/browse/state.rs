//! The review screen's state and key handling: which file is shown, the
//! diff's scroll, the thread list, and text being typed. Pure: keys in,
//! actions out, so it is tested without a terminal.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Which pane keys go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Files,
    Threads,
}

/// What the keyboard is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Moving around.
    Browse,
    /// Typing a comment on the shown file.
    Comment(String),
    /// Asked to confirm a reject.
    ConfirmReject,
}

/// What a key asks the caller to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Quit,
    Approve,
    Reject,
    /// Comment on file `path` with `body`.
    Comment {
        path: String,
        body: String,
    },
    /// Resolve (`true`) or reopen thread `id`.
    Resolve {
        id: String,
        resolved: bool,
    },
}

/// A thread as the list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadRow {
    pub id: String,
    pub resolved: bool,
    pub text: String,
}

/// The screen's state.
#[derive(Debug, Clone)]
pub struct App {
    pub files: Vec<String>,
    pub selected: usize,
    pub scroll: u16,
    pub threads: Vec<ThreadRow>,
    pub thread: usize,
    pub focus: Focus,
    pub mode: Mode,
    /// A message for the status line.
    pub notice: String,
}

/// Lines a page key moves.
const PAGE: u16 = 20;

impl App {
    pub fn new(files: Vec<String>, threads: Vec<ThreadRow>) -> Self {
        App {
            files,
            selected: 0,
            scroll: 0,
            threads,
            thread: 0,
            focus: Focus::Files,
            mode: Mode::Browse,
            notice: String::new(),
        }
    }

    /// The file shown.
    pub fn file(&self) -> Option<&str> {
        self.files.get(self.selected).map(String::as_str)
    }

    /// Handle one key.
    pub fn key(&mut self, key: KeyEvent) -> Action {
        match std::mem::replace(&mut self.mode, Mode::Browse) {
            Mode::Comment(mut text) => match key.code {
                KeyCode::Esc => {
                    self.notice = "comment discarded".into();
                    Action::None
                }
                KeyCode::Enter if !text.trim().is_empty() => match self.file() {
                    Some(path) => Action::Comment {
                        path: path.to_string(),
                        body: text,
                    },
                    None => Action::None,
                },
                KeyCode::Backspace => {
                    text.pop();
                    self.mode = Mode::Comment(text);
                    Action::None
                }
                KeyCode::Char(c) => {
                    text.push(c);
                    self.mode = Mode::Comment(text);
                    Action::None
                }
                _ => {
                    self.mode = Mode::Comment(text);
                    Action::None
                }
            },
            Mode::ConfirmReject => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => Action::Reject,
                _ => {
                    self.notice = "not rejected".into();
                    Action::None
                }
            },
            Mode::Browse => self.browse(key),
        }
    }

    fn browse(&mut self, key: KeyEvent) -> Action {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
            KeyCode::Char('c') if ctrl => Action::Quit,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Files if !self.threads.is_empty() => Focus::Threads,
                    _ => Focus::Files,
                };
                Action::None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.step(1);
                Action::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.step(-1);
                Action::None
            }
            KeyCode::Char(' ') | KeyCode::PageDown => {
                self.scroll = self.scroll.saturating_add(PAGE);
                Action::None
            }
            KeyCode::Char('b') | KeyCode::PageUp => {
                self.scroll = self.scroll.saturating_sub(PAGE);
                Action::None
            }
            KeyCode::Char('J') => {
                self.scroll = self.scroll.saturating_add(1);
                Action::None
            }
            KeyCode::Char('K') => {
                self.scroll = self.scroll.saturating_sub(1);
                Action::None
            }
            KeyCode::Char('c') if self.file().is_some() => {
                self.mode = Mode::Comment(String::new());
                Action::None
            }
            KeyCode::Char('a') => Action::Approve,
            KeyCode::Char('r') => {
                self.mode = Mode::ConfirmReject;
                Action::None
            }
            KeyCode::Enter if self.focus == Focus::Threads => match self.threads.get(self.thread) {
                Some(t) => Action::Resolve {
                    id: t.id.clone(),
                    resolved: !t.resolved,
                },
                None => Action::None,
            },
            _ => Action::None,
        }
    }

    fn step(&mut self, by: isize) {
        let (at, len) = match self.focus {
            Focus::Files => (&mut self.selected, self.files.len()),
            Focus::Threads => (&mut self.thread, self.threads.len()),
        };
        if len == 0 {
            return;
        }
        let next = (*at as isize + by).clamp(0, len as isize - 1) as usize;
        if next != *at && self.focus == Focus::Files {
            self.scroll = 0;
        }
        *at = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn app() -> App {
        App::new(
            vec!["a.rs".into(), "b.rs".into()],
            vec![ThreadRow {
                id: "t1".into(),
                resolved: false,
                text: "why?".into(),
            }],
        )
    }

    #[test]
    fn moving_between_files_resets_the_scroll_and_stops_at_the_ends() {
        let mut a = app();
        a.key(key(KeyCode::PageDown));
        assert_eq!(a.scroll, PAGE);
        a.key(key(KeyCode::Char('j')));
        assert_eq!((a.selected, a.scroll), (1, 0));
        a.key(key(KeyCode::Char('j')));
        assert_eq!(a.selected, 1);
    }

    #[test]
    fn a_typed_comment_goes_to_the_shown_file() {
        let mut a = app();
        a.key(key(KeyCode::Char('c')));
        for c in "nit".chars() {
            a.key(key(KeyCode::Char(c)));
        }
        a.key(key(KeyCode::Backspace));
        assert_eq!(
            a.key(key(KeyCode::Enter)),
            Action::Comment {
                path: "a.rs".into(),
                body: "ni".into()
            }
        );
    }

    #[test]
    fn reject_needs_a_yes_and_enter_toggles_a_thread() {
        let mut a = app();
        a.key(key(KeyCode::Char('r')));
        assert_eq!(a.key(key(KeyCode::Char('n'))), Action::None);
        a.key(key(KeyCode::Char('r')));
        assert_eq!(a.key(key(KeyCode::Char('y'))), Action::Reject);
        a.key(key(KeyCode::Tab));
        assert_eq!(
            a.key(key(KeyCode::Enter)),
            Action::Resolve {
                id: "t1".into(),
                resolved: true
            }
        );
        assert_eq!(a.key(key(KeyCode::Char('q'))), Action::Quit);
    }
}
