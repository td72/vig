//! The filter expression of a saved view (`ProjectV2View.filter`), parsed
//! and evaluated locally against the items already fetched — no API call.
//!
//! Supported: free-text title words, `field:value` (with `,` lists and
//! quoted values), `-` negation, `is:issue|pr|draft`, `is:open|closed|merged`
//! (an item without a known state counts as open), `no:<field>` /
//! `has:<field>`, `assignee:` (`@me` resolves to the signed-in login),
//! `label:`, `milestone:`, `repo:`. Anything else — ranges (`>`, `<`, `..`),
//! wildcards — is
//! reported in [`Filter::unsupported`] and ignored.

use crate::projects::domain::types::{item_key, ItemKind, ItemState, ProjectItem};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Term {
    /// A bare word: the title contains it (case-insensitive).
    Text(String),
    /// `is:issue` / `is:pr` / `is:draft`.
    Is(ItemKind),
    /// `is:open` / `is:closed` (closed or merged) / `is:merged`.
    State(ItemState),
    /// `no:<field>` (`true`) / `has:<field>` (`false`): the field is empty / set.
    Empty {
        key: String,
        empty: bool,
    },
    /// `assignee:` — logins; `@me` is already resolved when known.
    Assignee(Vec<String>),
    /// `assignee:@me` while the signed-in login is unknown: never matches.
    AssigneeMe,
    Label(Vec<String>),
    Milestone(Vec<String>),
    Repo(Vec<String>),
    /// `<field>:value,value` on any project field (`status`, custom ones).
    Field {
        key: String,
        values: Vec<String>,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    /// `(negated, term)` — all must hold.
    pub terms: Vec<(bool, Term)>,
    /// Tokens vig cannot evaluate, verbatim.
    pub unsupported: Vec<String>,
}

/// Split on whitespace, keeping double-quoted stretches together (the
/// quotes are dropped; `\"` is not a thing in GitHub's syntax).
fn tokenize(expr: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in expr.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A `value,value` list: split on commas outside quotes (already stripped
/// by `tokenize`, so a plain split), lowercased, empties dropped.
fn values(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .collect()
}

/// Ranges, comparisons and wildcards are not evaluated.
fn is_unsupported_value(raw: &str) -> bool {
    raw.starts_with(['>', '<'])
        || raw.contains("..")
        || raw.contains('*')
        || raw.starts_with("@today")
}

/// Parse a view filter. `viewer` is the signed-in login for `@me`.
pub fn parse(expr: &str, viewer: Option<&str>) -> Filter {
    let mut filter = Filter::default();
    for token in tokenize(expr) {
        let (negated, body) = match token.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, token.as_str()),
        };
        if body.is_empty() {
            continue;
        }
        let Some((key, raw)) = body.split_once(':') else {
            filter
                .terms
                .push((negated, Term::Text(body.to_lowercase())));
            continue;
        };
        let key_lc = key.to_lowercase();
        if is_unsupported_value(raw) {
            filter.unsupported.push(token.clone());
            continue;
        }
        let term = match key_lc.as_str() {
            "is" | "type" => match raw.to_lowercase().as_str() {
                "issue" => Term::Is(ItemKind::Issue),
                "pr" | "pull-request" | "pullrequest" => Term::Is(ItemKind::PullRequest),
                "draft" | "draft-issue" => Term::Is(ItemKind::Draft),
                "open" => Term::State(ItemState::Open),
                "closed" => Term::State(ItemState::Closed),
                "merged" => Term::State(ItemState::Merged),
                _ => {
                    filter.unsupported.push(token.clone());
                    continue;
                }
            },
            "no" | "has" => Term::Empty {
                key: item_key(raw),
                empty: key_lc == "no",
            },
            "assignee" | "assignees" => {
                let mut logins = values(raw);
                if logins.iter().any(|l| l == "@me") {
                    match viewer {
                        Some(me) => {
                            for l in &mut logins {
                                if l == "@me" {
                                    *l = me.to_lowercase();
                                }
                            }
                        }
                        None => {
                            filter.terms.push((negated, Term::AssigneeMe));
                            continue;
                        }
                    }
                }
                Term::Assignee(logins)
            }
            "label" | "labels" => Term::Label(values(raw)),
            "milestone" => Term::Milestone(values(raw)),
            "repo" | "repository" => Term::Repo(values(raw)),
            "created"
            | "updated"
            | "closed"
            | "merged"
            | "sort"
            | "reviewer"
            | "review"
            | "linked-pull-requests"
            | "reason"
            | "parent-issue"
            | "sub-issues-progress" => {
                filter.unsupported.push(token.clone());
                continue;
            }
            _ => Term::Field {
                key: item_key(key),
                values: values(raw),
            },
        };
        filter.terms.push((negated, term));
    }
    filter
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Whether `item` passes every term.
    pub fn matches(&self, item: &ProjectItem) -> bool {
        self.terms
            .iter()
            .all(|(negated, term)| term_matches(term, item) != *negated)
    }
}

/// The item's values for a key: an array of strings, a single string, or
/// an object's `title` (milestones, iterations) — lowercased.
fn list_values(item: &ProjectItem, key: &str) -> Vec<String> {
    use serde_json::Value;
    match item.fields.get(key) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| match v {
                Value::String(s) => Some(s.to_lowercase()),
                Value::Object(o) => o
                    .get("title")
                    .or_else(|| o.get("name"))
                    .or_else(|| o.get("login"))
                    .and_then(|t| t.as_str())
                    .map(str::to_lowercase),
                _ => None,
            })
            .collect(),
        Some(_) => item
            .field_text(key)
            .filter(|s| !s.is_empty())
            .map(|s| vec![s.to_lowercase()])
            .unwrap_or_default(),
        None => item
            .field_text(key)
            .filter(|s| !s.is_empty())
            .map(|s| vec![s.to_lowercase()])
            .unwrap_or_default(),
    }
}

fn any_of(have: &[String], want: &[String]) -> bool {
    want.iter().any(|w| have.iter().any(|h| h == w))
}

fn term_matches(term: &Term, item: &ProjectItem) -> bool {
    match term {
        Term::Text(word) => item.title().to_lowercase().contains(word.as_str()),
        Term::Is(kind) => item.kind() == *kind,
        // GitHub counts a merged PR as closed too.
        Term::State(ItemState::Closed) => item.state() != ItemState::Open,
        Term::State(state) => item.state() == *state,
        Term::Empty { key, empty } => list_values(item, key).is_empty() == *empty,
        Term::Assignee(logins) => any_of(
            &item
                .assignees()
                .iter()
                .map(|a| a.to_lowercase())
                .collect::<Vec<_>>(),
            logins,
        ),
        Term::AssigneeMe => false,
        Term::Label(labels) => any_of(&list_values(item, "labels"), labels),
        Term::Milestone(ms) => any_of(&list_values(item, "milestone"), ms),
        Term::Repo(repos) => {
            let Some(full) = item.repository().map(str::to_lowercase) else {
                return false;
            };
            let short = full.rsplit('/').next().unwrap_or("").to_string();
            repos.iter().any(|r| *r == full || *r == short)
        }
        Term::Field { key, values } => any_of(&list_values(item, key), values),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::domain::types::tests::board;

    fn ids(expr: &str) -> Vec<String> {
        let b = board();
        let f = parse(expr, Some("td72"));
        b.items
            .iter()
            .filter(|i| f.matches(i))
            .map(|i| i.id.clone())
            .collect()
    }

    #[test]
    fn tokenizer_keeps_quoted_values_together() {
        assert_eq!(
            tokenize(r#"status:"In Progress" -label:bug  foo"#),
            vec!["status:In Progress", "-label:bug", "foo"]
        );
    }

    #[test]
    fn field_values_lists_and_negation() {
        // Two items are Done in the fixture (I1, and none other?) — check
        // by value rather than by count.
        let done = ids("status:Done");
        assert!(!done.is_empty());
        assert!(done.iter().all(|id| board()
            .items
            .iter()
            .find(|i| &i.id == id)
            .unwrap()
            .status
            .as_deref()
            == Some("Done")));
        let not_done = ids("-status:Done");
        assert!(not_done.iter().all(|id| !done.contains(id)));
        assert_eq!(done.len() + not_done.len(), board().items.len());
        // Comma list is OR, case-insensitive, quotes allowed.
        let two = ids(r#"status:done,"in progress""#);
        assert!(two.len() > done.len());
    }

    #[test]
    fn is_no_has_label_assignee_repo_text() {
        assert!(ids("is:draft").iter().all(|id| id == "I3"));
        assert!(ids("is:pr").contains(&"I2".to_string()));
        assert!(ids("no:status").contains(&"I2".to_string()) || !ids("no:status").is_empty());
        assert_eq!(ids("has:priority").len(), ids("-no:priority").len());
        assert!(ids("label:enhancement").contains(&"I1".to_string()));
        assert!(ids("assignee:@me").contains(&"I1".to_string()));
        assert!(ids("assignee:TD72").contains(&"I1".to_string()));
        assert!(ids("repo:vig").contains(&"I1".to_string()));
        assert!(ids("repo:td72/vig").contains(&"I1".to_string()));
        assert_eq!(ids("projects view"), ids("Projects View"));
        assert!(ids("nothing-like-this").is_empty());
    }

    #[test]
    fn unsupported_tokens_are_reported_not_applied() {
        let f = parse(
            "status:Todo updated:>2026-01-01 is:archived estimate:1..3 label:x*",
            None,
        );
        assert_eq!(
            f.unsupported,
            vec![
                "updated:>2026-01-01",
                "is:archived",
                "estimate:1..3",
                "label:x*"
            ]
        );
        assert_eq!(f.terms.len(), 1);
        // @me without a known login is kept as a never-matching term.
        let f = parse("assignee:@me", None);
        assert_eq!(f.terms, vec![(false, Term::AssigneeMe)]);
        assert!(!f.matches(&board().items[0]));
    }

    /// `is:open` / `is:closed` / `is:merged` read the content state; an
    /// item without one (a draft, the CLI path) counts as open, and a
    /// merged PR is closed as well as merged.
    #[test]
    fn state_terms_follow_the_content_state() {
        let mut b = board();
        let set = |item: &mut crate::projects::domain::types::ProjectItem, s: &str| {
            item.content.as_mut().unwrap().state = Some(s.into());
        };
        set(&mut b.items[0], "CLOSED");
        set(&mut b.items[1], "MERGED");
        set(&mut b.items[2], "OPEN");
        // b.items[3..] keep no state.
        let ids = |expr: &str| -> Vec<String> {
            let f = parse(expr, None);
            assert!(f.unsupported.is_empty(), "{expr}: {:?}", f.unsupported);
            b.items
                .iter()
                .filter(|i| f.matches(i))
                .map(|i| i.id.clone())
                .collect()
        };
        assert_eq!(ids("is:closed"), vec!["I1", "I2"]);
        assert_eq!(ids("is:merged"), vec!["I2"]);
        assert!(!ids("is:open").contains(&"I1".to_string()));
        assert!(!ids("is:open").contains(&"I2".to_string()));
        assert!(ids("is:open").contains(&"I3".to_string()));
        assert_eq!(ids("is:open").len(), b.items.len() - 2);
        assert_eq!(ids("-is:closed"), ids("is:open"));
    }
}
