use self_update::cargo_crate_version;

pub fn run() -> anyhow::Result<()> {
    let status = self_update::backends::github::Update::configure()
        .repo_owner("td72")
        .repo_name("vig")
        .bin_name("vig")
        .identifier("vig-")
        .show_download_progress(true)
        .current_version(cargo_crate_version!())
        .build()?
        .update()?;
    println!("Updated to version: {}", status.version());
    Ok(())
}
