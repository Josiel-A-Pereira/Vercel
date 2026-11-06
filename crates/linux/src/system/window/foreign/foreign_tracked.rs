use super::foreign_pending::Pending;
use super::foreign_record::ToplevelRecord;

/// One handle the compositor announced: the proxy, the serial this
/// connection gave it, the last completed batch and the events held for the
/// next `done`.
pub(super) struct Tracked<H> {
    pub(super) serial: u32,
    pub(super) handle: H,
    pub(super) shown: Option<ToplevelRecord>,
    pub(super) pending: Pending,
}
