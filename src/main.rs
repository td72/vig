mod app;
mod event;
mod git;
mod github;
mod syntax;
mod tui;
mod ui;

use crate::app::{App, FocusedPane, ViewMode};
use crate::event::{Event, EventHandler};
use crate::git::repository::Repo;
use crate::git::watcher::FsWatcher;
use crate::ui::{
    branch_action_menu, branch_selector, commit_log, confirm_dialog, diff_view, file_tree,
    github as gh_ui, layout, reflog, status_bar,
};
use anyhow::Result;
use std::env;
use std::process::Command;
use std::time::Duration;

fn main() -> Result<()> {
    // Restore terminal on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = tui::restore();
        default_hook(info);
    }));

    let cwd = env::current_dir()?;
    let repo = Repo::discover(&cwd)?;
    let workdir = repo.workdir().to_path_buf();
    let mut app = App::new(repo)?;

    let events = EventHandler::new(Duration::from_millis(250));

    // Start file watcher
    let _watcher = FsWatcher::new(&workdir, events.tx())?;

    let mut terminal = tui::enter()?;

    loop {
        // Collect any completed background highlight results
        app.drain_bg_highlights();
        app.github.drain_bg_messages();

        // Draw
        terminal.draw(|frame| {
            match app.view_mode {
                ViewMode::Git => {
                    let layout = layout::compute_layout(frame.area());
                    status_bar::render_header(frame, &app, layout.header);
                    file_tree::render(frame, &app, layout.file_tree);
                    branch_selector::render(frame, &app, layout.branch_list);
                    reflog::render(frame, &mut app, layout.reflog);

                    match app.focused_pane {
                        FocusedPane::BranchList | FocusedPane::GitLog | FocusedPane::Reflog => {
                            commit_log::render(frame, &app, layout.main_pane);
                        }
                        _ => {
                            diff_view::render(frame, &mut app, layout.main_pane);
                        }
                    }

                    status_bar::render_status_bar(frame, &app, layout.status_bar);

                    if app.branch_action_menu.is_some() {
                        branch_action_menu::render(frame, &app, frame.area());
                    }

                    if app.error_dialog.is_some() {
                        confirm_dialog::render(frame, &app, frame.area());
                    }
                }
                ViewMode::GitHub => {
                    let gl = gh_ui::layout::compute_gh_layout(frame.area());
                    status_bar::render_gh_header(frame, &app, gl.header);
                    gh_ui::issue_list::render(frame, &app, gl.issue_list);
                    gh_ui::pr_list::render(frame, &app, gl.pr_list);
                    gh_ui::detail_view::render(frame, &mut app, gl.main_pane);
                    status_bar::render_gh_status_bar(frame, &app, gl.status_bar);
                }
            }

            if app.show_help {
                status_bar::render_help_overlay(frame, frame.area(), app.view_mode);
            }
        })?;

        // Handle events
        match events.next()? {
            Event::Key(key) => {
                // Skip release/repeat events
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }

                let open_editor = app.handle_key(key)?;

                if app.should_quit {
                    break;
                }

                if open_editor {
                    if let Some(file) = app.selected_file() {
                        let file_path = workdir.join(&file.path);
                        let editor = env::var("EDITOR")
                            .or_else(|_| env::var("VISUAL"))
                            .unwrap_or_else(|_| "vi".to_string());

                        // Pause event polling — blocks until the background
                        // thread has stopped calling crossterm::event::poll()
                        events.pause();
                        tui::restore()?;

                        let status = Command::new(&editor).arg(&file_path).status();

                        terminal = tui::enter()?;
                        // Flush stale terminal data before resuming the event thread
                        while crossterm::event::poll(Duration::ZERO)? {
                            let _ = crossterm::event::read();
                        }
                        events.drain();
                        events.resume();

                        match status {
                            Ok(s) if s.success() => {
                                app.refresh_diff()?;
                            }
                            Ok(s) => {
                                app.status_message =
                                    Some(format!("Editor exited with: {s}"));
                            }
                            Err(e) => {
                                app.status_message =
                                    Some(format!("Failed to open editor: {e}"));
                            }
                        }
                    }
                }
            }
            Event::FsChange => {
                app.load_branches();
                app.load_reflog();
                if let Err(e) = app.refresh_diff() {
                    app.status_message = Some(format!("Refresh error: {e}"));
                }
            }
            Event::Tick | Event::Resize(_, _) => {}
        }
    }

    tui::restore()?;
    Ok(())
}
