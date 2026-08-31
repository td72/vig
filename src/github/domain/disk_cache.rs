use crate::github::domain::types::*;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

const CACHE_VERSION: &str = "v1";

/// Extract "owner/repo" from the git remote URL (local operation, no network).
use crate::github::domain::remote::parse_github_nwo;

fn repo_nwo() -> Option<&'static str> {
    static NWO: OnceLock<Option<String>> = OnceLock::new();
    NWO.get_or_init(|| {
        let repo = git2::Repository::open_from_env().ok()?;
        let remote = repo.find_remote("origin").ok()?;
        let url = remote.url()?;
        parse_github_nwo(url)
    })
    .as_deref()
}

/// Build the cache directory path: `<cache_dir>/vig/<version>/<owner>/<repo>/`
pub(crate) fn cache_dir() -> Option<PathBuf> {
    let base = dirs::cache_dir()?;
    let nwo = repo_nwo()?;
    Some(base.join("vig").join(CACHE_VERSION).join(nwo))
}

pub(crate) fn load_json<T: serde::de::DeserializeOwned>(path: &PathBuf) -> Option<T> {
    let data = fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

pub(crate) fn save_json<T: serde::Serialize>(path: &PathBuf, value: &T) {
    let Some(parent) = path.parent() else {
        return;
    };
    if fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(data) = serde_json::to_vec(value) else {
        return;
    };
    // Write to a temp file then rename for atomicity
    let tmp = path.with_extension("tmp");
    let Ok(mut f) = fs::File::create(&tmp) else {
        return;
    };
    if f.write_all(&data).is_err() {
        let _ = fs::remove_file(&tmp);
        return;
    }
    let _ = fs::rename(&tmp, path);
}

pub fn load_issue_list() -> Option<Vec<GhIssueListItem>> {
    load_json(&cache_dir()?.join("issues.json"))
}

pub fn save_issue_list(issues: &[GhIssueListItem]) {
    if let Some(dir) = cache_dir() {
        save_json(&dir.join("issues.json"), &issues);
    }
}

pub fn load_pr_list() -> Option<Vec<GhPrListItem>> {
    load_json(&cache_dir()?.join("prs.json"))
}

pub fn save_pr_list(prs: &[GhPrListItem]) {
    if let Some(dir) = cache_dir() {
        save_json(&dir.join("prs.json"), &prs);
    }
}

pub fn load_issue_detail(number: u64) -> Option<GhIssueDetail> {
    load_json(&cache_dir()?.join("issue").join(format!("{number}.json")))
}

pub fn save_issue_detail(detail: &GhIssueDetail) {
    if let Some(dir) = cache_dir() {
        save_json(
            &dir.join("issue").join(format!("{}.json", detail.number)),
            detail,
        );
    }
}

pub fn load_pr_detail(number: u64) -> Option<GhPrDetail> {
    load_json(&cache_dir()?.join("pr").join(format!("{number}.json")))
}

pub fn save_pr_detail(detail: &GhPrDetail) {
    if let Some(dir) = cache_dir() {
        save_json(
            &dir.join("pr").join(format!("{}.json", detail.number)),
            detail,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_github_nwo_ssh_with_git_suffix() {
        assert_eq!(
            parse_github_nwo("git@github.com:td72/vig.git"),
            Some("td72/vig".to_string())
        );
    }

    #[test]
    fn test_parse_github_nwo_ssh_without_git_suffix() {
        assert_eq!(
            parse_github_nwo("git@github.com:td72/vig"),
            Some("td72/vig".to_string())
        );
    }

    #[test]
    fn test_parse_github_nwo_https_with_git_suffix() {
        assert_eq!(
            parse_github_nwo("https://github.com/td72/vig.git"),
            Some("td72/vig".to_string())
        );
    }

    #[test]
    fn test_parse_github_nwo_https_without_git_suffix() {
        assert_eq!(
            parse_github_nwo("https://github.com/td72/vig"),
            Some("td72/vig".to_string())
        );
    }

    #[test]
    fn test_parse_github_nwo_invalid_url() {
        assert_eq!(parse_github_nwo("https://gitlab.com/foo/bar"), None);
        assert_eq!(parse_github_nwo("https://notgithub.com/foo/bar"), None);
        assert_eq!(parse_github_nwo("not-a-url"), None);
    }

    #[test]
    fn test_parse_github_nwo_no_repo() {
        assert_eq!(parse_github_nwo("git@github.com:td72"), None);
        assert_eq!(parse_github_nwo("https://github.com/td72"), None);
    }

    #[test]
    fn test_repo_nwo_resolves() {
        let nwo = repo_nwo();
        assert!(nwo.is_some(), "repo_nwo() returned None");
        let nwo = nwo.unwrap();
        // Verify "owner/repo" format without hardcoding specific values
        let parts: Vec<&str> = nwo.split('/').collect();
        assert_eq!(parts.len(), 2, "expected owner/repo format, got: {nwo}");
        assert!(!parts[0].is_empty(), "owner is empty");
        assert!(!parts[1].is_empty(), "repo is empty");
    }

    #[test]
    fn test_cache_dir_resolves() {
        let dir = cache_dir();
        assert!(dir.is_some(), "cache_dir() returned None");
        let dir = dir.unwrap();
        // Verify path contains versioned cache structure
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.contains(&format!("vig/{CACHE_VERSION}/")),
            "unexpected cache dir: {}",
            dir.display()
        );
    }

    #[test]
    fn test_save_and_load_json_roundtrip() {
        let tmp = std::env::temp_dir().join("vig_test_cache");
        let _ = fs::remove_dir_all(&tmp);

        let issues = vec![GhIssueListItem {
            number: 99999,
            title: "Test issue".to_string(),
            state: "OPEN".to_string(),
            author: None,
            labels: vec![],
            created_at: "2025-01-01T00:00:00Z".to_string(),
            parent: None,
        }];

        let path = tmp.join("issues.json");
        save_json(&path, &issues);
        let loaded: Option<Vec<GhIssueListItem>> = load_json(&path);
        assert!(loaded.is_some(), "load_json returned None");
        assert_eq!(loaded.unwrap()[0].number, 99999);

        let detail = GhIssueDetail {
            number: 88888,
            title: "Test detail".to_string(),
            state: "OPEN".to_string(),
            author: None,
            body: "body".to_string(),
            comments: vec![],
            labels: vec![],
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };

        let path = tmp.join("issue/88888.json");
        save_json(&path, &detail);
        let loaded: Option<GhIssueDetail> = load_json(&path);
        assert!(loaded.is_some(), "load_json returned None");
        assert_eq!(loaded.unwrap().number, 88888);

        let _ = fs::remove_dir_all(&tmp);
    }
}
