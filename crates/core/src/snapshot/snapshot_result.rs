use crate::{AccessibilityNode, AppError, WindowInfo, refs::RefMap, refs::refs_store::RefStore};

#[derive(Clone)]
pub struct SnapshotResult {
    pub tree: AccessibilityNode,
    pub refmap: RefMap,
    pub window: WindowInfo,
    pub snapshot_id: Option<String>,
    pub complete: bool,
    pub nodes_observed: usize,
    /// Set when the whole-window read that proves tokenless refs failed or was
    /// cut short; it says why, and those refs are refused when used.
    pub identity_unproven: Option<String>,
}

impl SnapshotResult {
    pub(crate) fn save_refmap(&self, store: &RefStore) -> Result<String, AppError> {
        if self.complete {
            store.save_new_snapshot(&self.refmap)
        } else {
            store.save_new_snapshot_detached(&self.refmap)
        }
    }

    pub(crate) fn bind_snapshot_id(&mut self, snapshot_id: String) {
        crate::refs::ref_token::qualify_tree_refs(&mut self.tree, &snapshot_id);
        self.snapshot_id = Some(snapshot_id);
    }
}
