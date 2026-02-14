use self_update::cargo_crate_version;

const _: () = assert!(include_bytes!("../zipsign.pub").len() == 32);

pub fn run() -> anyhow::Result<()> {
    let updater = self_update::backends::github::Update::configure()
        .repo_owner("td72")
        .repo_name("vig")
        .bin_name("vig")
        .identifier("vig-")
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .verifying_keys([*include_bytes!("../zipsign.pub")])
        .build()?;

    match updater.update() {
        Ok(status) => {
            if status.updated() {
                println!("Updated to version: {}", status.version());
            } else {
                println!("Already up to date (v{}).", status.version());
            }
            Ok(())
        }
        Err(e) => {
            let msg = e.to_string().to_lowercase();
            if msg.contains("rate limit") {
                eprintln!("GitHub API rate limit exceeded. Please try again later.");
            } else if msg.contains("could not resolve")
                || msg.contains("failed to connect")
                || msg.contains("connection")
            {
                eprintln!("Failed to connect to GitHub. Please check your internet connection.");
            } else {
                eprintln!("Update failed: {e}");
            }
            Err(e.into())
        }
    }
}
