//! The signed-in GitHub login — "me" for `assignee:@me` filters and for
//! telling the items aimed at the viewer apart — resolved without an API
//! request wherever possible.

use super::client::run_gh;
use std::collections::HashMap;
use std::path::PathBuf;

/// The host the login is looked up for (vig reads github.com remotes only).
const HOST: &str = "github.com";

/// The login, in this order: `configured` (`github-login` in the config);
/// `gh config get -h github.com user`, the account `gh auth login` stored
/// locally (no request, and it follows `gh auth switch`); the login a
/// previous `gh api user` cached on disk; `gh api user` itself, cached for
/// the next start. `None` when none of them knows.
pub fn resolve(configured: Option<&str>) -> Option<String> {
    if let Some(login) = configured.map(str::trim).filter(|l| !l.is_empty()) {
        return Some(login.to_string());
    }
    from_gh_config().or_else(|| {
        load_cached().or_else(|| {
            let login = from_api()?;
            save_cached(&login);
            Some(login)
        })
    })
}

/// `gh config get -h github.com user`: the signed-in login as `gh` stores
/// it, read from its hosts file — no request.
pub fn from_gh_config() -> Option<String> {
    non_empty(
        run_gh(
            &["config", "get", "-h", HOST, "user"],
            "gh config get failed",
        )
        .ok()?,
    )
}

/// `gh api user` (REST, no GraphQL points): the last resort.
pub fn from_api() -> Option<String> {
    non_empty(run_gh(&["api", "user", "--jq", ".login"], "gh api user failed").ok()?)
}

fn non_empty(out: Vec<u8>) -> Option<String> {
    let login = String::from_utf8_lossy(&out).trim().to_string();
    (!login.is_empty()).then_some(login)
}

/// `<cache_dir>/vig/viewer.json`: `{ "<host>": "<login>" }`. Not under the
/// per-repository cache — a login belongs to the host.
fn cache_path() -> Option<PathBuf> {
    // Replayed fixtures must not be mixed with (or leak into) real caches.
    if crate::core::gh_fixture::active() {
        return None;
    }
    Some(dirs::cache_dir()?.join("vig").join("viewer.json"))
}

fn load_cached() -> Option<String> {
    let data = std::fs::read(cache_path()?).ok()?;
    let map: HashMap<String, String> = serde_json::from_slice(&data).ok()?;
    map.get(HOST).cloned().filter(|l| !l.is_empty())
}

fn save_cached(login: &str) {
    let Some(path) = cache_path() else {
        return;
    };
    let mut map: HashMap<String, String> = std::fs::read(&path)
        .ok()
        .and_then(|d| serde_json::from_slice(&d).ok())
        .unwrap_or_default();
    map.insert(HOST.to_string(), login.to_string());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_vec(&map) {
        let _ = std::fs::write(&path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_login_is_used_as_is() {
        assert_eq!(resolve(Some(" alice ")).as_deref(), Some("alice"));
    }

    /// Replay is process-global: run in a child process.
    #[test]
    fn gh_config_first_then_the_api() {
        crate::core::gh_fixture::run_isolated(
            "github::domain::viewer::tests::gh_config_first_then_the_api_isolated",
        );
    }

    #[test]
    #[ignore = "run through gh_config_first_then_the_api (own process)"]
    fn gh_config_first_then_the_api_isolated() {
        use crate::core::gh_fixture;
        let dir = std::env::temp_dir().join(format!("vig-viewer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let config = dir.join(gh_fixture::name_for(&["config", "get", "-h", HOST, "user"]));
        let api = dir.join(gh_fixture::name_for(&["api", "user", "--jq", ".login"]));
        gh_fixture::set_replay_dir(Some(dir.clone()));

        // Nothing recorded: unknown.
        assert_eq!(resolve(None), None);
        // `gh api user` alone: the fallback (no disk cache under replay).
        std::fs::write(&api, "bob\n").unwrap();
        assert_eq!(resolve(None).as_deref(), Some("bob"));
        // `gh config get` wins once it knows.
        std::fs::write(&config, "alice\n").unwrap();
        assert_eq!(resolve(None).as_deref(), Some("alice"));
        // An empty `user` entry is no answer.
        std::fs::write(&config, "\n").unwrap();
        assert_eq!(resolve(None).as_deref(), Some("bob"));
        // The config still beats everything.
        assert_eq!(resolve(Some("carol")).as_deref(), Some("carol"));

        gh_fixture::set_replay_dir(None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
