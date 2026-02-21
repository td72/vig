use crate::core::app::AppContext;
use crate::core::pane::{FocusState, SelectPane};
use crossterm::event::{KeyCode, KeyEvent};

pub struct PaneTab<S: FocusState + 'static> {
    pub select: &'static dyn SelectPane<S>,
    pub id: S::PaneId,
}

pub(crate) trait PaneRouter<S: FocusState + 'static> {
    // --- Required (1) ---
    fn tabs(&self) -> &'static [PaneTab<S>];

    // --- Optional hooks ---
    fn on_focus_change(&self, _ctx: &mut AppContext, _state: &mut S) {}

    fn is_prev_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('h'))
    }
    fn is_next_key(&self, key: &KeyEvent) -> bool {
        matches!(key.code, KeyCode::Char('l'))
    }
    fn handle_common_key(
        &self,
        _ctx: &mut AppContext,
        _state: &mut S,
        _key: KeyEvent,
        _idx: usize,
    ) -> bool {
        false
    }

    // --- Provided (derived from FocusState + tabs) ---
    fn current_index(&self, state: &S) -> Option<usize> {
        let id = state.focused_pane();
        self.tabs().iter().position(|t| t.id == id)
    }
    fn focus_index(&self, ctx: &mut AppContext, state: &mut S, idx: usize) {
        state.set_focus(self.tabs()[idx].id);
        self.on_focus_change(ctx, state);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane<S> {
        self.tabs()[idx].select
    }
    fn len(&self) -> usize {
        self.tabs().len()
    }

    fn next_tab_id(&self, state: &S) -> S::PaneId {
        let tabs = self.tabs();
        let idx = self.current_index(state).unwrap_or(0);
        tabs[(idx + 1) % tabs.len()].id
    }
    fn prev_tab_id(&self, state: &S) -> S::PaneId {
        let tabs = self.tabs();
        let idx = self.current_index(state).unwrap_or(0);
        tabs[(idx + tabs.len() - 1) % tabs.len()].id
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
