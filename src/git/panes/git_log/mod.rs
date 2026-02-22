pub(crate) mod view;

use crate::core::pane::{DetailState, Pane, PaneShared, SubPaneScroll};
use crate::git::domain::graph::{self, GraphRow};
use crate::git::domain::repository::{CommitFileChange, CommitInfo, Repo};
use crate::git::state::{PaneEvent, PANE_REFLOG};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, Frame};

use crate::core::app::AppContext;
use crate::core::search::SearchMatch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum GitLogDetailPane {
    Detail,
}

pub struct GitLogPane {
    pub commits: Vec<CommitInfo>,
    pub selected_idx: usize,
    pub view_height: u16,
    pub ref_name: String,
    pub graph: Vec<GraphRow>,
    pub detail: SubPaneScroll,
    pub detail_view_height: u16,
    pub detail_changed_files: Vec<CommitFileChange>,
}

impl GitLogPane {
    pub fn new() -> Self {
        Self {
            commits: Vec::new(),
            selected_idx: 0,
            view_height: 0,
            ref_name: String::new(),
            graph: Vec::new(),
            detail: SubPaneScroll::default(),
            detail_view_height: 0,
            detail_changed_files: Vec::new(),
        }
    }

    pub fn load_for_ref(&mut self, repo: &Repo, ref_name: &str) {
        self.ref_name = ref_name.to_string();
        self.commits = repo.log_for_ref(ref_name, 100);
        self.graph = graph::build_graph(&self.commits);
        self.selected_idx = 0;
        self.detail.reset();
        self.detail_changed_files.clear();
        self.load_detail(repo);
    }

    pub fn load_detail(&mut self, repo: &Repo) {
        if let Some(commit) = self.commits.get(self.selected_idx) {
            self.detail_changed_files = repo.commit_changed_files(&commit.full_hash);
            self.detail.reset();
        } else {
            self.detail_changed_files.clear();
        }
    }

    pub fn clear_log(&mut self) {
        self.commits.clear();
        self.graph.clear();
        self.ref_name.clear();
        self.detail_changed_files.clear();
    }

    pub fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        match key.code {
            KeyCode::Char('h') => {
                return vec![PaneEvent::SetFocus(PANE_REFLOG)];
            }
            KeyCode::Esc => {
                if shared.search.query.is_some() {
                    return vec![PaneEvent::ClearSearch];
                } else {
                    return vec![PaneEvent::SetFocus(shared.previous_pane)];
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.commits.is_empty() && self.selected_idx + 1 < self.commits.len() {
                    self.selected_idx += 1;
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1) as usize;
                let new_idx = self.selected_idx.saturating_add(half);
                self.selected_idx = new_idx.min(self.commits.len().saturating_sub(1));
                return vec![PaneEvent::SelectionChanged];
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let half = (self.view_height / 2).max(1) as usize;
                self.selected_idx = self.selected_idx.saturating_sub(half);
                return vec![PaneEvent::SelectionChanged];
            }
            KeyCode::Char('g') => {
                self.selected_idx = 0;
                return vec![PaneEvent::SelectionChanged];
            }
            KeyCode::Char('G') => {
                if !self.commits.is_empty() {
                    self.selected_idx = self.commits.len() - 1;
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            KeyCode::Char('y') => {
                if let Some(commit) = self.commits.get(self.selected_idx) {
                    return vec![PaneEvent::CopyToClipboard(commit.full_hash.clone())];
                }
            }
            KeyCode::Char('o') => {
                if let Some(commit) = self.commits.get(self.selected_idx) {
                    let hash = commit.full_hash.clone();
                    if let Some(nwo) = crate::github::domain::client::repo_nwo() {
                        let url = format!("https://github.com/{nwo}/commit/{hash}");
                        return vec![PaneEvent::OpenUrl(url)];
                    } else {
                        return vec![PaneEvent::StatusMessage(
                            "Could not determine GitHub repository".to_string(),
                        )];
                    }
                }
            }
            KeyCode::Char('/') => {
                return vec![PaneEvent::StartSearch(crate::git::state::PANE_GIT_LOG)];
            }
            KeyCode::Char('n') => {
                return vec![PaneEvent::JumpToMatch(true)];
            }
            KeyCode::Char('N') => {
                return vec![PaneEvent::JumpToMatch(false)];
            }
            _ => {}
        }
        vec![]
    }

    pub fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        self.commits
            .iter()
            .enumerate()
            .filter_map(|(idx, commit)| {
                let text = format!(
                    "{} {} {} {}",
                    commit.short_hash, commit.author, commit.date, commit.message
                );
                if text.to_lowercase().contains(&query_lower) {
                    Some(SearchMatch::ListEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        view::render(f, self, shared, area);
    }
}

impl Pane<PaneEvent> for GitLogPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        self.handle_key(shared, key)
    }

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render(f, ctx, shared, area)
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.selected_idx = idx;
    }

    fn collect_search_matches(&self, shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        self.collect_search_matches(shared, query)
    }

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.selected_idx = *idx;
        }
    }
}

impl crate::core::pane::FocusState<GitLogDetailPane> for GitLogPane {
    fn focused_pane(&self) -> GitLogDetailPane {
        GitLogDetailPane::Detail
    }
    fn set_focus(&mut self, _id: GitLogDetailPane) {}
}

impl DetailState for GitLogPane {
    type SubPaneId = GitLogDetailPane;
    fn sub_scroll(&self, _id: GitLogDetailPane) -> &SubPaneScroll {
        &self.detail
    }
    fn sub_scroll_mut(&mut self, _id: GitLogDetailPane) -> &mut SubPaneScroll {
        &mut self.detail
    }
    fn detail_view_height(&self) -> u16 {
        self.detail_view_height
    }
    fn set_detail_view_height(&mut self, h: u16) {
        self.detail_view_height = h;
    }
    fn reset_sub_panes(&mut self) {
        self.detail.reset();
    }
}
