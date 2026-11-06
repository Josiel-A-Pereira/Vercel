//! Keys on wlroots compositors through `zwp_virtual_keyboard_v1`.
//!
//! The client prints its own keymap text, so nothing is loaded at run time.
//! Held key state never reaches the wire: a session writes only balanced
//! groups, because the compositor releases nothing when a client dies.

pub(crate) mod base_key;
pub(crate) mod extras;
pub(crate) mod group_builder;
pub(crate) mod keymap_file;
pub(crate) mod keymap_text;
pub(crate) mod locate;
pub(crate) mod modifier_mask;
pub(crate) mod typable;
pub(crate) mod vk_link;
pub(crate) mod vk_port;
pub(crate) mod vk_send;
pub(crate) mod vk_session;
pub(crate) mod vk_state;
pub(crate) mod wire_op;

#[cfg(test)]
mod vk_fake;

#[cfg(test)]
mod vk_live_hazard_tests;
#[cfg(test)]
mod vk_live_modifiers;
#[cfg(test)]
mod vk_live_rig;
#[cfg(test)]
mod vk_live_tests;
