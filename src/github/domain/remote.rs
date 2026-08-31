//! Parsing of the `origin` remote URL — used to derive `owner/repo` without
//! an API request (browser URLs, cache keys).

/// Parse `owner/repo` out of a github.com remote URL. Accepted forms, each
/// with an optional `.git` and/or trailing `/`:
/// `git@github.com:owner/repo`, `https://github.com/owner/repo`,
/// `http://github.com/owner/repo`, `ssh://git@github.com/owner/repo`,
/// `ssh://git@github.com:22/owner/repo`.
pub(crate) fn parse_github_nwo(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = if let Some(r) = url.strip_prefix("git@github.com:") {
        r
    } else if let Some(r) = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
    {
        r
    } else if let Some(r) = url.strip_prefix("ssh://git@github.com") {
        // `/owner/repo` or `:<port>/owner/repo`
        match r.strip_prefix('/') {
            Some(r) => r,
            None => {
                let r = r.strip_prefix(':')?;
                let (port, rest) = r.split_once('/')?;
                if port.is_empty() || !port.bytes().all(|b| b.is_ascii_digit()) {
                    return None;
                }
                rest
            }
        }
    } else {
        return None;
    };
    // Trim the trailing slash first so `owner/repo.git/` parses too.
    let rest = rest.trim_end_matches('/');
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.splitn(2, '/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty() && !s.contains('/'))?;
    Some(format!("{owner}/{repo}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_common_github_remote_forms() {
        for url in [
            "git@github.com:td72/vig",
            "git@github.com:td72/vig.git",
            "https://github.com/td72/vig",
            "https://github.com/td72/vig.git",
            "https://github.com/td72/vig/",
            "https://github.com/td72/vig.git/",
            "http://github.com/td72/vig",
            "ssh://git@github.com/td72/vig.git",
            "ssh://git@github.com:22/td72/vig.git",
            "  git@github.com:td72/vig \n",
        ] {
            assert_eq!(parse_github_nwo(url).as_deref(), Some("td72/vig"), "{url}");
        }
    }

    #[test]
    fn rejects_non_github_or_malformed_urls() {
        for url in [
            "git@gitlab.com:td72/vig.git",
            "https://github.enterprise.example/td72/vig",
            "git@github.com:onlyowner",
            "https://github.com/",
            "https://github.com/td72/",
            "https://github.com/td72/vig/extra",
            "ssh://git@github.com:notaport/td72/vig",
            "ssh://git@github.com:/td72/vig",
        ] {
            assert_eq!(parse_github_nwo(url), None, "{url}");
        }
    }
}
