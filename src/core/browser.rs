use std::process::Command;

pub fn open_url(url: &str) -> Result<(), String> {
    // `start` is a cmd.exe builtin, not a standalone executable, so it must be
    // invoked via `cmd /C start`. The empty "" is start's window-title argument.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut c = Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut c = Command::new("xdg-open");
        c.arg(url);
        c
    };

    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to open URL: {e}"))?;
    Ok(())
}
