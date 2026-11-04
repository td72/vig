//! Width-aware text helpers shared by the pages.

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Cut `s` to at most `width` columns, ending in `…` when cut.
pub fn truncate_to_width(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut w = 0;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if w + cw > width.saturating_sub(1) {
            break;
        }
        out.push(c);
        w += cw;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_to_width_cuts_on_columns() {
        assert_eq!(truncate_to_width("日本語テキスト", 7), "日本語…");
        assert_eq!(truncate_to_width("short", 10), "short");
        assert_eq!(truncate_to_width("x", 0), "");
    }
}
