//! Disk cache for the project list and the last boards, under the GitHub
//! page's cache directory (`<cache_dir>/vig/<version>/<owner>/<repo>/projects/`).

use crate::github::domain::disk_cache::{cache_dir, load_json, save_json};
use crate::projects::domain::types::{Board, ProjectListCache};
use std::path::PathBuf;

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

pub fn load_board(number: u64) -> Option<Board> {
    load_json(&dir()?.join(format!("board-{number}.json")))
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
        let item: &ProjectItem = &loaded.items[0];
        assert_eq!(item.status.as_deref(), Some("Todo"));
        assert_eq!(item.field_text("priority").as_deref(), Some("P1"));
        assert_eq!(item.number(), Some(1));
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
