use crate::core::app::AppContext;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) trait PaneContainer<S: 'static> {
    // Required: container-specific
    fn current_index(&self, state: &S) -> Option<usize>;
    fn focus_index(&self, ctx: &mut AppContext, state: &mut S, idx: usize);
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane<S>;
    fn len(&self) -> usize;

    // Optional hooks
    fn is_prev_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('h'))
    }
    fn is_next_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('l'))
    }
    fn handle_common_key(&self, _ctx: &mut AppContext, _state: &mut S, _key: KeyEvent, _idx: usize) -> bool {
        false
    }

    // Provided: h/l switching + delegation
    fn dispatch(&self, ctx: &mut AppContext, state: &mut S, key: KeyEvent) {
        let Some(idx) = self.current_index(state) else {
            return;
        };
        if self.is_prev_key(&key) && idx > 0 {
            self.focus_index(ctx, state, idx - 1);
        } else if self.is_next_key(&key) && idx + 1 < self.len() {
            self.focus_index(ctx, state, idx + 1);
        } else if !self.handle_common_key(ctx, state, key, idx) {
            self.pane_at(idx).handle_key(ctx, state, key);
        }
    }
}
