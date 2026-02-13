use self_update::cargo_crate_version;

const PUB_KEY: [u8; 32] = [
    0x74, 0xe2, 0x41, 0x07, 0x72, 0x61, 0x7e, 0xf3, 0xb5, 0x18, 0x85, 0x96,
    0x08, 0x7d, 0x50, 0xf7, 0x12, 0x05, 0xa3, 0x9f, 0xd0, 0x25, 0x3a, 0x8c,
    0x4b, 0x39, 0x89, 0x52, 0x5e, 0xf7, 0x03, 0x10,
];

pub fn run() -> anyhow::Result<()> {
    let updater = self_update::backends::github::Update::configure()
        .repo_owner("td72")
        .repo_name("vig")
        .bin_name("vig")
        .identifier("vig-")
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .verifying_keys([PUB_KEY])
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
