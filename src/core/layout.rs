use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// Direction for splitting a layout node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

impl From<SplitDirection> for Direction {
    fn from(d: SplitDirection) -> Self {
        match d {
            SplitDirection::Horizontal => Direction::Horizontal,
            SplitDirection::Vertical => Direction::Vertical,
        }
    }
}

/// A tree node describing the layout structure.
///
/// - `Pane(id)` — a leaf that always maps to a fixed pane ID.
/// - `Slot(slot_id)` — a leaf whose pane ID is resolved at render time.
/// - `Split` — a container that divides its area among children.
#[derive(Debug, Clone)]
pub enum LayoutNode {
    Pane(usize),
    Slot(usize),
    Split {
        direction: SplitDirection,
        children: Vec<(Constraint, LayoutNode)>,
    },
}

/// Rule for resolving a `Slot` to a concrete pane at render time.
///
/// If the currently focused pane is in `trigger_panes`, the slot resolves to
/// `then_pane`; otherwise it resolves to `default_pane`.
pub struct SlotRule {
    pub slot_id: usize,
    pub trigger_panes: Vec<usize>,
    pub then_pane: usize,
    pub default_pane: usize,
}

/// Bundles a layout tree with its navigation and slot-resolution config.
///
/// This is the single source of truth for how a page's panes are arranged,
/// cycled, and dynamically resolved — everything a future config file needs.
pub struct PageLayoutConfig {
    pub tree: LayoutNode,
    pub tab_panes: Vec<usize>,
    pub slot_rules: Vec<SlotRule>,
}

impl PageLayoutConfig {
    /// Resolve all slots based on the currently focused pane.
    pub fn resolve_slots(&self, focused_pane: usize) -> Vec<(usize, usize)> {
        self.slot_rules
            .iter()
            .map(|r| {
                let pane = if r.trigger_panes.contains(&focused_pane) {
                    r.then_pane
                } else {
                    r.default_pane
                };
                (r.slot_id, pane)
            })
            .collect()
    }
}

/// The common page frame: header (1 line) + content + status bar (1 line).
pub struct PageFrame {
    pub header: Rect,
    pub content: Rect,
    pub status_bar: Rect,
}

/// Split an area into the standard page frame.
pub fn split_page_frame(area: Rect) -> PageFrame {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);

    PageFrame {
        header: chunks[0],
        content: chunks[1],
        status_bar: chunks[2],
    }
}

/// Recursively resolve a layout tree into a flat list of `(pane_id, Rect)`.
///
/// `slots` maps `slot_id` → `pane_id` for dynamic panes.
pub fn resolve_layout(
    area: Rect,
    tree: &LayoutNode,
    slots: &[(usize, usize)],
) -> Vec<(usize, Rect)> {
    let mut result = Vec::new();
    resolve_node(area, tree, slots, &mut result);
    result
}

fn resolve_node(
    area: Rect,
    node: &LayoutNode,
    slots: &[(usize, usize)],
    out: &mut Vec<(usize, Rect)>,
) {
    match node {
        LayoutNode::Pane(id) => {
            out.push((*id, area));
        }
        LayoutNode::Slot(slot_id) => {
            let pane_id = slots
                .iter()
                .find(|(s, _)| s == slot_id)
                .map(|(_, p)| *p)
                .unwrap_or(*slot_id);
            out.push((pane_id, area));
        }
        LayoutNode::Split {
            direction,
            children,
        } => {
            let constraints: Vec<Constraint> = children.iter().map(|(c, _)| *c).collect();
            let chunks = Layout::default()
                .direction(Direction::from(*direction))
                .constraints(constraints)
                .split(area);
            for (i, (_, child)) in children.iter().enumerate() {
                resolve_node(chunks[i], child, slots, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    #[test]
    fn single_pane_fills_area() {
        let area = Rect::new(0, 0, 80, 24);
        let tree = LayoutNode::Pane(42);
        let result = resolve_layout(area, &tree, &[]);
        assert_eq!(result, vec![(42, area)]);
    }

    #[test]
    fn horizontal_split() {
        let area = Rect::new(0, 0, 100, 24);
        let tree = LayoutNode::Split {
            direction: SplitDirection::Horizontal,
            children: vec![
                (Constraint::Percentage(50), LayoutNode::Pane(0)),
                (Constraint::Percentage(50), LayoutNode::Pane(1)),
            ],
        };
        let result = resolve_layout(area, &tree, &[]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[1].0, 1);
        // Both should have same height, widths should sum to 100
        assert_eq!(result[0].1.height, 24);
        assert_eq!(result[1].1.height, 24);
        assert_eq!(result[0].1.width + result[1].1.width, 100);
    }

    #[test]
    fn nested_split() {
        let area = Rect::new(0, 0, 80, 40);
        let tree = LayoutNode::Split {
            direction: SplitDirection::Vertical,
            children: vec![
                (
                    Constraint::Percentage(50),
                    LayoutNode::Split {
                        direction: SplitDirection::Horizontal,
                        children: vec![
                            (Constraint::Percentage(50), LayoutNode::Pane(0)),
                            (Constraint::Percentage(50), LayoutNode::Pane(1)),
                        ],
                    },
                ),
                (Constraint::Percentage(50), LayoutNode::Pane(2)),
            ],
        };
        let result = resolve_layout(area, &tree, &[]);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[1].0, 1);
        assert_eq!(result[2].0, 2);
        // Top row panes should be side by side
        assert_eq!(result[0].1.y, result[1].1.y);
        // Bottom pane should be below top row
        assert!(result[2].1.y > result[0].1.y);
    }

    #[test]
    fn slot_resolution() {
        let area = Rect::new(0, 0, 80, 24);
        let tree = LayoutNode::Slot(0);
        let result = resolve_layout(area, &tree, &[(0, 99)]);
        assert_eq!(result, vec![(99, area)]);
    }

    #[test]
    fn slot_rule_trigger() {
        let config = PageLayoutConfig {
            tree: LayoutNode::Slot(0),
            tab_panes: vec![0, 1],
            slot_rules: vec![SlotRule {
                slot_id: 0,
                trigger_panes: vec![1, 3],
                then_pane: 2,
                default_pane: 4,
            }],
        };
        // Focused on trigger pane → then_pane
        assert_eq!(config.resolve_slots(1), vec![(0, 2)]);
        assert_eq!(config.resolve_slots(3), vec![(0, 2)]);
        // Focused on non-trigger pane → default_pane
        assert_eq!(config.resolve_slots(0), vec![(0, 4)]);
        assert_eq!(config.resolve_slots(99), vec![(0, 4)]);
    }

    #[test]
    fn multiple_slot_rules() {
        let config = PageLayoutConfig {
            tree: LayoutNode::Pane(0),
            tab_panes: vec![],
            slot_rules: vec![
                SlotRule {
                    slot_id: 0,
                    trigger_panes: vec![1],
                    then_pane: 10,
                    default_pane: 20,
                },
                SlotRule {
                    slot_id: 1,
                    trigger_panes: vec![2],
                    then_pane: 30,
                    default_pane: 40,
                },
            ],
        };
        assert_eq!(config.resolve_slots(1), vec![(0, 10), (1, 40)]);
        assert_eq!(config.resolve_slots(2), vec![(0, 20), (1, 30)]);
    }

    #[test]
    fn page_frame_splits_correctly() {
        let area = Rect::new(0, 0, 80, 24);
        let frame = split_page_frame(area);
        assert_eq!(frame.header.height, 1);
        assert_eq!(frame.status_bar.height, 1);
        assert_eq!(frame.content.height, 22);
        assert_eq!(frame.header.y, 0);
        assert_eq!(frame.content.y, 1);
        assert_eq!(frame.status_bar.y, 23);
    }
}
