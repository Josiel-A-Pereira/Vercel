pub(crate) mod active_watch;
mod embedded_group;
pub(in crate::system) mod foreign;
pub(crate) use foreign::foreign_active::{compositor_active, offers_window_control, watch_active};
#[cfg(test)]
pub(crate) use foreign::foreign_live_rig;
pub(crate) mod frame_identity;
pub(super) mod ghost_window;
pub(crate) mod listed_twins;
pub(crate) mod native_group;
#[cfg(test)]
pub(super) mod stopped_live_tests;
pub(super) mod stopped_windows;
pub(super) mod window_client;
pub(crate) mod window_filter;
pub(crate) mod window_id;
pub(super) mod window_ids;
pub(super) mod window_listing;
pub(super) mod window_op;
#[cfg(test)]
pub(super) mod window_op_live_tests;
pub(super) mod window_ops;
pub(super) mod window_order;
pub(crate) mod window_position;
pub(super) mod window_report;
pub(super) mod window_resolve;
