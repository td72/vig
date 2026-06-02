use ratatui::layout::Constraint;

/// Parse a mini-constraint string into a ratatui Constraint.
///
/// Formats:
/// - `"40%"` → `Constraint::Percentage(40)`
/// - `"30"` → `Constraint::Length(30)`
/// - `"min:3"` → `Constraint::Min(3)`
pub fn parse_constraint(s: &str) -> Result<Constraint, String> {
    if let Some(rest) = s.strip_suffix('%') {
        let n: u16 = rest
            .parse()
            .map_err(|_| format!("Invalid percentage: {s:?}"))?;
        return Ok(Constraint::Percentage(n));
    }
    if let Some(rest) = s.strip_prefix("min:") {
        let n: u16 = rest
            .parse()
            .map_err(|_| format!("Invalid min constraint: {s:?}"))?;
        return Ok(Constraint::Min(n));
    }
    let n: u16 = s
        .parse()
        .map_err(|_| format!("Invalid length constraint: {s:?}"))?;
    Ok(Constraint::Length(n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage() {
        assert_eq!(parse_constraint("40%").unwrap(), Constraint::Percentage(40));
        assert_eq!(parse_constraint("0%").unwrap(), Constraint::Percentage(0));
        assert_eq!(parse_constraint("100%").unwrap(), Constraint::Percentage(100));
    }

    #[test]
    fn length() {
        assert_eq!(parse_constraint("30").unwrap(), Constraint::Length(30));
        assert_eq!(parse_constraint("1").unwrap(), Constraint::Length(1));
    }

    #[test]
    fn min() {
        assert_eq!(parse_constraint("min:3").unwrap(), Constraint::Min(3));
        assert_eq!(parse_constraint("min:20").unwrap(), Constraint::Min(20));
    }

    #[test]
    fn errors() {
        assert!(parse_constraint("").is_err());
        assert!(parse_constraint("abc").is_err());
        assert!(parse_constraint("min:").is_err());
        assert!(parse_constraint("abc%").is_err());
    }
}
