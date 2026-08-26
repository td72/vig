use crate::core::pane::{Pane, PaneEvent};

/// A generic tab that pairs a list pane with a detail pane.
///
/// Many views follow the list+detail pattern: a list of items on one side
/// and a detail view for the selected item on the other. This struct
/// captures that structural pairing, with specialized behavior added
/// via `impl Tab<ConcreteList, ConcreteDetail>` blocks.
pub struct Tab<L, D> {
    pub list: L,
    pub detail: D,
}

impl<L: Pane<PaneEvent>, D: Pane<PaneEvent>> Tab<L, D> {
    /// Look up a pane by its index. Returns `Some` if `idx` matches
    /// either `list_id` or `detail_id`.
    pub fn get_pane_mut(
        &mut self,
        list_id: usize,
        detail_id: usize,
        idx: usize,
    ) -> Option<&mut dyn Pane<PaneEvent>> {
        if idx == list_id {
            Some(&mut self.list)
        } else if idx == detail_id {
            Some(&mut self.detail)
        } else {
            None
        }
    }
}
