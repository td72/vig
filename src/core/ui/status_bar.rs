use crate::core::app::AppContext;
use crate::docker::state::DockerState;
use crate::files::state::FilesState;
use crate::git::state::GitState;
use crate::github::state::GitHubState;
use crate::procs::state::ProcsState;
use crate::projects::state::ProjectsState;
use crate::worktrees::state::WorktreesState;
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

fn page_tab_spans(ctx: &AppContext) -> Vec<Span<'static>> {
    let active_style = Style::default()
        .fg(Color::Black)
        .bg(Color::White)
        .add_modifier(Modifier::BOLD);
    let inactive_style = Style::default().fg(Color::DarkGray);

    let mut spans = vec![Span::raw("  ")];
    for (i, label) in ctx.page_labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw(" "));
        }
        let style = if i == ctx.active_page {
            active_style
        } else {
            inactive_style
        };
        // The number is the page's slot (tab position), not a key: keys are
        // bindings *onto* slots and are listed in the help overlay instead.
        spans.push(Span::styled(format!(" {}:{} ", i + 1, label), style));
    }
    spans
}

fn render_header_common(
    f: &mut Frame,
    ctx: &AppContext,
    context_spans: Vec<Span<'static>>,
    area: Rect,
) {
    let mut spans = vec![
        Span::styled(
            " vig ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    spans.extend(context_spans);
    spans.extend(page_tab_spans(ctx));
    spans.push(Span::raw("  "));
    spans.push(Span::styled("? help", Style::default().fg(Color::DarkGray)));

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_header(f: &mut Frame, ctx: &AppContext, git: &GitState, area: Rect) {
    let base_label = match &git.diff_base_ref {
        Some(base) => format!(" vs {base} "),
        None => " vs HEAD ".to_string(),
    };
    render_header_common(
        f,
        ctx,
        vec![
            Span::styled(
                format!(" {} ", git.diff_meta.branch_name),
                Style::default().fg(Color::Black).bg(Color::Magenta),
            ),
            Span::raw(" "),
            Span::styled(
                base_label,
                Style::default().fg(Color::Black).bg(Color::Yellow),
            ),
        ],
        area,
    );
}

pub fn render_gh_header(f: &mut Frame, ctx: &AppContext, area: Rect) {
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            " GitHub ",
            Style::default().fg(Color::Black).bg(Color::Rgb(36, 41, 47)),
        )],
        area,
    );
}

fn render_search_prompt(f: &mut Frame, input: &str, area: Rect) {
    render_prompt(f, "/", input, area);
}

/// One-line input prompt in the status bar: `<label><input>█`.
fn render_prompt(f: &mut Frame, label: &str, input: &str, area: Rect) {
    let line = Line::from(Span::styled(
        format!(" {label}{input}\u{2588}"),
        Style::default().fg(Color::White),
    ));
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_files_header(f: &mut Frame, ctx: &AppContext, files: &FilesState, area: Rect) {
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            format!(" {} ", files.cwd_display()),
            Style::default().fg(Color::Black).bg(Color::Blue),
        )],
        area,
    );
}

pub fn render_files_status_bar(f: &mut Frame, ctx: &AppContext, files: &FilesState, area: Rect) {
    if files.pane.search.active {
        render_search_prompt(f, &files.pane.search.input, area);
        return;
    }
    if files.open_with.active {
        render_prompt(f, "Open with: ", &files.open_with.input, area);
        return;
    }
    let line = if let Some(ref msg) = ctx.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        let list = &files.panes.tab.list;
        let n = list.entries.len();
        let mut spans = vec![Span::styled(
            format!(" {n} item{}", if n == 1 { "" } else { "s" }),
            Style::default().fg(Color::White),
        )];
        if let Some(e) = files.selected() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                e.path
                    .strip_prefix(&files.root)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| e.path.to_string_lossy().into_owned()),
                Style::default().fg(Color::DarkGray),
            ));
        }
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_docker_header(f: &mut Frame, ctx: &AppContext, area: Rect) {
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            " Docker ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Rgb(36, 150, 237)),
        )],
        area,
    );
}

pub fn render_docker_status_bar(f: &mut Frame, ctx: &AppContext, dk: &DockerState, area: Rect) {
    if dk.pane.search.active {
        render_search_prompt(f, &dk.pane.search.input, area);
        return;
    }
    if let Some(ref err) = dk.docker_error {
        let line = Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let (containers, running, images) = dk.counts();
    let mut spans = Vec::new();
    if dk.is_updating() && containers == 0 && images == 0 {
        spans.push(Span::styled(
            " Loading...",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            format!(
                " {containers} container{} ({running} running)",
                if containers == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::White),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{images} image{}", if images == 1 { "" } else { "s" }),
            Style::default().fg(Color::White),
        ));
        if dk.is_updating() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "↻ Updating...",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    if let Some(ref msg) = ctx.status_message {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_procs_header(f: &mut Frame, ctx: &AppContext, area: Rect) {
    // Deliberately no hostname: the header must not leak machine names
    // into screenshots and recordings.
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            " Procs ",
            Style::default().fg(Color::Black).bg(Color::Green),
        )],
        area,
    );
}

pub fn render_procs_status_bar(f: &mut Frame, ctx: &AppContext, procs: &ProcsState, area: Rect) {
    if procs.pane.search.active {
        render_search_prompt(f, &procs.pane.search.input, area);
        return;
    }
    let line = if let Some(ref msg) = ctx.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else {
        let n = procs.panes.tab.list.len();
        let m = procs.panes.ports.entries.len();
        let mut spans = vec![
            Span::styled(
                format!(" {n} process{}", if n == 1 { "" } else { "es" }),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{m} listening port{}", if m == 1 { "" } else { "s" }),
                Style::default().fg(Color::White),
            ),
        ];
        if procs.is_refreshing() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "↻ Updating...",
                Style::default().fg(Color::DarkGray),
            ));
        }
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_worktrees_header(f: &mut Frame, ctx: &AppContext, wt: &WorktreesState, area: Rect) {
    let current = wt
        .panes
        .worktrees
        .items
        .iter()
        .find(|w| w.is_current)
        .map(|w| w.display_path.clone())
        .unwrap_or_else(|| "Worktrees".to_string());
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            format!(" {current} "),
            Style::default().fg(Color::Black).bg(Color::Green),
        )],
        area,
    );
}

pub fn render_worktrees_status_bar(
    f: &mut Frame,
    ctx: &AppContext,
    wt: &WorktreesState,
    area: Rect,
) {
    if wt.pane.search.active {
        render_search_prompt(f, &wt.pane.search.input, area);
        return;
    }
    let line = if let Some(ref msg) = ctx.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else if let Some(ref err) = wt.error {
        Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ))
    } else {
        let n = wt.panes.worktrees.items.len();
        let m = wt.panes.stashes.items.len();
        let mut spans = vec![
            Span::styled(
                format!(" {n} worktree{}", if n == 1 { "" } else { "s" }),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{m} stash{}", if m == 1 { "" } else { "es" }),
                Style::default().fg(Color::White),
            ),
        ];
        let ids = wt.panes.ids;
        let detail = if wt.pane.focused_pane == ids.stashes {
            wt.panes.stashes.selected().map(|s| s.message.clone())
        } else if wt.pane.focused_pane == ids.worktrees {
            wt.panes
                .worktrees
                .selected()
                .map(|w| w.path.to_string_lossy().into_owned())
        } else {
            None
        };
        if let Some(detail) = detail.filter(|d| !d.is_empty()) {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(detail, Style::default().fg(Color::DarkGray)));
        }
        Line::from(spans)
    };
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_projects_header(f: &mut Frame, ctx: &AppContext, area: Rect) {
    render_header_common(
        f,
        ctx,
        vec![Span::styled(
            " Projects ",
            Style::default()
                .fg(Color::White)
                .bg(Color::Rgb(130, 80, 160)),
        )],
        area,
    );
}

pub fn render_projects_status_bar(f: &mut Frame, ctx: &AppContext, pj: &ProjectsState, area: Rect) {
    if pj.pane.search.active {
        render_search_prompt(f, &pj.pane.search.input, area);
        return;
    }
    if pj.scope_missing {
        let line = Line::from(Span::styled(
            format!(" {}", crate::projects::state::SCOPE_NOTICE),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    if let Some(ref err) = pj.gh_error {
        let line = Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let (projects, items, columns, truncated) = pj.counts();
    let mut spans = Vec::new();
    if pj.is_loading() && projects == 0 {
        spans.push(Span::styled(
            " Loading...",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            format!(
                " {projects} project{}",
                if projects == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::White),
        ));
        if pj.panes.board.board.is_some() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!(
                    "{items} item{} in {columns} column{}",
                    if items == 1 { "" } else { "s" },
                    if columns == 1 { "" } else { "s" }
                ),
                Style::default().fg(Color::White),
            ));
            if truncated {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!(
                        "(truncated at {})",
                        crate::projects::domain::client::ITEM_LIMIT
                    ),
                    Style::default().fg(Color::Yellow),
                ));
            }
        }
        if pj.is_loading() {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "↻ Updating...",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }
    if let Some(ref msg) = ctx.status_message {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn render_status_bar(f: &mut Frame, ctx: &AppContext, git: &GitState, area: Rect) {
    if git.pane.search.active {
        render_search_prompt(f, &git.pane.search.input, area);
        return;
    }

    let file_count = git.diff_meta.file_count;
    let adds = git.diff_meta.stats.additions;
    let dels = git.diff_meta.stats.deletions;

    let status = if let Some(ref msg) = ctx.status_message {
        Line::from(Span::styled(
            format!(" {msg}"),
            Style::default().fg(Color::Yellow),
        ))
    } else if file_count == 0 {
        Line::from(Span::styled(
            " Working tree clean",
            Style::default().fg(Color::Green),
        ))
    } else {
        Line::from(vec![
            Span::styled(
                format!(
                    " {file_count} file{}",
                    if file_count == 1 { "" } else { "s" }
                ),
                Style::default().fg(Color::White),
            ),
            Span::raw("  "),
            Span::styled(format!("+{adds}"), Style::default().fg(Color::Green)),
            Span::raw(" "),
            Span::styled(format!("-{dels}"), Style::default().fg(Color::Red)),
        ])
    };

    f.render_widget(Paragraph::new(status), area);
}

pub fn render_gh_status_bar(f: &mut Frame, ctx: &AppContext, gh: &GitHubState, area: Rect) {
    if gh.pane.search.active {
        render_search_prompt(f, &gh.pane.search.input, area);
        return;
    }

    if let Some(ref err) = gh.gh_error {
        let line = Line::from(Span::styled(
            format!(" {err}"),
            Style::default().fg(Color::Red),
        ));
        f.render_widget(Paragraph::new(line), area);
        return;
    }

    let issue_count = gh.panes.issue_tab.list.item_count();
    let pr_count = gh.panes.pr_tab.list.item_count();
    let (run_count, active_runs) = gh.run_counts();

    let loading = gh.panes.issue_tab.list.is_loading()
        || gh.panes.pr_tab.list.is_loading()
        || gh.panes.run_tab.list.is_loading()
        || gh.panes.run_tab.detail.is_run_loading();
    let has_data = issue_count > 0 || pr_count > 0 || run_count > 0;

    let mut spans = Vec::new();
    if loading && !has_data {
        // Initial load (no cache) — show Loading...
        spans.push(Span::styled(
            " Loading...",
            Style::default().fg(Color::DarkGray),
        ));
    } else {
        spans.push(Span::styled(
            format!(
                " {} issue{}",
                issue_count,
                if issue_count == 1 { "" } else { "s" }
            ),
            Style::default().fg(Color::White),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} PR{}", pr_count, if pr_count == 1 { "" } else { "s" }),
            Style::default().fg(Color::White),
        ));
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("{} run{}", run_count, if run_count == 1 { "" } else { "s" }),
            Style::default().fg(Color::White),
        ));
        if active_runs > 0 {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("◐ {active_runs} active"),
                Style::default().fg(Color::Yellow),
            ));
        }
        if loading {
            // Background refresh with cached data visible
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "↻ Updating...",
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    if let Some(ws) = gh.panes.pr_tab.detail.watch_status() {
        spans.push(Span::raw("  "));
        if let Some(ref err) = ws.error {
            spans.push(Span::styled(
                format!("\u{23f1} Watch (err: {err})"),
                Style::default().fg(Color::Red),
            ));
        } else {
            spans.push(Span::styled(
                format!("\u{23f1} Watch (last: {})", ws.last_update_time),
                Style::default().fg(Color::Yellow),
            ));
        }
    }

    // Status message (overrides if present)
    if let Some(ref msg) = ctx.status_message {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            msg.clone(),
            Style::default().fg(Color::Yellow),
        ));
    }

    let line = Line::from(spans);
    f.render_widget(Paragraph::new(line), area);
}

pub fn render_help_overlay(f: &mut Frame, area: Rect, keybindings: &[(String, String)]) {
    use ratatui::widgets::{Block, Borders, Clear};

    // Key column grows with the widest key group (user configs can bind
    // several keys to one action, e.g. "→ / Enter / o").
    let key_w = keybindings
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(0)
        .max(12);
    let desc_w = keybindings
        .iter()
        .map(|(_, d)| d.chars().count())
        .max()
        .unwrap_or(0);
    // 2 (indent) + key + 2 (gap) + desc + 2 (border)
    let help_width = ((key_w + desc_w + 6) as u16)
        .max(50)
        .min(area.width.saturating_sub(4));
    let help_height = ((keybindings.len() as u16) + 2).min(area.height.saturating_sub(4));
    let x = (area.width.saturating_sub(help_width)) / 2;
    let y = (area.height.saturating_sub(help_height)) / 2;
    let help_area = Rect::new(x, y, help_width, help_height);

    f.render_widget(Clear, help_area);

    let lines: Vec<Line> = keybindings
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(
                    format!("  {key:<key_w$}  "),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(desc.as_str()),
            ])
        })
        .collect();

    let block = Block::default()
        .title(" Keybindings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, help_area);
}
