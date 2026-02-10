use crate::event::Event;
use anyhow::Result;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub struct FsWatcher {
    _debouncer: notify_debouncer_mini::Debouncer<notify::RecommendedWatcher>,
}

impl FsWatcher {
    pub fn new(watch_path: &Path, tx: Sender<Event>) -> Result<Self> {
        let debouncer = notify_debouncer_mini::new_debouncer(
            Duration::from_millis(500),
            move |events: Result<Vec<notify_debouncer_mini::DebouncedEvent>, notify::Error>| {
                if let Ok(events) = events {
                    let dominated_by_git_internal = events.iter().all(|e| {
                        let in_git = e.path.components().any(|c| c.as_os_str() == ".git");
                        let is_index = e.path.ends_with(".git/index");
                        in_git && !is_index
                    });
                    // Skip if ALL events are .git-internal (except index changes)
                    if !dominated_by_git_internal {
                        let _ = tx.send(Event::FsChange);
                    }
                }
            },
        )?;

        let mut debouncer = debouncer;
        debouncer
            .watcher()
            .watch(watch_path, notify::RecursiveMode::Recursive)?;

        Ok(Self {
            _debouncer: debouncer,
        })
    }
}
