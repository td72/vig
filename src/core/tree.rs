//! Tree layout for list panes: order items depth-first under their parents
//! and compute the `├─ └─ │` guide prefix for each row. Used by the GitHub
//! lists (sub-issues, stacked PRs) and available to any page with a
//! parent/child relation (compose projects, process trees, job steps).

/// Placement of a row in the nested list.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TreePos {
    pub depth: usize,
    /// Guides drawn before the item (`│ ├─ └─`), empty for top-level rows.
    pub prefix: String,
}

/// Depth-first order of `n` items nested under their parents.
///
/// Top-level items (no parent, or a parent that is not in the list) keep
/// their original relative order, and so do siblings. Members of a parent
/// cycle (an item whose parent chain leads back to itself) are top-level;
/// items merely hanging off a cycle still nest under their parent. Returns
/// `(original index, position)` per output row.
pub fn nest_by(
    n: usize,
    number: impl Fn(usize) -> u64,
    parent: impl Fn(usize) -> Option<u64>,
) -> Vec<(usize, TreePos)> {
    let index_of = |num: u64| (0..n).find(|&i| number(i) == num);
    let parent_idx: Vec<Option<usize>> = (0..n)
        .map(|i| parent(i).and_then(index_of).filter(|&p| p != i))
        .collect();
    // An item whose parent chain comes back to itself is in a cycle; drop
    // its parent link so every cycle member becomes a root.
    let in_cycle = |i: usize| {
        let mut cur = parent_idx[i];
        for _ in 0..n {
            match cur {
                Some(p) if p == i => return true,
                Some(p) => cur = parent_idx[p],
                None => return false,
            }
        }
        false
    };
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut has_parent = vec![false; n];
    for (i, hp) in has_parent.iter_mut().enumerate() {
        if let Some(p) = parent_idx[i].filter(|_| !in_cycle(i)) {
            children[p].push(i);
            *hp = true;
        }
    }

    let mut out = Vec::with_capacity(n);
    let mut visited = vec![false; n];
    // (index, depth, ancestors' "more siblings follow" flags, is last sibling)
    fn walk(
        i: usize,
        depth: usize,
        trail: &mut Vec<bool>,
        last: bool,
        children: &[Vec<usize>],
        visited: &mut [bool],
        out: &mut Vec<(usize, TreePos)>,
    ) {
        if visited[i] {
            return;
        }
        visited[i] = true;
        let mut prefix = String::new();
        if depth > 0 {
            for &more in trail.iter() {
                prefix.push_str(if more { "│  " } else { "   " });
            }
            prefix.push_str(if last { "└─ " } else { "├─ " });
        }
        out.push((i, TreePos { depth, prefix }));
        let kids: Vec<usize> = children[i]
            .iter()
            .copied()
            .filter(|&c| !visited[c])
            .collect();
        if depth > 0 {
            trail.push(!last);
        }
        for (k, &c) in kids.iter().enumerate() {
            walk(
                c,
                depth + 1,
                trail,
                k + 1 == kids.len(),
                children,
                visited,
                out,
            );
        }
        if depth > 0 {
            trail.pop();
        }
    }
    let mut trail = Vec::new();
    for (i, _) in has_parent.iter().enumerate().filter(|(_, hp)| !**hp) {
        walk(i, 0, &mut trail, true, &children, &mut visited, &mut out);
    }
    // Safety net: anything still unreached is surfaced as a top-level row.
    for i in 0..n {
        if !visited[i] {
            walk(i, 0, &mut trail, true, &children, &mut visited, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (number, parent) pairs → rendered rows `"<prefix>#<number>"`.
    fn rows(spec: &[(u64, Option<u64>)]) -> Vec<String> {
        nest_by(spec.len(), |i| spec[i].0, |i| spec[i].1)
            .into_iter()
            .map(|(i, pos)| format!("{}#{}", pos.prefix, spec[i].0))
            .collect()
    }

    #[test]
    fn flat_list_keeps_order() {
        assert_eq!(rows(&[(3, None), (2, None), (1, None)]), ["#3", "#2", "#1"]);
    }

    #[test]
    fn children_follow_their_parent_with_guides() {
        let spec = [
            (5, None),
            (4, Some(5)),
            (3, None),
            (2, Some(5)),
            (1, Some(2)),
        ];
        assert_eq!(rows(&spec), ["#5", "├─ #4", "└─ #2", "   └─ #1", "#3"]);
    }

    #[test]
    fn guides_continue_past_siblings_with_children() {
        let spec = [(9, None), (8, Some(9)), (7, Some(8)), (6, Some(9))];
        assert_eq!(rows(&spec), ["#9", "├─ #8", "│  └─ #7", "└─ #6"]);
    }

    #[test]
    fn orphans_and_self_parent_are_top_level() {
        assert_eq!(rows(&[(2, Some(99)), (1, Some(1))]), ["#2", "#1"]);
    }

    #[test]
    fn cycles_do_not_drop_items() {
        // 1 → 2 → 1 plus a child hanging off the cycle: both cycle members
        // are top-level, the child still nests under its parent.
        let spec = [(1, Some(2)), (2, Some(1)), (3, Some(2))];
        assert_eq!(rows(&spec), ["#1", "#2", "└─ #3"]);
        // A longer cycle behaves the same.
        let spec = [(1, Some(3)), (2, Some(1)), (3, Some(2))];
        assert_eq!(rows(&spec), ["#1", "#2", "#3"]);
    }
}
