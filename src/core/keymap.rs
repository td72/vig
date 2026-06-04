use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;
use std::fmt;
use std::str::FromStr;

/// Normalized key input for use as HashMap key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyInput {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl From<KeyEvent> for KeyInput {
    fn from(key: KeyEvent) -> Self {
        // SHIFT is redundant for printable chars (the char itself already
        // encodes the shifted form, e.g. 'G' or '?') and for BackTab (which is
        // Shift+Tab by definition). crossterm reports it inconsistently across
        // terminals, so strip it to keep the same logical key matchable.
        let mods = if matches!(key.code, KeyCode::Char(_) | KeyCode::BackTab) {
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

/// Parse a key name string into a KeyCode.
fn parse_key_code(name: &str) -> Result<KeyCode, String> {
    match name {
        "Enter" | "Return" | "CR" => Ok(KeyCode::Enter),
        "Esc" | "Escape" => Ok(KeyCode::Esc),
        "Tab" => Ok(KeyCode::Tab),
        "BackTab" | "S-Tab" => Ok(KeyCode::BackTab),
        "Backspace" | "BS" => Ok(KeyCode::Backspace),
        "Delete" | "Del" => Ok(KeyCode::Delete),
        "Up" => Ok(KeyCode::Up),
        "Down" => Ok(KeyCode::Down),
        "Left" => Ok(KeyCode::Left),
        "Right" => Ok(KeyCode::Right),
        "Home" => Ok(KeyCode::Home),
        "End" => Ok(KeyCode::End),
        "PageUp" => Ok(KeyCode::PageUp),
        "PageDown" => Ok(KeyCode::PageDown),
        "Space" => Ok(KeyCode::Char(' ')),
        s if s.len() == 1 => Ok(KeyCode::Char(s.chars().next().unwrap())),
        _ => Err(format!("Unknown key: {name}")),
    }
}

impl FromStr for KeyInput {
    type Err = String;

    /// Parse a human-readable key string into a KeyInput.
    ///
    /// Supported formats:
    /// - `"j"`, `"G"`, `"/"` — plain character keys
    /// - `"Enter"`, `"Esc"`, `"Tab"`, `"BackTab"`, `"Up"`, `"Down"` etc.
    /// - `"Ctrl+d"`, `"Ctrl+u"` — with Ctrl modifier
    /// - `"Space"` — space character
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Empty key string".to_string());
        }

        if let Some(rest) = s.strip_prefix("Ctrl+") {
            let code = parse_key_code(rest)?;
            Ok(Self {
                code,
                modifiers: KeyModifiers::CONTROL,
            })
        } else {
            let code = parse_key_code(s)?;
            Ok(Self {
                code,
                modifiers: KeyModifiers::NONE,
            })
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

/// Implement `FromStr` for simple action enums (no nested variants).
macro_rules! impl_action_from_str {
    ($ty:ty, $( $variant:ident ),+ $(,)?) => {
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $( stringify!($variant) => Ok(Self::$variant), )+
                    _ => Err(format!("Unknown action: {s}")),
                }
            }
        }
    };
}

/// Implement `FromStr` for pane action enums that wrap `Nav(NavAction)`, `Search(SearchAction)`,
/// and have their own plain variants. Nested actions use dot notation: `"Nav.MoveDown"`, `"Search.Start"`.
///
/// When the `search:` + `esc:` variant is provided, also generates `HasSearchEsc` impl.
#[macro_export]
macro_rules! impl_pane_action_from_str {
    ($ty:ty, nav: $nav_wrap:ident, search: $search_wrap:ident, esc: $esc_variant:ident, $( $variant:ident ),+ $(,)?) => {
        // FromStr
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if let Some(rest) = s.strip_prefix("Nav.") {
                    return rest.parse::<$crate::core::keymap::NavAction>()
                        .map(Self::$nav_wrap);
                }
                if let Some(rest) = s.strip_prefix("Search.") {
                    return rest.parse::<$crate::core::keymap::SearchAction>()
                        .map(Self::$search_wrap);
                }
                match s {
                    stringify!($esc_variant) => Ok(Self::$esc_variant),
                    $( stringify!($variant) => Ok(Self::$variant), )+
                    _ => Err(format!("Unknown action: {s}")),
                }
            }
        }
        // HasSearchEsc
        impl $crate::core::pane::HasSearchEsc for $ty {
            fn as_search(&self) -> Option<&$crate::core::keymap::SearchAction> {
                match self {
                    Self::$search_wrap(sa) => Some(sa),
                    _ => None,
                }
            }
            fn is_esc(&self) -> bool {
                matches!(self, Self::$esc_variant)
            }
        }
    };
    ($ty:ty, nav: $nav_wrap:ident, search: $search_wrap:ident, $( $variant:ident ),+ $(,)?) => {
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if let Some(rest) = s.strip_prefix("Nav.") {
                    return rest.parse::<$crate::core::keymap::NavAction>()
                        .map(Self::$nav_wrap);
                }
                if let Some(rest) = s.strip_prefix("Search.") {
                    return rest.parse::<$crate::core::keymap::SearchAction>()
                        .map(Self::$search_wrap);
                }
                match s {
                    $( stringify!($variant) => Ok(Self::$variant), )+
                    _ => Err(format!("Unknown action: {s}")),
                }
            }
        }
    };
    ($ty:ty, nav: $nav_wrap:ident, $( $variant:ident ),+ $(,)?) => {
        impl std::str::FromStr for $ty {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                if let Some(rest) = s.strip_prefix("Nav.") {
                    return rest.parse::<$crate::core::keymap::NavAction>()
                        .map(Self::$nav_wrap);
                }
                match s {
                    $( stringify!($variant) => Ok(Self::$variant), )+
                    _ => Err(format!("Unknown action: {s}")),
                }
            }
        }
    };
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

impl_action_from_str!(
    NavAction,
    MoveDown,
    MoveUp,
    HalfPageDown,
    HalfPageUp,
    JumpTop,
    JumpBottom
);

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

/// Common search actions shared across panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchAction {
    Start,
    Next,
    Prev,
}

impl_action_from_str!(SearchAction, Start, Next, Prev);

impl ActionHelp for SearchAction {
    fn label(&self) -> Option<&'static str> {
        Some(match self {
            SearchAction::Start => "Search",
            SearchAction::Next => "Next match",
            SearchAction::Prev => "Prev match",
        })
    }
}

/// Generate search key bindings (/, n, N), wrapping each SearchAction with the given function.
pub fn search_bindings<A: Clone>(
    wrap: impl Fn(SearchAction) -> A,
) -> Vec<(KeyCode, KeyModifiers, A)> {
    vec![
        (
            KeyCode::Char('/'),
            KeyModifiers::NONE,
            wrap(SearchAction::Start),
        ),
        (
            KeyCode::Char('n'),
            KeyModifiers::NONE,
            wrap(SearchAction::Next),
        ),
        (
            KeyCode::Char('N'),
            KeyModifiers::NONE,
            wrap(SearchAction::Prev),
        ),
    ]
}

/// Page-level view actions (quit, help, refresh, tab navigation, etc.).
/// Each page maps these to its own concrete behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewAction {
    Quit,
    Help,
    Refresh,
    OpenEditor,
    PrevTab,
    NextTab,
    CyclePaneForward,
    CyclePaneBackward,
}

impl_action_from_str!(
    ViewAction,
    Quit,
    Help,
    Refresh,
    OpenEditor,
    PrevTab,
    NextTab,
    CyclePaneForward,
    CyclePaneBackward
);

impl ActionHelp for ViewAction {
    fn label(&self) -> Option<&'static str> {
        Some(match self {
            ViewAction::Quit => "Quit",
            ViewAction::Help => "Toggle help",
            ViewAction::Refresh => "Refresh",
            ViewAction::OpenEditor => "Open in $EDITOR",
            ViewAction::PrevTab => "Prev tab",
            ViewAction::NextTab => "Next tab",
            ViewAction::CyclePaneForward => "Next pane",
            ViewAction::CyclePaneBackward => "Prev pane",
        })
    }
}

/// Generate default view-level key bindings. Kept for regression tests.
#[allow(dead_code)]
pub fn view_bindings<A: Clone>(wrap: impl Fn(ViewAction) -> A) -> Vec<(KeyCode, KeyModifiers, A)> {
    vec![
        (
            KeyCode::Char('q'),
            KeyModifiers::NONE,
            wrap(ViewAction::Quit),
        ),
        (
            KeyCode::Char('?'),
            KeyModifiers::NONE,
            wrap(ViewAction::Help),
        ),
        (
            KeyCode::Char('r'),
            KeyModifiers::NONE,
            wrap(ViewAction::Refresh),
        ),
        (
            KeyCode::Char('h'),
            KeyModifiers::NONE,
            wrap(ViewAction::PrevTab),
        ),
        (
            KeyCode::Char('l'),
            KeyModifiers::NONE,
            wrap(ViewAction::NextTab),
        ),
        (
            KeyCode::Tab,
            KeyModifiers::NONE,
            wrap(ViewAction::CyclePaneForward),
        ),
        (
            KeyCode::BackTab,
            KeyModifiers::NONE,
            wrap(ViewAction::CyclePaneBackward),
        ),
    ]
}

/// Build a section header for help display.
pub fn help_section(title: &str) -> Vec<HelpEntry> {
    vec![
        (String::new(), String::new()),
        (String::new(), format!("── {title} ──")),
    ]
}

/// Number of rows a half-page navigation should move, given the viewport
/// height. Always at least 1 so a half-page step is never a no-op on very
/// short viewports.
pub fn half_page_step(view_height: u16) -> u16 {
    (view_height / 2).max(1)
}

/// Application-level actions for global key bindings (Ctrl+c, page switching).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppAction {
    /// Quit the application.
    Quit,
    /// Switch to the page at the given 0-indexed position.
    SwitchPage(usize),
}

impl FromStr for AppAction {
    type Err = String;

    /// Parse action strings for `AppAction`.
    ///
    /// - `"Quit"` → `AppAction::Quit`
    /// - `"SwitchPage:<n>"` → `AppAction::SwitchPage(n)`
    ///
    /// Note: `"page:<name>"` strings from the KDL config are **not** handled here.
    /// `build_app_keymap` resolves page names to indices and constructs
    /// `AppAction::SwitchPage` directly, without going through `FromStr`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s == "Quit" {
            return Ok(Self::Quit);
        }
        if let Some(idx_str) = s.strip_prefix("SwitchPage:") {
            let idx: usize = idx_str
                .parse()
                .map_err(|_| format!("Invalid page index in SwitchPage: {idx_str:?}"))?;
            return Ok(Self::SwitchPage(idx));
        }
        Err(format!("Unknown AppAction: {s:?}"))
    }
}

/// Build an app-level keymap from `(key_str, action_str)` pairs.
///
/// Page-name strings like `"page:git"` are resolved to indices using `page_names`.
/// Returns `Err` on the first invalid key, unknown page name, or invalid action.
pub fn build_app_keymap(
    entries: &[(String, String)],
    page_names: &[&str],
) -> Result<Keymap<AppAction>, String> {
    let mut km = Keymap::new();
    for (key_str, action_str) in entries {
        let ki = key_str
            .parse::<KeyInput>()
            .map_err(|e| format!("invalid app key {key_str:?}: {e}"))?;
        let action = if action_str == "Quit" {
            AppAction::Quit
        } else if let Some(name) = action_str.strip_prefix("page:") {
            let idx = page_names
                .iter()
                .position(|p| *p == name)
                .ok_or_else(|| format!("unknown page name in app keymap: {name:?}"))?;
            AppAction::SwitchPage(idx)
        } else {
            action_str
                .parse::<AppAction>()
                .map_err(|e| format!("invalid app action {action_str:?}: {e}"))?
        };
        km = km.bind(ki.code, ki.modifiers, action);
    }
    Ok(km)
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
            let half = half_page_step(view_height.unwrap_or(20)) as usize;
            *selected_idx = (*selected_idx + half).min(item_count - 1);
        }
        NavAction::HalfPageUp => {
            let half = half_page_step(view_height.unwrap_or(20)) as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_plain_char() {
        let ki: KeyInput = "j".parse().unwrap();
        assert_eq!(ki.code, KeyCode::Char('j'));
        assert_eq!(ki.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn parse_uppercase_char() {
        let ki: KeyInput = "G".parse().unwrap();
        assert_eq!(ki.code, KeyCode::Char('G'));
        assert_eq!(ki.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn parse_ctrl_modifier() {
        let ki: KeyInput = "Ctrl+d".parse().unwrap();
        assert_eq!(ki.code, KeyCode::Char('d'));
        assert_eq!(ki.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!("Enter".parse::<KeyInput>().unwrap().code, KeyCode::Enter);
        assert_eq!("Esc".parse::<KeyInput>().unwrap().code, KeyCode::Esc);
        assert_eq!("Tab".parse::<KeyInput>().unwrap().code, KeyCode::Tab);
        assert_eq!(
            "BackTab".parse::<KeyInput>().unwrap().code,
            KeyCode::BackTab
        );
        assert_eq!("Up".parse::<KeyInput>().unwrap().code, KeyCode::Up);
        assert_eq!("Down".parse::<KeyInput>().unwrap().code, KeyCode::Down);
        assert_eq!(
            "Space".parse::<KeyInput>().unwrap().code,
            KeyCode::Char(' ')
        );
    }

    #[test]
    fn parse_roundtrip() {
        for s in &["j", "G", "Enter", "Esc", "Tab"] {
            let ki: KeyInput = s.parse().unwrap();
            assert_eq!(ki.to_string(), *s);
        }
        let ctrl_d: KeyInput = "Ctrl+d".parse().unwrap();
        assert_eq!(ctrl_d.to_string(), "Ctrl+d");
    }

    #[test]
    fn parse_invalid() {
        assert!("".parse::<KeyInput>().is_err());
        assert!("FooBar".parse::<KeyInput>().is_err());
    }

    #[test]
    fn parse_nav_action() {
        assert_eq!(
            "MoveDown".parse::<NavAction>().unwrap(),
            NavAction::MoveDown
        );
        assert_eq!("JumpTop".parse::<NavAction>().unwrap(), NavAction::JumpTop);
        assert!("Unknown".parse::<NavAction>().is_err());
    }

    #[test]
    fn parse_search_action() {
        assert_eq!(
            "Start".parse::<SearchAction>().unwrap(),
            SearchAction::Start
        );
        assert_eq!("Prev".parse::<SearchAction>().unwrap(), SearchAction::Prev);
        assert!("Unknown".parse::<SearchAction>().is_err());
    }

    #[test]
    fn parse_view_action() {
        assert_eq!("Quit".parse::<ViewAction>().unwrap(), ViewAction::Quit);
        assert_eq!(
            "CyclePaneForward".parse::<ViewAction>().unwrap(),
            ViewAction::CyclePaneForward
        );
        assert!("Unknown".parse::<ViewAction>().is_err());
    }
}
