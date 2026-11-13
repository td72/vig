//! `vig update`: replace the running binary with the latest release.
//!
//! The release is looked up through `gh` like every other GitHub access in
//! vig, so authentication stays inside gh and no API quota is spent
//! anonymously. Downloading, zipsign verification and the binary swap are
//! `self_update`'s default implementation of [`ReleaseUpdate`].

use self_update::cargo_crate_version;
use self_update::update::{Release, ReleaseAsset, ReleaseUpdate};
use self_update::{DEFAULT_PROGRESS_CHARS, DEFAULT_PROGRESS_TEMPLATE};
use serde::Deserialize;
use std::path::PathBuf;

use crate::github::domain::client::{is_rate_limited, run_gh};

const REPO: &str = "td72/vig";
const RELEASES_PAGE: &str = "https://github.com/td72/vig/releases";

const _: () = assert!(include_bytes!("../zipsign.pub").len() == 32);

pub fn run() -> anyhow::Result<()> {
    let updater = GhRelease {
        current_version: cargo_crate_version!().to_string(),
        bin_install_path: std::env::current_exe()?,
        verifying_keys: [*include_bytes!("../zipsign.pub")],
    };

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
            eprintln!("Update failed: {e}");
            Err(e.into())
        }
    }
}

/// A `ReleaseUpdate` backend that asks `gh` for the latest release.
struct GhRelease {
    current_version: String,
    bin_install_path: PathBuf,
    verifying_keys: [[u8; 32]; 1],
}

/// The `gh release view --json tagName,assets` payload.
#[derive(Deserialize)]
struct GhReleaseView {
    #[serde(rename = "tagName")]
    tag_name: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    url: String,
}

fn gh_release_args(tag: Option<&str>) -> Vec<&str> {
    let mut args = vec!["release", "view"];
    if let Some(tag) = tag {
        args.push(tag);
    }
    args.extend(["--repo", REPO, "--json", "tagName,assets"]);
    args
}

/// Turn the `gh release view` JSON into a `self_update` release.
fn release_from_gh(json: &[u8]) -> Result<Release, String> {
    let view: GhReleaseView =
        serde_json::from_slice(json).map_err(|e| format!("unexpected gh output: {e}"))?;
    Ok(Release {
        name: view.tag_name.clone(),
        version: view.tag_name.trim_start_matches('v').to_string(),
        date: String::new(),
        body: None,
        assets: view
            .assets
            .into_iter()
            .map(|a| ReleaseAsset {
                name: a.name,
                download_url: a.url,
            })
            .collect(),
    })
}

/// Explain a failed `gh` call in the user's terms.
fn explain_gh_error(msg: &str) -> String {
    if is_rate_limited(msg) {
        return "GitHub API rate limit exceeded. Please try again later.".to_string();
    }
    let lower = msg.to_lowercase();
    if lower.contains("not logged in") || lower.contains("gh auth login") {
        return format!(
            "gh is not signed in. Run `gh auth login`, or download a release from {RELEASES_PAGE}"
        );
    }
    // `run_gh` prefixes a spawn failure with its context, "gh not found".
    if lower.starts_with("gh not found") {
        return format!("gh (GitHub CLI) is not installed. Install it, or download a release from {RELEASES_PAGE}");
    }
    format!("gh release view failed: {msg}")
}

fn fetch_release(tag: Option<&str>) -> self_update::errors::Result<Release> {
    let stdout = run_gh(&gh_release_args(tag), "gh not found")
        .map_err(|e| self_update::errors::Error::Release(explain_gh_error(&e)))?;
    release_from_gh(&stdout).map_err(self_update::errors::Error::Release)
}

impl ReleaseUpdate for GhRelease {
    fn get_latest_release(&self) -> self_update::errors::Result<Release> {
        fetch_release(None)
    }

    /// Only the latest release, and only when it is newer than the running
    /// binary: an empty list means "up to date" to `self_update`.
    fn get_latest_releases(
        &self,
        current_version: &str,
    ) -> self_update::errors::Result<Vec<Release>> {
        let latest = self.get_latest_release()?;
        let newer = self_update::version::bump_is_greater(current_version, &latest.version)?;
        Ok(if newer { vec![latest] } else { Vec::new() })
    }

    fn get_release_version(&self, ver: &str) -> self_update::errors::Result<Release> {
        fetch_release(Some(ver))
    }

    fn current_version(&self) -> String {
        self.current_version.clone()
    }

    fn target(&self) -> String {
        self_update::get_target().to_string()
    }

    fn target_version(&self) -> Option<String> {
        None
    }

    fn identifier(&self) -> Option<String> {
        Some("vig-".to_string())
    }

    fn bin_name(&self) -> String {
        "vig".to_string()
    }

    fn bin_install_path(&self) -> PathBuf {
        self.bin_install_path.clone()
    }

    fn bin_path_in_archive(&self) -> String {
        "vig".to_string()
    }

    fn show_download_progress(&self) -> bool {
        true
    }

    fn show_output(&self) -> bool {
        true
    }

    fn no_confirm(&self) -> bool {
        false
    }

    fn progress_template(&self) -> String {
        DEFAULT_PROGRESS_TEMPLATE.to_string()
    }

    fn progress_chars(&self) -> String {
        DEFAULT_PROGRESS_CHARS.to_string()
    }

    /// The asset download is a plain public URL: no token, ever.
    fn auth_token(&self) -> Option<String> {
        None
    }

    fn verifying_keys(&self) -> &[[u8; 32]] {
        &self.verifying_keys
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIEW: &str = r#"{
        "tagName": "v0.19.0",
        "assets": [
            {"name": "vig-aarch64-apple-darwin.tar.gz", "url": "https://github.com/td72/vig/releases/download/v0.19.0/vig-aarch64-apple-darwin.tar.gz"},
            {"name": "vig-aarch64-apple-darwin.tar.gz.sha256", "url": "https://github.com/td72/vig/releases/download/v0.19.0/vig-aarch64-apple-darwin.tar.gz.sha256"},
            {"name": "vig-x86_64-unknown-linux-gnu.tar.gz", "url": "https://github.com/td72/vig/releases/download/v0.19.0/vig-x86_64-unknown-linux-gnu.tar.gz"}
        ]
    }"#;

    #[test]
    fn the_gh_view_becomes_a_release_with_its_assets() {
        let release = release_from_gh(VIEW.as_bytes()).unwrap();
        assert_eq!(release.version, "0.19.0");
        assert_eq!(release.name, "v0.19.0");
        assert_eq!(release.assets.len(), 3);
        let asset = release
            .asset_for("aarch64-apple-darwin", Some("vig-"))
            .unwrap();
        assert_eq!(asset.name, "vig-aarch64-apple-darwin.tar.gz");
        assert!(asset
            .download_url
            .ends_with("/v0.19.0/vig-aarch64-apple-darwin.tar.gz"));
    }

    #[test]
    fn the_asset_lookup_picks_the_archive_not_its_checksum() {
        let release = release_from_gh(VIEW.as_bytes()).unwrap();
        let asset = release
            .asset_for("aarch64-apple-darwin", Some("vig-"))
            .unwrap();
        assert!(!asset.name.ends_with(".sha256"));
    }

    #[test]
    fn garbage_from_gh_is_reported_not_panicked() {
        let err = release_from_gh(b"not json").unwrap_err();
        assert!(err.contains("unexpected gh output"), "{err}");
    }

    #[test]
    fn gh_errors_are_explained() {
        assert!(
            explain_gh_error("HTTP 403: API rate limit exceeded for 1.2.3.4")
                .contains("rate limit")
        );
        assert!(
            explain_gh_error("To get started with GitHub CLI, please run:  gh auth login")
                .contains("gh auth login")
        );
        assert!(
            explain_gh_error("gh not found: No such file or directory (os error 2)")
                .contains("not installed")
        );
        assert!(explain_gh_error("release not found").starts_with("gh release view failed"));
    }

    #[test]
    fn the_gh_arguments_name_the_repo_and_fields() {
        assert_eq!(
            gh_release_args(None),
            [
                "release",
                "view",
                "--repo",
                "td72/vig",
                "--json",
                "tagName,assets"
            ]
        );
        assert_eq!(gh_release_args(Some("v0.18.0"))[2], "v0.18.0");
    }
}
