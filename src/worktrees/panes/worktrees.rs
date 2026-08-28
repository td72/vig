//! Top-left pane of the Worktrees page: the `git worktree list`.

use crate::core::app::AppContext;
use crate::core::keymap::{
    nav_bindings, search_bindings, ActionHelp, Keymap, NavAction, SearchAction,
};
use crate::core::pane::{self, Pane, PaneEvent, PaneShared};
use crate::core::search::SearchMatch;
use crate::core::theme;
use crate::worktrees::domain::types::Worktree;
use crate::worktrees::panes::fit_tail;
use crossterm::event::KeyCode;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::{layout::Rect, widgets::ListItem, Frame};

#[derive(Debug, Clone)]
pub enum WorktreesAction {
    Nav(NavAction),
    /// Move focus to the preview pane.
    FocusPreview,
    Search(SearchAction),
    Esc,
}

crate::impl_pane_action_from_str!(
    WorktreesAction, nav: Nav, search: Search, esc: Esc,
    FocusPreview
);

impl ActionHelp for WorktreesAction {
    fn label(&self) -> Option<&'static str> {
        match self {
            WorktreesAction::Nav(nav) => nav.label(),
            WorktreesAction::FocusPreview => Some("Focus preview"),
            WorktreesAction::Search(sa) => sa.label(),
            WorktreesAction::Esc => Some("Clear search"),
        }
    }
}

pub fn default_keymap() -> Keymap<WorktreesAction> {
    Keymap::new()
        .bindings(nav_bindings(WorktreesAction::Nav))
        .bindings(search_bindings(WorktreesAction::Search))
        .key(KeyCode::Char('i'), WorktreesAction::FocusPreview)
        .key(KeyCode::Enter, WorktreesAction::FocusPreview)
        .key(KeyCode::Char('l'), WorktreesAction::FocusPreview)
        .key(KeyCode::Esc, WorktreesAction::Esc)
}

pub struct WorktreesPane {
    pub items: Vec<Worktree>,
    pub selected_idx: usize,
    keymap: Keymap<WorktreesAction>,
    pane_id: usize,
    preview_pane_id: usize,
    view_height: u16,
}

impl WorktreesPane {
    pub fn new(pane_id: usize, preview_pane_id: usize) -> Self {
        Self {
            items: Vec::new(),
            selected_idx: 0,
            keymap: default_keymap(),
            pane_id,
            preview_pane_id,
            view_height: 20,
        }
    }

    pub fn set_keymap(&mut self, km: Keymap<WorktreesAction>) {
        self.keymap = km;
    }

    pub fn keymap(&self) -> &Keymap<WorktreesAction> {
        &self.keymap
    }

    pub fn selected(&self) -> Option<&Worktree> {
        self.items.get(self.selected_idx)
    }

    /// Replace the listing, keeping the selection on the same path when
    /// it still exists.
    pub fn set_items(&mut self, items: Vec<Worktree>) {
        let keep = self.selected().map(|w| w.path.clone());
        self.items = items;
        self.selected_idx = keep
            .and_then(|p| self.items.iter().position(|w| w.path == p))
            .unwrap_or(self.selected_idx)
            .min(self.items.len().saturating_sub(1));
    }

    fn execute(&mut self, shared: &PaneShared, action: WorktreesAction) -> Vec<PaneEvent> {
        if let Some(events) = pane::try_dispatch_search_esc(&action, shared, self.pane_id, vec![]) {
            return events;
        }
        match action {
            WorktreesAction::Nav(nav) => pane::execute_list_nav(
                nav,
                &mut self.selected_idx,
                self.items.len(),
                Some(self.view_height),
            ),
            WorktreesAction::FocusPreview if !self.items.is_empty() => {
                vec![PaneEvent::SetFocus(self.preview_pane_id)]
            }
            _ => vec![],
        }
    }

    fn row(
        &self,
        wt: &Worktree,
        hl: &theme::SearchHighlight,
        path_w: usize,
        ref_w: usize,
    ) -> Line<'static> {
        let current_style = Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD);
        let path_style = if hl.is_active() {
            hl.apply(Style::default())
        } else if wt.is_current {
            current_style
        } else {
            Style::default()
        };
        let ref_style = hl.style_with_fg(if wt.branch.is_some() {
            Color::Blue
        } else {
            Color::Magenta
        });
        let mut spans = vec![
            Span::raw(" "),
            Span::styled(
                if wt.is_current { "* " } else { "  " },
                if hl.is_active() {
                    hl.apply(current_style)
                } else {
                    current_style
                },
            ),
            Span::styled(fit_tail(&wt.display_path, path_w), path_style),
            Span::raw("  "),
            Span::styled(format!("{:<ref_w$}", wt.ref_label()), ref_style),
        ];
        for flag in wt.flags() {
            let color = match flag {
                "locked" => Color::Yellow,
                "prunable" => Color::Red,
                // Gray (not DarkGray) stays readable on the selection bg.
                _ => Color::Gray,
            };
            spans.push(Span::raw(" "));
            spans.push(Span::styled(format!("[{flag}]"), hl.style_with_fg(color)));
        }
        Line::from(spans)
    }
}

impl Pane<PaneEvent> for WorktreesPane {
    crate::impl_handle_key!(keymap);

    fn render(&mut self, f: &mut Frame, _ctx: &AppContext, shared: &PaneShared, area: Rect) {
        self.view_height = area.height.saturating_sub(2);
        let width = area.width.saturating_sub(2) as usize;
        let empty = self.items.is_empty().then_some("No worktrees");
        let is_focused = shared.focused_pane == self.pane_id;
        let show_selection = is_focused
            || (shared.focused_pane == self.preview_pane_id
                && shared.previous_pane == self.pane_id);
        let selected = show_selection.then_some(self.selected_idx);

        // Column widths: paths get at most half the pane, refs whatever they need.
        let path_w = self
            .items
            .iter()
            .map(|w| w.display_path.chars().count())
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(6) / 2)
            .max(1);
        let ref_w = self
            .items
            .iter()
            .map(|w| w.ref_label().chars().count())
            .max()
            .unwrap_or(0);

        theme::render_list_pane(
            f,
            area,
            shared,
            self.pane_id,
            "Worktrees",
            selected,
            empty,
            |match_set, current_match_idx| {
                self.items
                    .iter()
                    .enumerate()
                    .map(|(idx, wt)| {
                        let hl = theme::search_highlight_for(match_set, current_match_idx, idx);
                        let mut li = ListItem::new(self.row(wt, &hl, path_w, ref_w));
                        if hl.is_active() {
                            li = li.style(hl.apply(Style::default()));
                        }
                        li
                    })
                    .collect()
            },
        );
    }

    fn collect_search_matches(&self, _shared: &PaneShared, query: &str) -> Vec<SearchMatch> {
        pane::collect_list_search_matches(&self.items, query, |w| {
            format!("{} {}", w.display_path, w.ref_label())
        })
    }

    crate::impl_list_pane_selection!();
}
