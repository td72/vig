use crate::app::App;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) trait PaneContainer {
    // Required: container-specific
    fn current_index(&self, app: &App) -> Option<usize>;
    fn focus_index(&self, app: &mut App, idx: usize);
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane;
    fn len(&self) -> usize;

    // Optional hooks
    fn is_prev_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('h'))
    }
    fn is_next_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('l'))
    }
    fn handle_common_key(&self, _app: &mut App, _key: KeyEvent, _idx: usize) -> bool {
        false
    }

    // Provided: h/l switching + delegation
    fn dispatch(&self, app: &mut App, key: KeyEvent) {
        let Some(idx) = self.current_index(app) else {
            return;
        };
        if self.is_prev_key(&key) && idx > 0 {
            self.focus_index(app, idx - 1);
        } else if self.is_next_key(&key) && idx + 1 < self.len() {
            self.focus_index(app, idx + 1);
        } else if !self.handle_common_key(app, key, idx) {
            self.pane_at(idx).handle_key(app, key);
        }
    }
}
