use crate::app::{App, FocusedPane};
use crate::container::PaneContainer;
use crate::pane::{GitDetailId, SelectPane, GIT_GROUPS};
use crossterm::event::{KeyCode, KeyEvent};

pub(crate) struct GitContainer;

impl PaneContainer for GitContainer {
    fn current_index(&self, app: &App) -> Option<usize> {
        GIT_GROUPS.iter().position(|g| g.id == app.focused_pane)
    }
    fn focus_index(&self, app: &mut App, idx: usize) {
        app.set_focus(GIT_GROUPS[idx].id);
    }
    fn pane_at(&self, idx: usize) -> &'static dyn SelectPane {
        GIT_GROUPS[idx].select
    }
    fn len(&self) -> usize {
        GIT_GROUPS.len()
    }

    fn handle_common_key(&self, app: &mut App, key: KeyEvent, idx: usize) -> bool {
        match key.code {
            KeyCode::Char('i') => {
                let target = match GIT_GROUPS[idx].detail {
                    GitDetailId::DiffView => FocusedPane::DiffView,
                    GitDetailId::CommitLog => FocusedPane::GitLog,
                };
                app.set_focus(target);
                true
            }
            KeyCode::Esc if app.search.query.is_some() => {
                app.search.clear();
                true
            }
            KeyCode::Char('/') => {
                app.search.start(GIT_GROUPS[idx].search_origin);
                true
            }
            KeyCode::Char('n') => {
                app.jump_to_match(true);
                true
            }
            KeyCode::Char('N') => {
                app.jump_to_match(false);
                true
            }
            _ => false,
        }
    }
}
