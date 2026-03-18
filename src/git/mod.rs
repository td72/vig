// Shared architecture
pub(crate) mod page;
pub(crate) mod panes;
pub mod state;

// Domain-specific
pub(crate) mod domain;
pub use domain::watcher;
