mod view;

use crate::core::app::{App, SearchOrigin};
use crate::git::state::FocusedPane;
use crate::core::pane::SelectPane;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

pub struct GitLogSelectPane;

impl SelectPane for GitLogSelectPane {
    fn handle_key(&self, app: &mut App, key: KeyEvent) {
        match key.code {
            KeyCode::Char('h') => {
                app.git.set_focus(FocusedPane::Reflog);
            }
            KeyCode::Esc => {
                if app.git.search.query.is_some() {
                    app.git.search.clear();
                } else {
                    app.git.set_focus(app.git.previous_pane);
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !app.git.git_log.commits.is_empty()
                    && app.git.git_log.selected_idx + 1 < app.git.git_log.commits.len()
                {
                    app.git.git_log.selected_idx += 1;
                    app.git.load_commit_detail();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.git.git_log.selected_idx > 0 {
                    app.git.git_log.selected_idx -= 1;
                    app.git.load_commit_detail();
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.git.git_log.view_height / 2).max(1) as usize;
                let new_idx = app.git.git_log.selected_idx.saturating_add(half);
                app.git.git_log.selected_idx =
                    new_idx.min(app.git.git_log.commits.len().saturating_sub(1));
                app.git.load_commit_detail();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (app.git.git_log.view_height / 2).max(1) as usize;
                app.git.git_log.selected_idx = app.git.git_log.selected_idx.saturating_sub(half);
                app.git.load_commit_detail();
            }
            KeyCode::Char('g') => {
                app.git.git_log.selected_idx = 0;
                app.git.load_commit_detail();
            }
            KeyCode::Char('G') => {
                if !app.git.git_log.commits.is_empty() {
                    app.git.git_log.selected_idx = app.git.git_log.commits.len() - 1;
                    app.git.load_commit_detail();
                }
            }
            KeyCode::Char('y') => {
                if let Some(commit) = app.git.git_log.commits.get(app.git.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    app.copy_to_clipboard(&hash);
                }
            }
            KeyCode::Char('o') => {
                if let Some(commit) = app.git.git_log.commits.get(app.git.git_log.selected_idx) {
                    let hash = commit.full_hash.clone();
                    if let Some(nwo) = crate::github::client::repo_nwo() {
                        let url = format!("https://github.com/{nwo}/commit/{hash}");
                        match crate::github::client::open_url(&url) {
                            Ok(()) => {
                                app.ctx.status_message =
                                    Some("Opening in browser...".to_string());
                            }
                            Err(e) => {
                                app.ctx.status_message =
                                    Some(format!("Failed to open URL: {e}"));
                            }
                        }
                    } else {
                        app.ctx.status_message =
                            Some("Could not determine GitHub repository".to_string());
                    }
                }
            }
            KeyCode::Char('/') => {
                app.git.search.start(SearchOrigin::CommitLog);
            }
            KeyCode::Char('n') => {
                app.jump_to_git_match(true);
            }
            KeyCode::Char('N') => {
                app.jump_to_git_match(false);
            }
            _ => {}
        }
    }

    fn render(&self, f: &mut Frame, app: &mut App, area: Rect) {
        view::render(f, app, area);
    }
}
