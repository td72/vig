pub mod preview;
pub mod stashes;
pub mod worktrees;

/// Fit `s` into `width` columns: pad with spaces on the right, or keep the
/// tail with a leading ellipsis when it is too long (paths are most
/// recognisable by their last components).
pub fn fit_tail(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        return format!("{s:<width$}");
    }
    if width == 0 {
        return String::new();
    }
    let tail: String = s.chars().skip(len - (width - 1)).collect();
    format!("…{tail}")
}

/// Truncate `s` to `width` columns with a trailing ellipsis when needed.
pub fn fit_head(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let head: String = s.chars().take(width - 1).collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_tail_pads_and_truncates() {
        assert_eq!(fit_tail("abc", 5), "abc  ");
        assert_eq!(fit_tail("abcdef", 4), "…def");
        assert_eq!(fit_tail("abc", 0), "");
    }

    #[test]
    fn fit_head_truncates() {
        assert_eq!(fit_head("abc", 5), "abc");
        assert_eq!(fit_head("abcdef", 4), "abc…");
    }
}
