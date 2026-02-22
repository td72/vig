use crate::github::state::GitHubState;

/// Create a GitHub page.
pub fn new_page() -> crate::core::app::Page {
    crate::core::app::Page::new(GitHubState::new())
}
