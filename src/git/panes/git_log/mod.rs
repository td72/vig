pub(crate) mod view;

use crate::core::keymap::{
    execute_nav, nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared, SubPaneScroll};
use crate::git::domain::graph::{self, GraphRow};
use crate::git::domain::repository::{CommitFileChange, CommitInfo, Repo};
use crate::git::state::{PaneEvent, PANE_REFLOG};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{layout::Rect, Frame};

use crate::core::app::AppContext;
use crate::core::search::SearchMatch;

#[derive(Debug, Clone)]
pub enum GitLogAction {
    Nav(NavAction),
    YankHash,
    OpenGitHub,
    FocusReflog,
    Search(SearchAction),
    Esc,
}

impl ActionHelp for GitLogAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            GitLogAction::Nav(nav) => nav.label(),
            GitLogAction::YankHash => Some("Copy commit hash"),
            GitLogAction::OpenGitHub => Some("Open in GitHub"),
            GitLogAction::FocusReflog => Some("Focus reflog"),
            GitLogAction::Search(sa) => sa.label(),
            GitLogAction::Esc => Some("Clear search / Back"),
        }
    }
}

pub fn default_keymap() -> Keymap<GitLogAction> {
    Keymap::new()
        .bindings(nav_bindings(GitLogAction::Nav))
        .bindings(search_bindings(GitLogAction::Search))
        .key(KeyCode::Char('y'), GitLogAction::YankHash)
        .key(KeyCode::Char('o'), GitLogAction::OpenGitHub)
        .key(KeyCode::Char('h'), GitLogAction::FocusReflog)
        .key(KeyCode::Esc, GitLogAction::Esc)
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
    keymap: Keymap<GitLogAction>,
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
            keymap: default_keymap(),
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

    fn execute(&mut self, shared: &PaneShared, action: GitLogAction) -> Vec<PaneEvent> {
        match action {
            GitLogAction::FocusReflog => {
                return vec![PaneEvent::SetFocus(PANE_REFLOG)];
            }
            GitLogAction::Esc => {
                return pane::execute_esc(shared, vec![PaneEvent::SetFocus(shared.previous_pane)]);
            }
            GitLogAction::Nav(nav) => {
                if execute_nav(
                    nav,
                    &mut self.selected_idx,
                    self.commits.len(),
                    Some(self.view_height),
                ) {
                    return vec![PaneEvent::SelectionChanged];
                }
            }
            GitLogAction::YankHash => {
                if let Some(commit) = self.commits.get(self.selected_idx) {
                    return vec![PaneEvent::CopyToClipboard(commit.full_hash.clone())];
                }
            }
            GitLogAction::OpenGitHub => {
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
            GitLogAction::Search(sa) => {
                return pane::execute_search(sa, crate::git::state::PANE_GIT_LOG);
            }
        }
        vec![]
    }
}

impl Pane<PaneEvent> for GitLogPane {
    fn handle_key(&mut self, shared: &PaneShared, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        self.execute(shared, action)
    }

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        view::render(f, self, shared, area);
    }

    fn set_selected_idx(&mut self, idx: usize) {
        self.selected_idx = idx;
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
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

    fn jump_to_match(&mut self, _shared: &PaneShared, search_match: &SearchMatch) {
        if let SearchMatch::ListEntry(idx) = search_match {
            self.selected_idx = *idx;
        }
    }
}
