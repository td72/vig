use crate::git::domain::repository::CommitInfo;

pub const NUM_GRAPH_COLORS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphCell {
    Commit,     // ●
    Vertical,   // │
    Horizontal, // ─
    DownRight,  // ╮  line from left, turns down
    DownLeft,   // ╭  line from right, turns down
    UpRight,    // ╯  line from above, turns left
    UpLeft,     // ╰  line from above, turns right
    Cross,      // ┼  vertical + horizontal crossing
    Empty,      //
}

#[derive(Debug, Clone)]
pub struct GraphRow {
    pub cells: Vec<GraphCell>,
    #[allow(dead_code)]
    pub commit_col: usize,
    pub colors: Vec<usize>,
    /// For each cell, the commit index that originated the pipe passing through it.
    pub from_indices: Vec<Option<usize>>,
}

/// Build a lane-based branch graph for the given commit list.
///
/// Each lane tracks the next expected commit hash. When a commit is found,
/// its first parent continues in the same lane and additional parents are
/// assigned to free or existing lanes, producing merge/fork lines.
///
/// Each lane also tracks `from_commit_idx` — the index of the commit that
/// spawned this pipe segment (like lazygit's `fromHash`).
pub fn build_graph(commits: &[CommitInfo]) -> Vec<GraphRow> {
    if commits.is_empty() {
        return Vec::new();
    }

    let mut active_lanes: Vec<Option<String>> = Vec::new();
    let mut lane_colors: Vec<usize> = Vec::new();
    let mut lane_from: Vec<Option<usize>> = Vec::new(); // origin commit idx per lane
    let mut next_color: usize = 0;
    let mut rows = Vec::new();

    for (commit_idx, commit) in commits.iter().enumerate() {
        let hash = &commit.full_hash;

        // Find which lane(s) expect this commit
        let matching: Vec<usize> = active_lanes
            .iter()
            .enumerate()
            .filter_map(|(i, lane)| {
                if lane.as_deref() == Some(hash) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        let commit_col = if matching.is_empty() {
            let col = active_lanes
                .iter()
                .position(|l| l.is_none())
                .unwrap_or_else(|| {
                    active_lanes.push(None);
                    lane_colors.push(0);
                    lane_from.push(None);
                    active_lanes.len() - 1
                });
            lane_colors[col] = next_color % NUM_GRAPH_COLORS;
            next_color += 1;
            active_lanes[col] = Some(hash.clone());
            lane_from[col] = Some(commit_idx);
            col
        } else {
            matching[0]
        };

        // Close duplicate lanes (converging)
        for &lane_idx in matching.iter().skip(1) {
            active_lanes[lane_idx] = None;
        }

        // Assign parents — update lane_from for this commit's pipes
        let parents = &commit.parent_ids;
        if parents.is_empty() {
            active_lanes[commit_col] = None;
            lane_from[commit_col] = None;
        } else {
            // First parent continues in the same lane
            active_lanes[commit_col] = Some(parents[0].clone());
            lane_from[commit_col] = Some(commit_idx);

            // Additional parents get new or existing lanes
            for parent in parents.iter().skip(1) {
                let already = active_lanes
                    .iter()
                    .any(|l| l.as_deref() == Some(parent.as_str()));
                if !already {
                    let free = active_lanes
                        .iter()
                        .position(|l| l.is_none())
                        .unwrap_or_else(|| {
                            active_lanes.push(None);
                            lane_colors.push(0);
                            lane_from.push(None);
                            active_lanes.len() - 1
                        });
                    lane_colors[free] = next_color % NUM_GRAPH_COLORS;
                    next_color += 1;
                    active_lanes[free] = Some(parent.clone());
                    lane_from[free] = Some(commit_idx);
                }
            }
        }

        // Build row cells
        let width = active_lanes.len();
        let mut cells = vec![GraphCell::Empty; width];
        let mut colors = vec![0usize; width];
        let mut from_indices: Vec<Option<usize>> = vec![None; width];

        // Place vertical lines for all active lanes
        for (i, lane) in active_lanes.iter().enumerate() {
            if lane.is_some() {
                cells[i] = GraphCell::Vertical;
                colors[i] = lane_colors[i];
                from_indices[i] = lane_from[i];
            }
        }

        // Place the commit marker
        cells[commit_col] = GraphCell::Commit;
        colors[commit_col] = lane_colors[commit_col];
        from_indices[commit_col] = Some(commit_idx);

        // Draw converging lines from duplicate lanes to commit
        for &lane_idx in matching.iter().skip(1) {
            let merge_color = lane_colors[lane_idx];
            let merge_from = lane_from[lane_idx];
            if lane_idx < commit_col {
                cells[lane_idx] = GraphCell::UpLeft;
                colors[lane_idx] = merge_color;
                from_indices[lane_idx] = merge_from;
                for col in (lane_idx + 1)..commit_col {
                    if cells[col] == GraphCell::Vertical {
                        cells[col] = GraphCell::Cross;
                    } else {
                        cells[col] = GraphCell::Horizontal;
                    }
                    colors[col] = merge_color;
                    from_indices[col] = merge_from;
                }
            } else if lane_idx > commit_col {
                cells[lane_idx] = GraphCell::UpRight;
                colors[lane_idx] = merge_color;
                from_indices[lane_idx] = merge_from;
                for col in (commit_col + 1)..lane_idx {
                    if cells[col] == GraphCell::Vertical {
                        cells[col] = GraphCell::Cross;
                    } else {
                        cells[col] = GraphCell::Horizontal;
                    }
                    colors[col] = merge_color;
                    from_indices[col] = merge_from;
                }
            }
            // Clear the closed lane's from
            lane_from[lane_idx] = None;
        }

        // Draw fork lines to new parent lanes (merge commits)
        if parents.len() > 1 {
            for parent in parents.iter().skip(1) {
                if let Some(parent_lane) = active_lanes
                    .iter()
                    .position(|l| l.as_deref() == Some(parent.as_str()))
                {
                    let fork_color = lane_colors[parent_lane];
                    let fork_from = lane_from[parent_lane]; // = Some(commit_idx)
                    if parent_lane < commit_col {
                        if cells[parent_lane] == GraphCell::Vertical
                            || cells[parent_lane] == GraphCell::Empty
                        {
                            cells[parent_lane] = GraphCell::DownLeft;
                        }
                        colors[parent_lane] = fork_color;
                        from_indices[parent_lane] = fork_from;
                        for col in (parent_lane + 1)..commit_col {
                            if cells[col] == GraphCell::Vertical {
                                cells[col] = GraphCell::Cross;
                            } else if cells[col] == GraphCell::Empty {
                                cells[col] = GraphCell::Horizontal;
                            }
                            colors[col] = fork_color;
                            from_indices[col] = fork_from;
                        }
                    } else if parent_lane > commit_col {
                        if cells[parent_lane] == GraphCell::Vertical
                            || cells[parent_lane] == GraphCell::Empty
                        {
                            cells[parent_lane] = GraphCell::DownRight;
                        }
                        colors[parent_lane] = fork_color;
                        from_indices[parent_lane] = fork_from;
                        for col in (commit_col + 1)..parent_lane {
                            if cells[col] == GraphCell::Vertical {
                                cells[col] = GraphCell::Cross;
                            } else if cells[col] == GraphCell::Empty {
                                cells[col] = GraphCell::Horizontal;
                            }
                            colors[col] = fork_color;
                            from_indices[col] = fork_from;
                        }
                    }
                }
            }
        }

        // Trim trailing Empty cells
        while cells.last() == Some(&GraphCell::Empty) {
            cells.pop();
            colors.pop();
            from_indices.pop();
        }

        rows.push(GraphRow {
            cells,
            commit_col,
            colors,
            from_indices,
        });
    }

    rows
}
