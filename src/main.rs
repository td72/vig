mod core;
mod git;
mod github;
mod update;

use crate::core::app::App;
use crate::core::event::{Event, EventHandler};
use crate::core::ui::{confirm_dialog, status_bar};
use crate::core::view::ViewAction;
use crate::git::watcher::FsWatcher;
use anyhow::Result;
use clap::{Parser, Subcommand};
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

    let cwd = std::env::current_dir()?;
    let mut app = App::new(&cwd)?;
    let workdir = app.workdir().to_path_buf();

    let events = EventHandler::new(Duration::from_millis(250));

    // Start file watcher
    let _watcher = FsWatcher::new(&workdir, events.tx())?;

    let mut terminal = crate::core::tui::enter()?;

    loop {
        // Collect any completed background results
        app.drain_all_background();

        // Draw
        terminal.draw(|frame| {
            let area = frame.area();
            app.render(frame, area);

            // Shared overlays (rendered on top of any view)
            if app.ctx.error_dialog.is_some() {
                confirm_dialog::render(frame, &app.ctx, area);
            }
            if app.ctx.show_help {
                status_bar::render_help_overlay(frame, area, app.ctx.view_mode);
            }
        })?;

        // Handle events
        match events.next()? {
            Event::Key(key) => {
                // Skip release/repeat events
                if key.kind != crossterm::event::KeyEventKind::Press {
                    continue;
                }

                let action = app.handle_key(key)?;

                if app.ctx.should_quit {
                    break;
                }

                if let ViewAction::Suspend(cmd) = action {
                    events.pause();
                    crate::core::tui::restore()?;

                    let status = Command::new(&cmd.program).args(&cmd.args).status();

                    terminal = crate::core::tui::enter()?;
                    while crossterm::event::poll(Duration::ZERO)? {
                        let _ = crossterm::event::read();
                    }
                    events.drain();
                    events.resume();

                    app.on_suspend_return(status)?;
                }
            }
            Event::FsChange => {
                app.on_fs_change()?;
            }
            Event::Tick => {
                app.on_tick();
            }
            Event::Resize(_, _) => {}
        }
    }

    crate::core::tui::restore()?;
    Ok(())
}
