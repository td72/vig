use crate::core::app::AppContext;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent};

pub struct PaneTab<S: 'static, P: Copy + PartialEq> {
    pub select: &'static dyn SelectPane<S>,
    pub id: P,
}

pub(crate) trait PaneRouter<S: 'static> {
    type PaneId: Copy + PartialEq + 'static;

    // --- Required (3) ---
    fn tabs(&self) -> &'static [PaneTab<S, Self::PaneId>];
    fn focused_id(&self, state: &S) -> Self::PaneId;
    fn set_focused(&self, ctx: &mut AppContext, state: &mut S, id: Self::PaneId);

    // --- Provided (derived from tabs) ---
    fn current_index(&self, state: &S) -> Option<usize> {
        let id = self.focused_id(state);
        self.tabs().iter().position(|t| t.id == id)
    }
    fn focus_index(&self, ctx: &mut AppContext, state: &mut S, idx: usize) {
        self.set_focused(ctx, state, self.tabs()[idx].id);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane<S> {
        self.tabs()[idx].select
    }
    fn len(&self) -> usize {
        self.tabs().len()
    }

    // --- Provided (wrapping tab cycling) ---
    fn next_tab_id(&self, state: &S) -> Self::PaneId {
        let tabs = self.tabs();
        let idx = self.current_index(state).unwrap_or(0);
        tabs[(idx + 1) % tabs.len()].id
    }
    fn prev_tab_id(&self, state: &S) -> Self::PaneId {
        let tabs = self.tabs();
        let idx = self.current_index(state).unwrap_or(0);
        tabs[(idx + tabs.len() - 1) % tabs.len()].id
    }

    // --- Optional hooks ---
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
