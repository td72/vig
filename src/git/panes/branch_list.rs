use crate::core::app::{AppContext, SearchMatch, SearchOrigin};
use crate::git::domain::repository::{BranchInfo, Repo};
use crate::git::state::{BranchAction, BranchActionMenuState, GitShared, PaneEvent};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};
use std::collections::HashSet;

use crate::git::state::FocusedPane;

const ACTION_MENU_BG: Color = Color::Rgb(30, 30, 30);

pub struct BranchListPane {
    pub branches: Vec<BranchInfo>,
    pub selected_idx: usize,
    pub action_menu: Option<BranchActionMenuState>,
}

impl BranchListPane {
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            selected_idx: 0,
            action_menu: None,
        }
    }

    pub fn load(&mut self, repo: &Repo) {
        self.branches = repo.list_local_branches();
        if self.selected_idx >= self.branches.len() {
            self.selected_idx = 0;
        }
    }

    pub fn selected_branch(&self) -> Option<&BranchInfo> {
        self.branches.get(self.selected_idx)
    }

    pub fn handle_key(&mut self, shared: &GitShared, key: KeyEvent) -> Vec<PaneEvent> {
        if self.action_menu.is_some() {
            return self.handle_action_menu_key(key);
        }
        match key.code {
            KeyCode::Esc => {
                if shared.diff_base_ref.is_some() {
                    return vec![PaneEvent::SetDiffBase(None), PaneEvent::RefreshDiff];
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if !self.branches.is_empty() && self.selected_idx + 1 < self.branches.len() {
                    self.selected_idx += 1;
                    return vec![PaneEvent::UpdateBranchLog];
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.selected_idx > 0 {
                    self.selected_idx -= 1;
                    return vec![PaneEvent::UpdateBranchLog];
                }
            }
            KeyCode::Enter => {
                return vec![PaneEvent::OpenBranchActionMenu];
            }
            _ => {}
        }
        vec![]
    }

    fn handle_action_menu_key(&mut self, key: KeyEvent) -> Vec<PaneEvent> {
        let menu = match self.action_menu.as_mut() {
            Some(m) => m,
            None => return vec![],
        };

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.action_menu = None;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if menu.selected_idx + 1 < BranchAction::ALL.len() {
                    menu.selected_idx += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if menu.selected_idx > 0 {
                    menu.selected_idx -= 1;
                }
            }
            KeyCode::Enter => {
                let action = BranchAction::ALL[menu.selected_idx];
                return self.execute_menu_action(action);
            }
            KeyCode::Char('s') => {
                return self.execute_menu_action(BranchAction::Switch);
            }
            KeyCode::Char('d') => {
                return self.execute_menu_action(BranchAction::Delete);
            }
            KeyCode::Char('b') => {
                return self.execute_menu_action(BranchAction::DiffBase);
            }
            _ => {}
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
                vec![PaneEvent::SetDiffBase(base), PaneEvent::RefreshDiff]
            }
        }
    }

    pub fn collect_search_matches(&self, query: &str) -> Vec<SearchMatch> {
        let query_lower = query.to_lowercase();
        self.branches
            .iter()
            .enumerate()
            .filter_map(|(idx, branch)| {
                if branch.name.to_lowercase().contains(&query_lower) {
                    Some(SearchMatch::BranchEntry(idx))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn render(&self, f: &mut Frame, _ctx: &AppContext, shared: &GitShared, area: Rect) {
        let border_color = if shared.focused_pane == FocusedPane::BranchList {
            Color::Cyan
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .title(" Branches ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        if self.branches.is_empty() {
            let items: Vec<ListItem> = vec![ListItem::new(Line::from(Span::styled(
                "  No branches",
                Style::default().fg(Color::DarkGray),
            )))];
            let list = List::new(items).block(block);
            f.render_widget(list, area);
            return;
        }

        // Build set of matched branch entry indices
        let (match_set, current_match_idx) = if shared.search.origin == SearchOrigin::BranchList {
            let set: HashSet<usize> = shared
                .search
                .matches
                .iter()
                .filter_map(|m| match m {
                    SearchMatch::BranchEntry(idx) => Some(*idx),
                    _ => None,
                })
                .collect();
            let current = shared.search.current_match_idx.and_then(|ci| {
                match shared.search.matches.get(ci) {
                    Some(SearchMatch::BranchEntry(idx)) => Some(*idx),
                    _ => None,
                }
            });
            (set, current)
        } else {
            (HashSet::new(), None)
        };

        let items: Vec<ListItem> = self
            .branches
            .iter()
            .enumerate()
            .map(|(idx, branch)| {
                let is_current = current_match_idx == Some(idx);
                let is_match = match_set.contains(&idx);

                let mut spans = vec![Span::raw(" ")];

                let name_style = if is_current {
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Rgb(200, 120, 0))
                } else if is_match {
                    Style::default().bg(Color::Rgb(60, 60, 0))
                } else if branch.is_head {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                if branch.is_head {
                    let star_style = if is_current {
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Rgb(200, 120, 0))
                            .add_modifier(Modifier::BOLD)
                    } else if is_match {
                        Style::default()
                            .fg(Color::Green)
                            .bg(Color::Rgb(60, 60, 0))
                            .add_modifier(Modifier::BOLD)
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
            .collect();

        let selected = self.selected_idx;
        let selected_is_match = match_set.contains(&selected);

        let highlight_style = if selected_is_match {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        };

        let list = List::new(items)
            .block(block)
            .highlight_style(highlight_style);

        let mut list_state = ListState::default();
        list_state.select(Some(selected));
        f.render_stateful_widget(list, area, &mut list_state);
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
                .bg(ACTION_MENU_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .bg(ACTION_MENU_BG)
                .add_modifier(Modifier::BOLD)
        };
        lines.push(pad_line(
            Line::from(Span::styled(format!(" {}", menu.branch_name), name_style)),
            inner_w,
        ));
        lines.push(pad_line(
            Line::from(Span::styled(
                " ─────────────────────",
                Style::default().fg(Color::DarkGray).bg(ACTION_MENU_BG),
            )),
            inner_w,
        ));

        // Menu items
        for (idx, action) in BranchAction::ALL.iter().enumerate() {
            let is_selected = idx == menu.selected_idx;
            let key_char = action.key();
            let label = action.label();
            let item_bg = if is_selected {
                Color::DarkGray
            } else {
                ACTION_MENU_BG
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
            ));
        }

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan).bg(ACTION_MENU_BG))
            .style(Style::default().bg(ACTION_MENU_BG));

        let para = Paragraph::new(lines).block(block);
        f.render_widget(para, menu_area);
    }
}

fn pad_line(line: Line<'static>, width: usize) -> Line<'static> {
    let content_len: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
    if content_len < width {
        let mut spans = line.spans;
        spans.push(Span::styled(
            " ".repeat(width - content_len),
            Style::default().bg(ACTION_MENU_BG),
        ));
        Line::from(spans)
    } else {
        line
    }
}
