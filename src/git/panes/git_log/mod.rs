mod view;

use crate::core::app::{AppContext, SearchOrigin};
use crate::core::pane::{FocusState, SelectPane};
use crate::git::domain::search;
use crate::git::panes::diff_view::keys::copy_to_clipboard;
use crate::git::state::{FocusedPane, GitState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

pub struct GitLogSelectPane;

impl SelectPane<GitState> for GitLogSelectPane {
    fn handle_key(&self, ctx: &mut AppContext, state: &mut GitState, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') => {
                state.set_focus(FocusedPane::Reflog);
            }
            KeyCode::Esc => {
                if state.shared.search.query.is_some() {
                    state.shared.search.clear();
                } else {
                    state.set_focus(state.shared.previous_pane);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !state.git_log.commits.is_empty()
                    && state.git_log.selected_idx + 1 < state.git_log.commits.len()
                {
                    state.git_log.selected_idx += 1;
                    state.load_commit_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if state.git_log.selected_idx > 0 {
                    state.git_log.selected_idx -= 1;
                    state.load_commit_detail();
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.git_log.view_height / 2).max(1) as usize;
                let new_idx = state.git_log.selected_idx.saturating_add(half);
                state.git_log.selected_idx =
                    new_idx.min(state.git_log.commits.len().saturating_sub(1));
                state.load_commit_detail();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (state.git_log.view_height / 2).max(1) as usize;
                state.git_log.selected_idx = state.git_log.selected_idx.saturating_sub(half);
                state.load_commit_detail();
            }
            KeyCode::Char('g') => {
                state.git_log.selected_idx = 0;
                state.load_commit_detail();
            }
            KeyCode::Char('G') => {
                if !state.git_log.commits.is_empty() {
                    state.git_log.selected_idx = state.git_log.commits.len() - 1;
                    state.load_commit_detail();
                }
            }
            KeyCode::Char('y') => {
                if let Some(commit) = state.git_log.commits.get(state.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    copy_to_clipboard(ctx, &hash);
                }
            }
            KeyCode::Char('o') => {
                if let Some(commit) = state.git_log.commits.get(state.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    if let Some(nwo) = crate::github::domain::client::repo_nwo() {
                        let url = format!("https://github.com/{nwo}/commit/{hash}");
                        match crate::github::domain::client::open_url(&url) {
                            Ok(()) => {
                                ctx.status_message = Some("Opening in browser...".to_string());
                            }
                            Err(e) => {
                                ctx.status_message = Some(format!("Failed to open URL: {e}"));
                            }
                        }
                    } else {
                        ctx.status_message =
                            Some("Could not determine GitHub repository".to_string());
                    }
                }
            }
            KeyCode::Char('/') => {
                state.shared.search.start(SearchOrigin::CommitLog);
            }
            KeyCode::Char('n') => {
                search::jump_to_git_match(ctx, state, true);
            }
            KeyCode::Char('N') => {
                search::jump_to_git_match(ctx, state, false);
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, _ctx: &AppContext, state: &mut GitState, area: Rect) {
        view::render(f, state, area);
    }
}
