use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::fmt;

/// Normalized key input for use as HashMap key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl From<KeyEvent> for KeyInput {
    fn from(key: KeyEvent) -> Self {
        // Strip SHIFT for uppercase letters (crossterm sends SHIFT+Char('G'))
        let mods = if matches!(key.code, KeyCode::Char('A'..='Z')) {
            key.modifiers & !KeyModifiers::SHIFT
        } else {
            key.modifiers
        };
        Self {
            code: key.code,
            modifiers: mods,
        }
    }
}

impl fmt::Display for KeyInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let key = match self.code {
            KeyCode::Char(c) => c.to_string(),
            KeyCode::Enter => "Enter".to_string(),
            KeyCode::Esc => "Esc".to_string(),
            KeyCode::Tab => "Tab".to_string(),
            KeyCode::BackTab => "S-Tab".to_string(),
            KeyCode::Up => "↑".to_string(),
            KeyCode::Down => "↓".to_string(),
            KeyCode::Left => "←".to_string(),
            KeyCode::Right => "→".to_string(),
            _ => format!("{:?}", self.code),
        };
        if self.modifiers.contains(KeyModifiers::CONTROL) {
            write!(f, "Ctrl+{key}")
        } else {
            write!(f, "{key}")
        }
    }
}

/// Trait for action enums that can provide help descriptions.
/// Return `None` to hide the action from help.
pub trait ActionHelp {
    fn label(&self) -> Option<&'static str>;
}

/// A single help entry: key display string + description.
pub type HelpEntry = (String, String);

/// Key-to-action mapping.
pub struct Keymap<A: Clone> {
    bindings: HashMap<KeyInput, A>,
    order: Vec<KeyInput>,
}

impl<A: Clone> Keymap<A> {
    pub fn new() -> Self {
        Self {
            bindings: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Bind a key with modifiers to an action (builder pattern).
    pub fn bind(mut self, code: KeyCode, mods: KeyModifiers, action: A) -> Self {
        let ki = KeyInput {
            code,
            modifiers: mods,
        };
        if !self.bindings.contains_key(&ki) {
            self.order.push(ki);
        }
        self.bindings.insert(ki, action);
        self
    }

    /// Bind a plain key (no modifiers) to an action.
    pub fn key(self, code: KeyCode, action: A) -> Self {
        self.bind(code, KeyModifiers::NONE, action)
    }

    /// Bind Ctrl+c to an action.
    pub fn ctrl(self, c: char, action: A) -> Self {
        self.bind(KeyCode::Char(c), KeyModifiers::CONTROL, action)
    }

    /// Add multiple bindings at once.
    pub fn bindings(mut self, entries: Vec<(KeyCode, KeyModifiers, A)>) -> Self {
        for (code, mods, action) in entries {
            let ki = KeyInput {
                code,
                modifiers: mods,
            };
            if !self.bindings.contains_key(&ki) {
                self.order.push(ki);
            }
            self.bindings.insert(ki, action);
        }
        self
    }

    /// Look up an action for a key event.
    pub fn lookup(&self, key: KeyEvent) -> Option<&A> {
        self.bindings.get(&KeyInput::from(key))
    }
}

impl<A: Clone + ActionHelp> Keymap<A> {
    /// Generate help entries by grouping keys that map to the same label.
    /// Keys are joined with " / " (e.g., "j / ↓" → "Next item").
    /// Actions returning `None` from `label()` are excluded.
    pub fn help_entries(&self) -> Vec<HelpEntry> {
        let mut groups: Vec<(&'static str, Vec<&KeyInput>)> = Vec::new();

        for ki in &self.order {
            if let Some(action) = self.bindings.get(ki) {
                if let Some(label) = action.label() {
                    if let Some(group) = groups.iter_mut().find(|(l, _)| *l == label) {
                        group.1.push(ki);
                    } else {
                        groups.push((label, vec![ki]));
                    }
                }
            }
        }

        groups
            .into_iter()
            .map(|(label, keys)| {
                let key_str = keys
                    .iter()
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>()
                    .join(" / ");
                (key_str, label.to_string())
            })
            .collect()
    }
}

/// Common navigation actions shared across list panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavAction {
    MoveDown,
    MoveUp,
    HalfPageDown,
    HalfPageUp,
    JumpTop,
    JumpBottom,
}

impl ActionHelp for NavAction {
    fn label(&self) -> Option<&'static str> {
        Some(match self {
            NavAction::MoveDown => "Next item",
            NavAction::MoveUp => "Prev item",
            NavAction::HalfPageDown => "Half page down",
            NavAction::HalfPageUp => "Half page up",
            NavAction::JumpTop => "Top",
            NavAction::JumpBottom => "Bottom",
        })
    }
}

/// Generate default navigation key bindings, wrapping each NavAction with the given function.
pub fn nav_bindings<A: Clone>(wrap: impl Fn(NavAction) -> A) -> Vec<(KeyCode, KeyModifiers, A)> {
    vec![
        (
            KeyCode::Char('j'),
            KeyModifiers::NONE,
            wrap(NavAction::MoveDown),
        ),
        (KeyCode::Down, KeyModifiers::NONE, wrap(NavAction::MoveDown)),
        (
            KeyCode::Char('k'),
            KeyModifiers::NONE,
            wrap(NavAction::MoveUp),
        ),
        (KeyCode::Up, KeyModifiers::NONE, wrap(NavAction::MoveUp)),
        (
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
            wrap(NavAction::HalfPageDown),
        ),
        (
            KeyCode::Char('u'),
            KeyModifiers::CONTROL,
            wrap(NavAction::HalfPageUp),
        ),
        (
            KeyCode::Char('g'),
            KeyModifiers::NONE,
            wrap(NavAction::JumpTop),
        ),
        (
            KeyCode::Char('G'),
            KeyModifiers::NONE,
            wrap(NavAction::JumpBottom),
        ),
    ]
}

/// Generate search key bindings (/, n, N).
pub fn search_bindings<A: Clone>(start: A, next: A, prev: A) -> Vec<(KeyCode, KeyModifiers, A)> {
    vec![
        (KeyCode::Char('/'), KeyModifiers::NONE, start),
        (KeyCode::Char('n'), KeyModifiers::NONE, next),
        (KeyCode::Char('N'), KeyModifiers::NONE, prev),
    ]
}

/// Build a section header for help display.
pub fn help_section(title: &str) -> Vec<HelpEntry> {
    vec![
        (String::new(), String::new()),
        (String::new(), format!("── {title} ──")),
    ]
}

/// Execute a navigation action on a list, updating `selected_idx`.
/// Returns `true` if the selection actually changed.
pub fn execute_nav(
    nav: NavAction,
    selected_idx: &mut usize,
    item_count: usize,
    view_height: Option<u16>,
) -> bool {
    if item_count == 0 {
        return false;
    }
    let old = *selected_idx;
    match nav {
        NavAction::MoveDown => {
            if *selected_idx + 1 < item_count {
                *selected_idx += 1;
            }
        }
        NavAction::MoveUp => {
            if *selected_idx > 0 {
                *selected_idx -= 1;
            }
        }
        NavAction::HalfPageDown => {
            let half = (view_height.unwrap_or(20) / 2).max(1) as usize;
            *selected_idx = (*selected_idx + half).min(item_count - 1);
        }
        NavAction::HalfPageUp => {
            let half = (view_height.unwrap_or(20) / 2).max(1) as usize;
            *selected_idx = selected_idx.saturating_sub(half);
        }
        NavAction::JumpTop => {
            *selected_idx = 0;
        }
        NavAction::JumpBottom => {
            *selected_idx = item_count - 1;
        }
    }
    *selected_idx != old
}
