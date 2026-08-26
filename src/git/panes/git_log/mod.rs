pub(crate) mod view;

use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared, SubPaneScroll};
use crate::git::domain::graph::{self, GraphRow};
use crate::git::domain::repository::{CommitFileChange, CommitInfo, Repo};
use crate::git::state::PaneEvent;
use crossterm::event::KeyCode;
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

crate::impl_pane_action_from_str!(
    GitLogAction, nav: Nav, search: Search, esc: Esc,
    YankHash, OpenGitHub, FocusReflog
);

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
    pub pane_id: usize,
    reflog_id: usize,
    pub branch_list_id: usize,
}

impl GitLogPane {
    pub fn new(pane_id: usize, reflog_id: usize, branch_list_id: usize) -> Self {
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
            pane_id,
            reflog_id,
            branch_list_id,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<GitLogAction>) {
        self.keymap = km;
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
        if let Some(events) = pane::try_dispatch_search_esc(
            &action,
            shared,
            self.pane_id,
            vec![PaneEvent::SetFocus(shared.previous_pane)],
        ) {
            return events;
        }
        match action {
            GitLogAction::FocusReflog => {
                return vec![PaneEvent::SetFocus(self.reflog_id)];
            }
            GitLogAction::Nav(nav) => {
                return pane::execute_list_nav(
                    nav,
                    &mut self.selected_idx,
                    self.commits.len(),
                    Some(self.view_height),
                );
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
            _ => {}
        }
        vec![]
    }
}

impl Pane<PaneEvent> for GitLogPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        view::render(f, self, shared, area);
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.commits, query, |commit| {
            format!(
                "{} {} {} {}",
                commit.short_hash, commit.author, commit.date, commit.message
            )
        })
    }

    crate::impl_list_pane_selection!();
}
