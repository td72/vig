use crate::core::app::Page;
use std::ffi::OsString;

#[derive(Debug)]
pub struct ExternalCommand {
    pub program: String,
    pub args: Vec<OsString>,
}

/// What a page asks the app to do after handling a key.
pub enum PageAction {
    None,
    Suspend(ExternalCommand),
    /// Show `Page` on top of the current page until it asks for [`Pop`]
    /// (a PR's diff window, for example). The page underneath keeps its
    /// state and is shown again, as it was, when the window closes.
    ///
    /// [`Pop`]: PageAction::Pop
    Push(Page),
    /// Close the topmost pushed page (no-op from a page in the tab row).
    Pop,
}

impl std::fmt::Debug for PageAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PageAction::None => write!(f, "None"),
            PageAction::Suspend(cmd) => f.debug_tuple("Suspend").field(cmd).finish(),
            PageAction::Push(page) => f.debug_tuple("Push").field(&page.id()).finish(),
            PageAction::Pop => write!(f, "Pop"),
        }
    }
}
