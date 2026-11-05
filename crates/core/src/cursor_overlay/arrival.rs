use crate::ProcessId;
use serde::{Deserialize, Serialize};

/// What a cursor travel shows on arrival and which process owns the window it
/// lands in. A ref action names the owner, so a renderer can draw the overlay
/// directly above the target's window and windows in front of the target cover
/// it. A travel to raw coordinates has none, and the overlay draws above every
/// window.
#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CursorArrival {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pointer: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<ProcessId>,
}

impl CursorArrival {
    /// `pointer` arrives showing the pointer image, because the action lands
    /// on a pressable control.
    pub const fn new(pointer: bool, owner: Option<ProcessId>) -> Self {
        Self { pointer, owner }
    }

    pub const fn pointer(&self) -> bool {
        self.pointer
    }

    pub const fn owner(&self) -> Option<ProcessId> {
        self.owner
    }

    pub(crate) const fn is_plain(&self) -> bool {
        !self.pointer && self.owner.is_none()
    }
}
