mod core;
mod git;
mod github;
mod update;

use crate::core::app::{App, AppContext};
use crate::core::config::source::ConfigSource;
use crate::core::config::Config;
use crate::core::event::{Event, EventHandler};
use crate::core::page::PageAction;
use crate::core::ui::{confirm_dialog, status_bar};
use crate::git::watcher::FsWatcher;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Interval between tick events driving periodic UI updates (e.g. watch mode).
const TICK_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Config file to use (overrides $VIG_CONFIG and ~/.config/vig/config.kdl)
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Update vig to the latest version
    Update,
    /// Inspect the configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommands,
    },
}

#[derive(Subcommand)]
enum ConfigCommands {
    /// Print the config file path that would be used
    Path,
    /// Print the built-in default config (copy it to start your own)
    Dump,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Update) => update::run()?,
        Some(Commands::Config { command }) => run_config(command, cli.config),
        None => {
            let cfg = crate::core::config::source::load(cli.config)?;
            run_tui(cfg)?
        }
    }

    Ok(())
}

fn run_config(command: ConfigCommands, explicit: Option<PathBuf>) {
    match command {
        ConfigCommands::Path => match ConfigSource::resolve(explicit) {
            ConfigSource::Unavailable => {
                eprintln!("no config path: home directory could not be determined")
            }
            source => {
                let path = source.path().expect("resolved source has a path");
                println!("{}", path.display());
                if !path.exists() {
                    eprintln!("(not found; built-in defaults are in effect)");
                }
            }
        },
        ConfigCommands::Dump => print!("{}", Config::default_text()),
    }
}

fn run_tui(cfg: Config) -> Result<()> {
    // Restore terminal on panic
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crate::core::tui::restore();
        default_hook(info);
    }));

    let cwd = std::env::current_dir()?;
    let (git_page, workdir) = crate::git::page::new_page(&cwd, &cfg)?;
    let gh_page = crate::github::page::new_page(&cfg)?;

    let pages = vec![git_page, gh_page];
    let page_labels = pages.iter().map(|p| p.label()).collect();
    let ctx = AppContext {
        should_quit: false,
        active_page: 0,
        page_labels,
        show_help: false,
        status_message: None,
        error_dialog: None,
        workdir: workdir.clone(),
    };
    let mut app = App::new(ctx, pages, &cfg)?;

    let events = EventHandler::new(TICK_INTERVAL);

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
                let bindings = app.active_help_bindings();
                status_bar::render_help_overlay(frame, area, &bindings);
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

                if let PageAction::Suspend(cmd) = action {
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
