use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Build the platform launcher for `target` (a URL or a path). With `app`,
/// the target is opened with that application instead of the default one.
fn launcher(app: Option<&str>, target: &OsStr) -> Command {
    #[cfg(target_os = "windows")]
    {
        // `start` is a cmd.exe builtin, not a standalone executable, so it must
        // be invoked via `cmd /C start`. The empty "" is start's window-title
        // argument.
        let mut c = Command::new("cmd");
        c.args(["/C", "start", ""]);
        if let Some(app) = app {
            c.arg(app);
        }
        c.arg(target);
        c
    }
    #[cfg(target_os = "macos")]
    {
        let mut c = Command::new("open");
        if let Some(app) = app {
            c.arg("-a").arg(app);
        }
        c.arg(target);
        c
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let mut c = match app {
            Some(app) => Command::new(app),
            None => Command::new("xdg-open"),
        };
        c.arg(target);
        c
    }
}

/// Spawn `command` detached from the TUI (all stdio to null) and report
/// launch failures. The launcher helpers (`open`, `xdg-open`) exit almost
/// immediately, so a short grace period catches "no such application" style
/// failures that only surface through the exit status.
fn spawn_detached(mut command: Command, what: &str) -> Result<(), String> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to open {what}: {e}"))?;
    std::thread::sleep(Duration::from_millis(150));
    match child.try_wait() {
        Ok(Some(status)) if !status.success() => Err(format!(
            "Failed to open {what}: {} exited with {status}",
            command.get_program().to_string_lossy()
        )),
        _ => Ok(()),
    }
}

pub fn open_url(url: &str) -> Result<(), String> {
    spawn_detached(launcher(None, OsStr::new(url)), "URL")
}

/// Open `path` with the OS default application.
pub fn open_path(path: &Path) -> Result<(), String> {
    spawn_detached(launcher(None, path.as_os_str()), "file")
}

/// Open `path` with the named application (`open -a <app>` on macOS; the
/// application is run directly elsewhere).
pub fn open_path_with(app: &str, path: &Path) -> Result<(), String> {
    spawn_detached(launcher(Some(app), path.as_os_str()), "file")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(c: &Command) -> (String, Vec<String>) {
        (
            c.get_program().to_string_lossy().into_owned(),
            c.get_args()
                .map(|a| a.to_string_lossy().into_owned())
                .collect(),
        )
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_launcher_uses_open() {
        assert_eq!(
            parts(&launcher(None, OsStr::new("a.pdf"))),
            ("open".into(), vec!["a.pdf".into()])
        );
        assert_eq!(
            parts(&launcher(Some("Preview"), OsStr::new("a.pdf"))),
            (
                "open".into(),
                vec!["-a".into(), "Preview".into(), "a.pdf".into()]
            )
        );
    }

    #[test]
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    fn unix_launcher_uses_xdg_open_or_the_app() {
        assert_eq!(
            parts(&launcher(None, OsStr::new("a.pdf"))),
            ("xdg-open".into(), vec!["a.pdf".into()])
        );
        assert_eq!(
            parts(&launcher(Some("evince"), OsStr::new("a.pdf"))),
            ("evince".into(), vec!["a.pdf".into()])
        );
    }
}
