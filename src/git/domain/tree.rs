use crate::git::domain::diff::FileDiff;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum TreeEntry {
    Dir {
        path: String,
        depth: usize,
        collapsed: bool,
    },
    File {
        file_idx: usize,
        depth: usize,
    },
}

pub fn build_tree_entries(files: &[FileDiff], collapsed_dirs: &HashSet<String>) -> Vec<TreeEntry> {
    if files.is_empty() {
        return Vec::new();
    }

    // Count files per directory to detect single-file directories
    let mut dir_file_count: HashMap<String, usize> = HashMap::new();
    for file in files {
        let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
        if parts.len() == 2 {
            // Has a directory component
            let dir = parts[1];
            // Count for this dir and all ancestor dirs
            let mut current = String::new();
            for segment in dir.split('/') {
                if !current.is_empty() {
                    current.push('/');
                }
                current.push_str(segment);
                *dir_file_count.entry(current.clone()).or_insert(0) += 1;
            }
        }
    }

    let mut entries = Vec::new();
    let mut prev_dir_parts: Vec<&str> = Vec::new();

    for (file_idx, file) in files.iter().enumerate() {
        let parts: Vec<&str> = file.path.rsplitn(2, '/').collect();
        if parts.len() == 2 {
            let dir = parts[1];
            let dir_parts: Vec<&str> = dir.split('/').collect();

            // Check if the entire path from root is single-file at every level
            // If so, inline the file (show full path, no directory node)
            let leaf_dir = dir.to_string();
            if dir_file_count.get(&leaf_dir).copied().unwrap_or(0) == 1 {
                // Single file in this directory — inline with full path at depth 0
                entries.push(TreeEntry::File { file_idx, depth: 0 });
                // Don't update prev_dir_parts since we inlined
                prev_dir_parts = Vec::new();
                continue;
            }

            // Find common prefix with previous directory
            let common_len = prev_dir_parts
                .iter()
                .zip(dir_parts.iter())
                .take_while(|(a, b)| a == b)
                .count();

            // Emit new directory entries for parts beyond common prefix
            let mut collapsed_ancestor = false;
            for i in common_len..dir_parts.len() {
                let dir_path: String = dir_parts[..=i].join("/");
                let is_collapsed = collapsed_dirs.contains(&dir_path);
                if !collapsed_ancestor {
                    entries.push(TreeEntry::Dir {
                        path: dir_path.clone(),
                        depth: i,
                        collapsed: is_collapsed,
                    });
                }
                if is_collapsed {
                    collapsed_ancestor = true;
                }
            }

            // Check if any ancestor dir is collapsed
            let mut skip_file = false;
            let mut check_path = String::new();
            for part in &dir_parts {
                if !check_path.is_empty() {
                    check_path.push('/');
                }
                check_path.push_str(part);
                if collapsed_dirs.contains(&check_path) {
                    skip_file = true;
                    break;
                }
            }

            if !skip_file {
                entries.push(TreeEntry::File {
                    file_idx,
                    depth: dir_parts.len(),
                });
            }

            prev_dir_parts = dir_parts;
        } else {
            // Root-level file (no directory component)
            prev_dir_parts = Vec::new();
            entries.push(TreeEntry::File { file_idx, depth: 0 });
        }
    }

    entries
}
