use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::core::ui::pad_line;
use crate::git::domain::repository::{BranchInfo, Repo};
use crate::git::state::PaneEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchAction {
    Switch,
    Delete,
    DiffBase,
}

impl std::str::FromStr for BranchAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Switch" => Ok(Self::Switch),
            "Delete" => Ok(Self::Delete),
            "DiffBase" => Ok(Self::DiffBase),
            _ => Err(format!("Unknown action: {s}")),
        }
    }
}

impl BranchAction {
    pub const ALL: [BranchAction; 3] = [
        BranchAction::Switch,
        BranchAction::Delete,
        BranchAction::DiffBase,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BranchAction::Switch => "Switch",
            BranchAction::Delete => "Delete",
            BranchAction::DiffBase => "Set as diff base",
        }
    }

    pub fn key(self) -> char {
        match self {
            BranchAction::Switch => 's',
            BranchAction::Delete => 'd',
            BranchAction::DiffBase => 'b',
        }
    }
}

/// Actions available in the branch action menu.
#[derive(Debug, Clone)]
pub enum MenuAction {
    Close,
    MoveDown,
    MoveUp,
    Confirm,
    Direct(BranchAction),
}

impl std::str::FromStr for MenuAction {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some(rest) = s.strip_prefix("Direct.") {
            return rest.parse::<BranchAction>().map(Self::Direct);
        }
        match s {
            "Close" => Ok(Self::Close),
            "MoveDown" => Ok(Self::MoveDown),
            "MoveUp" => Ok(Self::MoveUp),
            "Confirm" => Ok(Self::Confirm),
            _ => Err(format!("Unknown action: {s}")),
        }
    }
}

impl ActionHelp for MenuAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            MenuAction::Close => Some("Close menu"),
            MenuAction::MoveDown => Some("Next item"),
            MenuAction::MoveUp => Some("Prev item"),
            MenuAction::Confirm => Some("Confirm"),
            MenuAction::Direct(a) => Some(a.label()),
        }
    }
}

pub fn default_menu_keymap() -> Keymap<MenuAction> {
    Keymap::new()
        .key(KeyCode::Esc, MenuAction::Close)
        .key(KeyCode::Char('q'), MenuAction::Close)
        .key(KeyCode::Char('j'), MenuAction::MoveDown)
        .key(KeyCode::Down, MenuAction::MoveDown)
        .key(KeyCode::Char('k'), MenuAction::MoveUp)
        .key(KeyCode::Up, MenuAction::MoveUp)
        .key(KeyCode::Enter, MenuAction::Confirm)
        .key(KeyCode::Char('s'), MenuAction::Direct(BranchAction::Switch))
        .key(KeyCode::Char('d'), MenuAction::Direct(BranchAction::Delete))
        .key(
            KeyCode::Char('b'),
            MenuAction::Direct(BranchAction::DiffBase),
        )
}

pub struct BranchActionMenuState {
    pub branch_name: String,
    pub is_head: bool,
    pub selected_idx: usize,
}
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, ListItem, Paragraph},
    Frame,
};

use crate::git::state::{PANE_BRANCH_LIST, PANE_GIT_LOG};

#[derive(Debug, Clone)]
pub enum BranchListAction {
    Nav(NavAction),
    OpenActionMenu,
    FocusLog,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    BranchListAction, nav: Nav, search: Search, esc: Esc,
    OpenActionMenu, FocusLog
);

impl ActionHelp for BranchListAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            BranchListAction::Nav(nav) => nav.label(),
            BranchListAction::OpenActionMenu => Some("Action menu"),
            BranchListAction::FocusLog => Some("Focus log"),
            BranchListAction::Search(sa) => sa.label(),
            BranchListAction::Esc => Some("Clear search / Reset"),
        }
    }
}

pub fn default_keymap() -> Keymap<BranchListAction> {
    Keymap::new()
        .bindings(nav_bindings(BranchListAction::Nav))
        .bindings(search_bindings(BranchListAction::Search))
        .key(KeyCode::Enter, BranchListAction::OpenActionMenu)
        .key(KeyCode::Char('i'), BranchListAction::FocusLog)
        .key(KeyCode::Esc, BranchListAction::Esc)
}

pub struct BranchListPane {
    pub branches: Vec<BranchInfo>,
    pub selected_idx: usize,
    pub action_menu: Option<BranchActionMenuState>,
    keymap: Keymap<BranchListAction>,
    menu_keymap: Keymap<MenuAction>,
}

impl BranchListPane {
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            selected_idx: 0,
            action_menu: None,
            keymap: default_keymap(),
            menu_keymap: default_menu_keymap(),
        }
    }

    pub fn load(&mut self, repo: &Repo) {
        self.branches = repo.list_local_branches();
        if self.selected_idx >= self.branches.len() {
            self.selected_idx = 0;
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<BranchListAction>) {
        self.keymap = km;
    }

    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        self.branches.get(self.selected_idx)
    }

    fn execute(&mut self, shared: &PaneShared, action: BranchListAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(
            &action,
            shared,
            PANE_BRANCH_LIST,
            vec![PaneEvent::SetDiffBase(None)],
        ) {
            return events;
        }
        match action {
            BranchListAction::FocusLog => {
                return vec![PaneEvent::SetFocus(PANE_GIT_LOG)];
            }
            BranchListAction::Nav(nav) => {
                return pane::execute_list_nav(
                    nav,
                    &mut self.selected_idx,
                    self.branches.len(),
                    None,
                );
            }
            BranchListAction::OpenActionMenu => {
                if let Some(branch) = self.branches.get(self.selected_idx) {
                    self.action_menu = Some(BranchActionMenuState {
                        branch_name: branch.name.clone(),
                        is_head: branch.is_head,
                        selected_idx: 0,
                    });
                }
            }
            _ => {}
        }
        vec![]
    }

    fn handle_action_menu_key(&mut self, key: KeyEvent) -> Vec<PaneEvent> {
        let action = match self.menu_keymap.lookup(key) {
            Some(a) => a.clone(),
            None => return vec![],
        };
        let menu = match self.action_menu.as_mut() {
            Some(m) => m,
            None => return vec![],
        };
        match action {
            MenuAction::Close => {
                self.action_menu = None;
            }
            MenuAction::MoveDown => {
                if menu.selected_idx + 1 < BranchAction::ALL.len() {
                    menu.selected_idx += 1;
                }
            }
            MenuAction::MoveUp => {
                if menu.selected_idx > 0 {
                    menu.selected_idx -= 1;
                }
            }
            MenuAction::Confirm => {
                let action = BranchAction::ALL[menu.selected_idx];
                return self.execute_menu_action(action);
            }
            MenuAction::Direct(branch_action) => {
                return self.execute_menu_action(branch_action);
            }
        }
        vec![]
    }

    fn execute_menu_action(&mut self, action: BranchAction) -> Vec<PaneEvent> {
        let menu = match self.action_menu.take() {
            Some(m) => m,
            None => return vec![],
        };
        match action {
            BranchAction::Switch => {
                if menu.is_head {
                    return vec![PaneEvent::StatusMessage(
                        "Already on this branch".to_string(),
                    )];
                }
                vec![PaneEvent::SwitchBranch(menu.branch_name)]
            }
            BranchAction::Delete => {
                if menu.is_head {
                    return vec![PaneEvent::StatusMessage(
                        "Cannot delete the current branch".to_string(),
                    )];
                }
                vec![PaneEvent::DeleteBranch(menu.branch_name)]
            }
            BranchAction::DiffBase => {
                let base = if menu.is_head {
                    None
                } else {
                    Some(menu.branch_name)
                };
                vec![PaneEvent::SetDiffBase(base)]
            }
        }
    }

    fn render_impl(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        let empty = self.branches.is_empty().then_some("No branches");
        theme::render_list_pane(
            f,
            area,
            shared,
            PANE_BRANCH_LIST,
            "Branches",
            Some(self.selected_idx),
            empty,
            |match_set, current_match_idx| {
                self.branches
                    .iter()
                    .enumerate()
                    .map(|(idx, branch)| {
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);

                        let mut spans = vec![Span::raw(" ")];

                        let name_style = if hl.is_active() {
                            hl.apply(Style::default())
                        } else if branch.is_head {
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default()
                        };

                        if branch.is_head {
                            let star_style = if hl.is_active() {
                                hl.apply(
                                    Style::default()
                                        .fg(Color::Green)
                                        .add_modifier(Modifier::BOLD),
                                )
                            } else {
                                Style::default()
                                    .fg(Color::Green)
                                    .add_modifier(Modifier::BOLD)
                            };
                            spans.push(Span::styled("* ", star_style));
                            spans.push(Span::styled(branch.name.clone(), name_style));
                        } else {
                            spans.push(Span::raw("  "));
                            spans.push(Span::styled(branch.name.clone(), name_style));
                        }

                        ListItem::new(Line::from(spans))
                    })
                    .collect()
            },
        );
    }

    pub fn render_action_menu(&self, f: &mut Frame, area: Rect) {
        let menu = match &self.action_menu {
            Some(m) => m,
            None => return,
        };

        let menu_width = 25u16.min(area.width.saturating_sub(4));
        let menu_height = (BranchAction::ALL.len() as u16 + 4).min(area.height.saturating_sub(4));
        let x = (area.width.saturating_sub(menu_width)) / 2;
        let y = (area.height.saturating_sub(menu_height)) / 2;
        let menu_area = Rect::new(x, y, menu_width, menu_height);

        f.render_widget(Clear, menu_area);

        let inner_w = menu_width.saturating_sub(2) as usize;
        let mut lines: Vec<Line> = Vec::new();

        // Branch name header
        let name_style = if menu.is_head {
            Style::default()
                .fg(Color::Green)
                .bg(theme::MODAL_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(theme::MODAL_BG)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(pad_line(
            Line::from(Span::styled(format!(" {}", menu.branch_name), name_style)),
            inner_w,
            theme::MODAL_BG,
        ));
        lines.push(pad_line(
            Line::from(Span::styled(
                " ─────────────────────",
                Style::default().fg(Color::DarkGray).bg(theme::MODAL_BG),
            )),
            inner_w,
            theme::MODAL_BG,
        ));

        // Menu items
        for (idx, action) in BranchAction::ALL.iter().enumerate() {
            let is_selected = idx == menu.selected_idx;
            let key_char = action.key();
            let label = action.label();
            let item_bg = if is_selected {
                Color::DarkGray
            } else {
                theme::MODAL_BG
            };
            let style = Style::default().bg(item_bg).add_modifier(if is_selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            });
            let key_style = Style::default()
                .fg(Color::Cyan)
                .bg(item_bg)
                .add_modifier(Modifier::BOLD);
            lines.push(pad_line(
                Line::from(vec![
                    Span::styled(format!(" {key_char}  "), key_style),
                    Span::styled(label.to_string(), style),
                ]),
                inner_w,
                theme::MODAL_BG,
            ));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan).bg(theme::MODAL_BG))
            .style(Style::default().bg(theme::MODAL_BG));

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, menu_area);
    }
}

impl Pane<PaneEvent> for BranchListPane {
    crate::impl_handle_key!(keymap, modal: action_menu => handle_action_menu_key);

    fn render(&mut self, f: &mut Frame, ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.render_impl(f, ctx, shared, area)
    }

    fn is_modal(&self) -> bool {
        self.action_menu.is_some()
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.branches, query, |branch| branch.name.clone())
    }

    crate::impl_list_pane_selection!();
}
