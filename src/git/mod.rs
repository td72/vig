// Shared architecture
pub(crate) mod layout;
pub(crate) mod page;
pub(crate) mod panes;
pub mod state;

// Domain-specific
pub(crate) mod branch_action;
pub(crate) mod diff;
pub(crate) mod graph;
pub(crate) mod repository;
pub(crate) mod search;
pub mod watcher;
