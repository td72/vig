//! `OpenDefault` / `OpenWith`: open a file with the OS default application
//! or with one the user names in a one-line prompt. Shared by the Files
//! view, the Git view's file tree and the diff window.

use crate::core::browser;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::Path;

/// One-line input for the `OpenWith` action (`O`): the application name to
/// open the selected entry with.
#[derive(Debug, Default)]
pub struct OpenWithPrompt {
    pub active: bool,
    pub input: String,
    /// Last confirmed application name; pre-filled the next time.
    last: String,
}

impl OpenWithPrompt {
    pub fn start(&mut self) {
        self.active = true;
        self.input = self.last.clone();
    }

    /// Handle a key while the prompt is active. Returns `Some(app)` when the
    /// user confirmed a non-empty application name.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        match key.code {
            KeyCode::Enter => {
                self.active = false;
                let app = self.input.trim().to_string();
                if app.is_empty() {
                    return None;
                }
                self.last = app.clone();
                Some(app)
            }
            KeyCode::Esc => {
                self.active = false;
                None
            }
            KeyCode::Backspace => {
                self.input.pop();
                None
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
                None
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                None
            }
            _ => None,
        }
    }

    /// The remembered application name (tests).
    #[cfg(test)]
    pub fn last(&self) -> &str {
        &self.last
    }
}

/// Open `path` with `app` (or the default application) and say what
/// happened, for the status bar.
pub fn open_path_message(path: &Path, app: Option<&str>) -> String {
    let result = match app {
        Some(app) => browser::open_path_with(app, path),
        None => browser::open_path(path),
    };
    match result {
        Ok(()) => match app {
            Some(app) => format!("Opening with {app}..."),
            None => "Opening...".to_string(),
        },
        Err(e) => e,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn type_str(p: &mut OpenWithPrompt, s: &str) {
        for c in s.chars() {
            assert_eq!(p.handle_key(key(KeyCode::Char(c))), None);
        }
    }

    #[test]
    fn open_with_prompt_confirms_trimmed_name_and_remembers_it() {
        let mut p = OpenWithPrompt::default();
        p.start();
        assert!(p.active);
        assert_eq!(p.input, "");
        type_str(&mut p, " Preview ");
        assert_eq!(
            p.handle_key(key(KeyCode::Enter)),
            Some("Preview".to_string())
        );
        assert!(!p.active);

        // The last name is pre-filled next time and can be cleared with Ctrl+u.
        p.start();
        assert_eq!(p.input, "Preview");
        assert_eq!(p.handle_key(ctrl('u')), None);
        assert_eq!(p.input, "");
        type_str(&mut p, "Xcode");
        assert_eq!(p.handle_key(key(KeyCode::Backspace)), None);
        assert_eq!(p.input, "Xcod");
    }

    #[test]
    fn open_with_prompt_esc_and_empty_enter_cancel() {
        let mut p = OpenWithPrompt::default();
        p.start();
        type_str(&mut p, "abc");
        assert_eq!(p.handle_key(key(KeyCode::Esc)), None);
        assert!(!p.active);

        p.start();
        assert_eq!(p.handle_key(key(KeyCode::Enter)), None);
        assert!(!p.active);
        assert_eq!(p.last(), "");
    }
}
