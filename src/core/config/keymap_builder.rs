use crate::core::keymap::{KeyInput, Keymap};
use std::str::FromStr;

/// A single entry in a pane's `keys { ... }` block.
#[derive(Debug, Clone)]
pub enum KeymapEntry {
    /// A named preset to expand (e.g. `preset "nav"`).
    Preset(String),
    /// An explicit key-to-action binding (e.g. `"Enter" "ExpandOrOpen"`).
    Binding { key: String, action: String },
}

/// Expand a preset name into `(key_string, action_string)` pairs.
fn expand_preset(name: &str) -> Result<Vec<(&'static str, &'static str)>, String> {
    match name {
        "nav" => Ok(vec![
            ("j", "Nav.MoveDown"),
            ("Down", "Nav.MoveDown"),
            ("k", "Nav.MoveUp"),
            ("Up", "Nav.MoveUp"),
            ("Ctrl+d", "Nav.HalfPageDown"),
            ("Ctrl+u", "Nav.HalfPageUp"),
            ("g", "Nav.JumpTop"),
            ("G", "Nav.JumpBottom"),
        ]),
        "search" => Ok(vec![
            ("/", "Search.Start"),
            ("n", "Search.Next"),
            ("N", "Search.Prev"),
        ]),
        _ => Err(format!("Unknown preset: {name:?}")),
    }
}

/// Build a typed `Keymap<A>` from a slice of `KeymapEntry` values.
///
/// Presets are expanded first; explicit bindings in the same block override
/// preset bindings for the same key (preset先展開→明示バインド後勝ち上書き).
pub fn build_keymap<A>(entries: &[KeymapEntry]) -> Result<Keymap<A>, String>
where
    A: Clone + FromStr<Err = String>,
{
    // Collect preset bindings first, then explicit bindings.
    // Because Keymap::bind always overwrites the action for an existing key,
    // applying explicit bindings after preset bindings gives explicit ones priority.
    let mut preset_pairs: Vec<(String, String)> = Vec::new();
    let mut explicit_pairs: Vec<(String, String)> = Vec::new();

    for entry in entries {
        match entry {
            KeymapEntry::Preset(name) => {
                for (key, action) in expand_preset(name)? {
                    preset_pairs.push((key.to_string(), action.to_string()));
                }
            }
            KeymapEntry::Binding { key, action } => {
                explicit_pairs.push((key.clone(), action.clone()));
            }
        }
    }

    let mut km = Keymap::new();

    for (key_str, action_str) in preset_pairs.iter().chain(explicit_pairs.iter()) {
        let ki: KeyInput = key_str
            .parse()
            .map_err(|e| format!("Invalid key {key_str:?}: {e}"))?;
        let action: A = action_str
            .parse()
            .map_err(|e| format!("Invalid action {action_str:?}: {e}"))?;
        km = km.bind(ki.code, ki.modifiers, action);
    }

    Ok(km)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::keymap::{NavAction, SearchAction};
    use crate::git::panes::file_tree::FileTreeAction;
    use crossterm::event::KeyEvent;

    fn key_event(s: &str) -> KeyEvent {
        let ki: KeyInput = s.parse().unwrap();
        KeyEvent::new(ki.code, ki.modifiers)
    }

    #[test]
    fn preset_nav_expands() {
        let entries = vec![KeymapEntry::Preset("nav".into())];
        let km: Keymap<FileTreeAction> = build_keymap(&entries).unwrap();
        assert!(matches!(
            km.lookup(key_event("j")),
            Some(FileTreeAction::Nav(NavAction::MoveDown))
        ));
        assert!(matches!(
            km.lookup(key_event("G")),
            Some(FileTreeAction::Nav(NavAction::JumpBottom))
        ));
    }

    #[test]
    fn explicit_overrides_preset() {
        // If an explicit binding overrides a preset binding for the same key,
        // the explicit one wins.
        let entries = vec![
            KeymapEntry::Preset("nav".into()),
            // Override 'j' (preset: MoveDown) with a different action
            KeymapEntry::Binding {
                key: "j".into(),
                action: "ToggleDir".into(),
            },
        ];
        let km: Keymap<FileTreeAction> = build_keymap(&entries).unwrap();
        assert!(matches!(
            km.lookup(key_event("j")),
            Some(FileTreeAction::ToggleDir)
        ));
    }

    #[test]
    fn preset_search_expands() {
        let entries = vec![KeymapEntry::Preset("search".into())];
        let km: Keymap<FileTreeAction> = build_keymap(&entries).unwrap();
        assert!(matches!(
            km.lookup(key_event("/")),
            Some(FileTreeAction::Search(SearchAction::Start))
        ));
        assert!(matches!(
            km.lookup(key_event("n")),
            Some(FileTreeAction::Search(SearchAction::Next))
        ));
        assert!(matches!(
            km.lookup(key_event("N")),
            Some(FileTreeAction::Search(SearchAction::Prev))
        ));
    }

    #[test]
    fn unknown_preset_fails() {
        let entries = vec![KeymapEntry::Preset("unknown_preset".into())];
        let result: Result<Keymap<FileTreeAction>, _> = build_keymap(&entries);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_key_fails() {
        let entries = vec![KeymapEntry::Binding {
            key: "NotAKey".into(),
            action: "ToggleDir".into(),
        }];
        let result: Result<Keymap<FileTreeAction>, _> = build_keymap(&entries);
        assert!(result.is_err());
    }

    #[test]
    fn invalid_action_fails() {
        let entries = vec![KeymapEntry::Binding {
            key: "j".into(),
            action: "NoSuchAction".into(),
        }];
        let result: Result<Keymap<FileTreeAction>, _> = build_keymap(&entries);
        assert!(result.is_err());
    }
}
