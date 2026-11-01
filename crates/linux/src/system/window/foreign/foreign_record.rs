use super::foreign_flags::ToplevelFlags;

/// One open toplevel as the compositor last described it in a `done` batch.
/// `id` is the handle's protocol object id, stable for the connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) struct ToplevelRecord {
    pub(in crate::system) id: u32,
    pub(in crate::system) title: String,
    pub(in crate::system) app_id: String,
    pub(in crate::system) flags: ToplevelFlags,
}
