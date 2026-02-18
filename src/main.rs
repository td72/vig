mod core;
mod git;
mod github;
mod update;

use crate::core::app::{App, ViewMode};
use crate::core::event::{Event, EventHandler};
use crate::core::ui::{confirm_dialog, status_bar};
use crate::git::watcher::FsWatcher;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::env;
use std::process::Command;
use std::time::Duration;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Update vig to the latest version
    Update,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Update) => update::run()?,
        None => run_tui()?,
    }

    Ok(())
}

fn run_tui() -> Result<()> {
    // Restore terminal on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crate::core::tui::restore();
        default_hook(info);
    }));

    let cwd = env::current_dir()?;
    let mut app = App::new(&cwd)?;
    let workdir = app.git.repo.workdir().to_path_buf();

    let events = EventHandler::new(Duration::from_millis(250));

    // Start file watcher
    let _watcher = FsWatcher::new(&workdir, events.tx())?;

    let mut terminal = crate::core::tui::enter()?;

    loop {
        // Collect any completed background highlight results
        app.git.drain_bg_highlights();
        app.github.drain_bg_messages();

        // Draw
        terminal.draw(|frame| {
            let view = app.active_view();
            view.render(frame, &mut app, frame.area());

            // Shared overlays (rendered on top of any view)
            if app.ctx.error_dialog.is_some() {
                confirm_dialog::render(frame, &app, frame.area());
            }
            if app.ctx.show_help {
                status_bar::render_help_overlay(frame, frame.area(), app.ctx.view_mode);
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

                if app.ctx.should_quit {
                    break;
                }

                if open_editor {
                    if let Some(file) = app.git.selected_file() {
                        let file_path = workdir.join(&file.path);
                        let editor = env::var("EDITOR")
                            .or_else(|_| env::var("VISUAL"))
                            .unwrap_or_else(|_| "vi".to_string());

                        // Pause event polling — blocks until the background
                        // thread has stopped calling crossterm::event::poll()
                        events.pause();
                        crate::core::tui::restore()?;

                        let status = Command::new(&editor).arg(&file_path).status();

                        terminal = crate::core::tui::enter()?;
                        // Flush stale terminal data before resuming the event thread
                        while crossterm::event::poll(Duration::ZERO)? {
                            let _ = crossterm::event::read();
                        }
                        events.drain();
                        events.resume();

                        match status {
                            Ok(s) if s.success() => {
                                app.post_edit_refresh()?;
                            }
                            Ok(s) => {
                                app.ctx.status_message =
                                    Some(format!("Editor exited with: {s}"));
                            }
                            Err(e) => {
                                app.ctx.status_message =
                                    Some(format!("Failed to open editor: {e}"));
                            }
                        }
                    }
                }
            }
            Event::FsChange => {
                app.git.load_branches();
                app.git.load_reflog();
                if let Err(e) = app.refresh_diff() {
                    app.ctx.status_message = Some(format!("Refresh error: {e}"));
                }
            }
            Event::Tick => {
                if app.ctx.view_mode == ViewMode::GitHub {
                    app.github.handle_watch_tick();
                }
            }
            Event::Resize(_, _) => {}
        }
    }

    crate::core::tui::restore()?;
    Ok(())
}
