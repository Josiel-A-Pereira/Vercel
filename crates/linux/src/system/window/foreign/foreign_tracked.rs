use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

use super::foreign_pending::Pending;
use super::foreign_record::ToplevelRecord;

/// One handle the compositor announced: the proxy, the last completed batch
/// and the events held for the next `done`.
pub(super) struct Tracked {
    pub(super) handle: ZwlrForeignToplevelHandleV1,
    pub(super) shown: Option<ToplevelRecord>,
    pub(super) pending: Pending,
}
