//! Disk cache for the project list and the last boards, under the GitHub
//! page's cache directory (`<cache_dir>/vig/<version>/<owner>/<repo>/projects/`).

use crate::github::domain::disk_cache::{cache_dir, load_json, save_json};
use crate::projects::domain::types::{Board, ProjectListCache};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn dir() -> Option<PathBuf> {
    Some(cache_dir()?.join("projects"))
}

/// The cached linked projects. A file without `repo` predates the
/// repository-scoped list (it held the owner's projects) and is ignored.
pub fn load_project_list() -> Option<ProjectListCache> {
    load_json::<ProjectListCache>(&dir()?.join("list.json")).filter(|c| !c.repo.is_empty())
}

pub fn save_project_list(list: &ProjectListCache) {
    if let Some(dir) = dir() {
        save_json(&dir.join("list.json"), list);
    }
}

/// The cached board and how long ago it was fetched: the last fetch wrote
/// the file, so the board is as old as the file's mtime.
pub fn load_board_with_age(number: u64) -> Option<(Board, Duration)> {
    let path = dir()?.join(format!("board-{number}.json"));
    let board = load_json(&path)?;
    Some((board, file_age(&path)))
}

/// How old the file is; `Duration::MAX` when the mtime cannot be read.
fn file_age(path: &PathBuf) -> Duration {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .unwrap_or(Duration::MAX)
}

pub fn save_board(board: &Board) {
    if let Some(dir) = dir() {
        save_json(&dir.join(format!("board-{}.json", board.number)), board);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::domain::types::{ItemList, ProjectItem};

    #[test]
    fn board_roundtrips_through_json_with_custom_fields() {
        let items: ItemList = serde_json::from_str(
            r#"{"items":[{"id":"I1","title":"t","status":"Todo","priority":"P1","content":{"type":"Issue","number":1,"title":"t"}}],"totalCount":1}"#,
        )
        .unwrap();
        let board = Board {
            number: 7,
            fields: vec![],
            items: items.items,
            total_count: 1,
        };
        let tmp = std::env::temp_dir().join("vig_test_projects_cache");
        let _ = std::fs::remove_dir_all(&tmp);
        let path = tmp.join("board-7.json");
        save_json(&path, &board);
        let loaded: Board = load_json(&path).expect("board");
        assert!(
            file_age(&path) < Duration::from_secs(60),
            "a just-written file is young"
        );
        assert_eq!(
            file_age(&tmp.join("missing.json")),
            Duration::MAX,
            "an unreadable mtime counts as infinitely old"
        );
        let item: &ProjectItem = &loaded.items[0];
        assert_eq!(item.status.as_deref(), Some("Todo"));
        assert_eq!(item.field_text("priority").as_deref(), Some("P1"));
        assert_eq!(item.number(), Some(1));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
