//! Window control through the compositor's `zwlr_foreign_toplevel_manager_v1`
//! protocol, for sessions where the accessibility tree names windows but no
//! X11 server can address them.

mod foreign_await;
mod foreign_awaited;
pub(in crate::system) mod foreign_close;
pub(in crate::system) mod foreign_close_route;
pub(in crate::system) mod foreign_flags;
pub(in crate::system) mod foreign_focus;
pub(in crate::system) mod foreign_frame;
mod foreign_handle_events;
pub(in crate::system) mod foreign_link;
mod foreign_locate;
pub(in crate::system) mod foreign_names;
pub(in crate::system) mod foreign_op;
mod foreign_pending;
pub(in crate::system) mod foreign_port;
pub(in crate::system) mod foreign_proof;
pub(in crate::system) mod foreign_record;
pub(in crate::system) mod foreign_refusal;
pub(in crate::system) mod foreign_request;
pub(in crate::system) mod foreign_route;
mod foreign_state;
pub(in crate::system) mod foreign_target;
mod foreign_tracked;

#[cfg(test)]
mod foreign_fake;
#[cfg(test)]
mod foreign_live_rig;
#[cfg(test)]
mod foreign_live_tests;
