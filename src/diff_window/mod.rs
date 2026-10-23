//! A diff window: a page pushed on top of the tab row (`PageAction::Push`)
//! showing a patch as the Git page does — a file tree on the left, the
//! side-by-side diff on the right — and closing with `q` / `Esc`. The patch
//! comes from a [`DiffSource`] (a pull request's `gh pr diff`); the window
//! itself is independent of where it came from.

pub mod state;

pub use state::{DiffSource, DiffWindowState};
