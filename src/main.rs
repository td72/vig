mod core;
mod docker;
mod files;
mod git;
mod github;
mod pages;
mod procs;
mod projects;
mod update;
mod worktrees;

use crate::core::app::{App, AppContext};
use crate::core::config::repo::{self, RepoLayer};
use crate::core::config::source::ConfigSource;
use crate::core::config::trust::{self, TrustDecision, TrustStore};
use crate::core::config::Config;
use crate::core::event::{Event, EventHandler};
use crate::core::page::PageAction;
use crate::core::ui::trust_dialog::{self, TrustChoice};
use crate::core::ui::{confirm_dialog, status_bar};
use crate::git::domain::repository::Repo;
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
    /// List the config layers (builtin / user / repo-local) and their status
    Path,
    /// Print the built-in default config (copy it to start your own)
    Dump,
    /// List the available syntax highlighting themes (`*` marks the active one)
    Themes,
    /// List remembered .vig.kdl trust decisions
    Trust {
        /// Forget the decision remembered for this worktree path
        #[arg(long, value_name = "PATH")]
        forget: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Update) => update::run()?,
        Some(Commands::Config { command }) => run_config(command, cli.config)?,
        None => {
            let cfg = crate::core::config::source::load(cli.config)?;
            run_tui(cfg)?
        }
    }

    Ok(())
}

fn run_config(command: ConfigCommands, explicit: Option<PathBuf>) -> Result<()> {
    match command {
        ConfigCommands::Path => print_config_layers(explicit)?,
        ConfigCommands::Dump => print!("{}", Config::default_text()),
        ConfigCommands::Themes => {
            let active = crate::core::config::source::load(explicit)?.theme()?;
            for name in crate::core::syntax::theme_names() {
                let mark = if name == active { '*' } else { ' ' };
                println!("{mark} {name}");
            }
        }
        ConfigCommands::Trust { forget } => run_config_trust(forget)?,
    }
    Ok(())
}

/// `vig config path`: one line per config layer with its path and status.
fn print_config_layers(explicit: Option<PathBuf>) -> Result<()> {
    println!("{:<11} {:<52} loaded", "builtin", "(embedded defaults)");

    let src = ConfigSource::resolve(explicit.clone());
    let loaded = crate::core::config::source::load(explicit);
    let (user_path, user_status) = match &src {
        ConfigSource::Unavailable => (
            "(none)".to_string(),
            "not found (no home directory; built-in defaults in effect)".to_string(),
        ),
        ConfigSource::Explicit(p) | ConfigSource::Default(p) => {
            let status = if !p.exists() {
                if matches!(src, ConfigSource::Explicit(_)) {
                    format!(
                        "not found (vig will not start; fix --config / ${})",
                        crate::core::config::source::ENV_VAR
                    )
                } else {
                    "not found (built-in defaults in effect)".to_string()
                }
            } else {
                match &loaded {
                    Ok(_) => "loaded".to_string(),
                    Err(e) => format!("invalid ({})", repo::summarize(e)),
                }
            };
            (p.display().to_string(), status)
        }
    };
    println!("{:<11} {user_path:<52} {user_status}", "user");

    let (repo_path, repo_status) = match &loaded {
        Err(_) => (
            "-".to_string(),
            "not evaluated (fix the user config first)".to_string(),
        ),
        Ok(cfg) => match Repo::discover(&std::env::current_dir()?) {
            Err(_) => ("-".to_string(), "not in a git repository".to_string()),
            Ok(r) => {
                let workdir = r.workdir().to_path_buf();
                let store = TrustStore::load_default();
                let layer = repo::classify(&workdir, cfg.repo_config()?, &store, repo::is_tracked);
                let apply_error = match &layer {
                    RepoLayer::Load { path, text } => repo::apply(cfg, path, text)
                        .err()
                        .map(|e| repo::summarize(&e)),
                    _ => None,
                };
                let path = match &layer {
                    RepoLayer::Absent { path }
                    | RepoLayer::Disabled { path }
                    | RepoLayer::Declined { path }
                    | RepoLayer::Load { path, .. }
                    | RepoLayer::Undecided { path, .. } => path.display().to_string(),
                };
                (path, repo::status_text(&layer, apply_error.as_deref()))
            }
        },
    };
    println!("{:<11} {repo_path:<52} {repo_status}", "repo-local");
    Ok(())
}

/// `vig config trust [--forget <path>]`: the remembered `.vig.kdl` decisions.
fn run_config_trust(forget: Option<PathBuf>) -> Result<()> {
    let store_path = TrustStore::default_path().ok_or_else(|| {
        anyhow::anyhow!("no state directory: home directory could not be determined")
    })?;
    let mut store = TrustStore::load_from(&store_path);
    match forget {
        Some(target) => {
            let key = target
                .canonicalize()
                .unwrap_or(target)
                .to_string_lossy()
                .into_owned();
            if !store.forget(&key) {
                anyhow::bail!("no remembered decision for {key}");
            }
            store.save_to(&store_path)?;
            println!("forgot {key}");
        }
        None => {
            if store.entries.is_empty() {
                println!("no remembered decisions");
            }
            for e in &store.entries {
                println!(
                    "{:<7} {}  {}  {}",
                    e.decision.to_string(),
                    trust::format_unix(e.decided_at),
                    &e.hash[..12.min(e.hash.len())],
                    e.path
                );
            }
        }
    }
    Ok(())
}

/// Merge the repository-local `.vig.kdl` over `cfg`, asking for trust when
/// the file is tracked and not yet decided — a pre-app dialog, because the
/// answer decides which pages, layouts and keybindings exist. Returns the
/// effective config and the startup status-bar notice.
fn apply_repo_config_layer(cfg: Config, cwd: &std::path::Path) -> Result<(Config, Option<String>)> {
    // Outside a repository, build_pages reports "Not a git repository".
    let Ok(r) = Repo::discover(cwd) else {
        return Ok((cfg, None));
    };
    let workdir = r.workdir().to_path_buf();
    let mut store = TrustStore::load_default();
    let layer = repo::classify(&workdir, cfg.repo_config()?, &store, repo::is_tracked);
    let (path, text) = match layer {
        RepoLayer::Absent { .. } | RepoLayer::Disabled { .. } | RepoLayer::Declined { .. } => {
            return Ok((cfg, None))
        }
        RepoLayer::Load { path, text } => (path, text),
        RepoLayer::Undecided { path, text, hash } => {
            let mut terminal = crate::core::tui::enter()?;
            let choice = trust_dialog::run(&mut terminal, &path, &text);
            crate::core::tui::restore()?;
            match choice? {
                TrustChoice::LoadRemember => {
                    store.remember(&workdir, &hash, TrustDecision::Load);
                    if let Err(e) = store.save_default() {
                        eprintln!("vig: could not save the trust decision: {e:#}");
                    }
                    (path, text)
                }
                TrustChoice::IgnoreRemember => {
                    store.remember(&workdir, &hash, TrustDecision::Ignore);
                    if let Err(e) = store.save_default() {
                        eprintln!("vig: could not save the trust decision: {e:#}");
                    }
                    return Ok((cfg, None));
                }
                TrustChoice::IgnoreOnce => return Ok((cfg, None)),
            }
        }
    };
    match repo::apply(&cfg, &path, &text) {
        Ok(merged) => Ok((merged, Some("loaded .vig.kdl".to_string()))),
        Err(e) => {
            eprintln!("vig: ignored {}: {e:#}", path.display());
            let summary = repo::summarize(&e);
            Ok((cfg, Some(format!("ignored .vig.kdl: {summary}"))))
        }
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
    // The repo-local `.vig.kdl` layer must be settled before anything reads
    // the config: it may change pages, layouts and keybindings.
    let (cfg, startup_notice) = apply_repo_config_layer(cfg, &cwd)?;
    // Terminal graphics detection talks to the terminal directly, so it has
    // to happen before the TUI takes over stdin/stdout — and only when the
    // Files page (the sole consumer) is enabled.
    let picker = if cfg.pages()?.iter().any(|p| p == "files") {
        crate::files::domain::image::make_picker(cfg.image_preview()?)
    } else {
        None
    };
    // Pages come back in the slot order of `pages "git" "github" ...`.
    let (pages, workdir) = crate::pages::build_pages(&cfg, &cwd, picker)?;
    let page_labels = pages.iter().map(|p| p.label()).collect();
    let ctx = AppContext {
        should_quit: false,
        active_page: 0,
        page_labels,
        page_keys: Vec::new(),
        show_help: false,
        status_message: startup_notice,
        error_dialog: None,
        workdir: workdir.clone(),
        needs_full_redraw: false,
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
        if app.ctx.take_full_redraw() {
            terminal.clear()?;
        }
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
