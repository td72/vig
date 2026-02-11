use git2::{Delta, DiffDelta, DiffLine, DiffOptions, ObjectType, Patch, Repository};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Untracked,
}

impl FileStatus {
    pub fn from_delta(delta: Delta) -> Self {
        match delta {
            Delta::Added => FileStatus::Added,
            Delta::Deleted => FileStatus::Deleted,
            Delta::Modified => FileStatus::Modified,
            Delta::Renamed => FileStatus::Renamed,
            _ => FileStatus::Modified,
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            FileStatus::Added => "A",
            FileStatus::Deleted => "D",
            FileStatus::Modified => "M",
            FileStatus::Renamed => "R",
            FileStatus::Untracked => "?",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum LineType {
    Context,
    Added,
    Deleted,
    HunkHeader,
}

#[derive(Debug, Clone)]
pub struct SideLine {
    pub line_no: u32,
    pub content: String,
}

#[derive(Debug, Clone)]
pub struct SideBySideRow {
    pub left: Option<SideLine>,
    pub right: Option<SideLine>,
    pub line_type: LineType,
}

#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String,
    pub rows: Vec<SideBySideRow>,
}

#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
}

#[derive(Debug, Clone)]
pub struct DiffStats {
    pub additions: usize,
    pub deletions: usize,
}

#[derive(Debug, Clone)]
pub struct DiffState {
    pub files: Vec<FileDiff>,
    pub branch_name: String,
    pub stats: DiffStats,
}

struct RawHunkLine {
    origin: char,
    old_lineno: Option<u32>,
    new_lineno: Option<u32>,
    content: String,
}

pub fn parse_diff(repo: &Repository, base_ref: Option<&str>) -> anyhow::Result<Vec<FileDiff>> {
    let head = match base_ref {
        Some(r) => {
            let obj = repo
                .revparse_single(r)
                .map_err(|e| anyhow::anyhow!("Cannot resolve '{}': {}", r, e))?;
            let tree_obj = obj
                .peel(ObjectType::Tree)
                .map_err(|e| anyhow::anyhow!("Cannot peel to tree: {}", e))?;
            Some(
                tree_obj
                    .into_tree()
                    .map_err(|_| anyhow::anyhow!("Not a tree"))?,
            )
        }
        None => repo.head().ok().and_then(|r| r.peel_to_tree().ok()),
    };
    let mut opts = DiffOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);
    opts.show_untracked_content(true);

    let diff = repo.diff_tree_to_workdir_with_index(head.as_ref(), Some(&mut opts))?;

    let mut files = Vec::new();

    let num_deltas = diff.deltas().count();
    for idx in 0..num_deltas {
        let delta = diff.get_delta(idx).unwrap();
        let status = delta_status(&delta);
        let path = delta_path(&delta);

        if let Ok(patch) = Patch::from_diff(&diff, idx) {
            if let Some(patch) = patch {
                let is_binary = patch.delta().flags().is_binary();
                if is_binary {
                    files.push(FileDiff {
                        path,
                        status,
                        hunks: Vec::new(),
                        is_binary: true,
                    });
                    continue;
                }

                let mut hunks = Vec::new();
                for hunk_idx in 0..patch.num_hunks() {
                    let (hunk, _) = patch.hunk(hunk_idx)?;
                    let header = String::from_utf8_lossy(hunk.header()).trim().to_string();

                    let mut raw_lines = Vec::new();
                    for line_idx in 0..patch.num_lines_in_hunk(hunk_idx)? {
                        let line = patch.line_in_hunk(hunk_idx, line_idx)?;
                        raw_lines.push(raw_from_diff_line(&line));
                    }

                    let rows = align_hunk_lines(&raw_lines);
                    hunks.push(DiffHunk { header, rows });
                }

                files.push(FileDiff {
                    path,
                    status,
                    hunks,
                    is_binary: false,
                });
            }
        }
    }

    Ok(files)
}

fn delta_status(delta: &DiffDelta) -> FileStatus {
    match delta.status() {
        Delta::Added | Delta::Untracked => {
            if delta.status() == Delta::Untracked {
                FileStatus::Untracked
            } else {
                FileStatus::Added
            }
        }
        other => FileStatus::from_delta(other),
    }
}

fn delta_path(delta: &DiffDelta) -> String {
    delta
        .new_file()
        .path()
        .or_else(|| delta.old_file().path())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "<unknown>".to_string())
}

fn raw_from_diff_line(line: &DiffLine) -> RawHunkLine {
    let content = String::from_utf8_lossy(line.content()).to_string();
    // Strip trailing newline for display
    let content = content.trim_end_matches('\n').to_string();
    RawHunkLine {
        origin: line.origin(),
        old_lineno: line.old_lineno(),
        new_lineno: line.new_lineno(),
        content,
    }
}

fn align_hunk_lines(lines: &[RawHunkLine]) -> Vec<SideBySideRow> {
    let mut rows = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        match lines[i].origin {
            ' ' => {
                // Context line
                rows.push(SideBySideRow {
                    left: Some(SideLine {
                        line_no: lines[i].old_lineno.unwrap_or(0),
                        content: lines[i].content.clone(),
                    }),
                    right: Some(SideLine {
                        line_no: lines[i].new_lineno.unwrap_or(0),
                        content: lines[i].content.clone(),
                    }),
                    line_type: LineType::Context,
                });
                i += 1;
            }
            '-' => {
                // Collect consecutive deletions
                let del_start = i;
                while i < lines.len() && lines[i].origin == '-' {
                    i += 1;
                }
                let dels = &lines[del_start..i];

                // Collect consecutive additions
                let add_start = i;
                while i < lines.len() && lines[i].origin == '+' {
                    i += 1;
                }
                let adds = &lines[add_start..i];

                // Pair them up
                let max_len = dels.len().max(adds.len());
                for j in 0..max_len {
                    let left = dels.get(j).map(|l| SideLine {
                        line_no: l.old_lineno.unwrap_or(0),
                        content: l.content.clone(),
                    });
                    let right = adds.get(j).map(|l| SideLine {
                        line_no: l.new_lineno.unwrap_or(0),
                        content: l.content.clone(),
                    });

                    let line_type = match (left.is_some(), right.is_some()) {
                        (true, true) => LineType::Deleted,
                        (true, false) => LineType::Deleted,
                        (false, true) => LineType::Added,
                        (false, false) => continue,
                    };

                    rows.push(SideBySideRow {
                        left,
                        right,
                        line_type,
                    });
                }
            }
            '+' => {
                // Addition without preceding deletion
                rows.push(SideBySideRow {
                    left: None,
                    right: Some(SideLine {
                        line_no: lines[i].new_lineno.unwrap_or(0),
                        content: lines[i].content.clone(),
                    }),
                    line_type: LineType::Added,
                });
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    rows
}

pub fn compute_stats(files: &[FileDiff]) -> DiffStats {
    let mut additions = 0;
    let mut deletions = 0;
    for file in files {
        for hunk in &file.hunks {
            for row in &hunk.rows {
                match row.line_type {
                    LineType::Added => additions += 1,
                    LineType::Deleted => {
                        // Paired rows count as both a deletion and addition
                        if row.right.is_some() {
                            additions += 1;
                        }
                        deletions += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    DiffStats {
        additions,
        deletions,
    }
}
