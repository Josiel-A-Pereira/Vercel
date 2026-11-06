//! What the compositor has said on the virtual keyboard connection.

use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::{
    self, ZwpVirtualKeyboardManagerV1,
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::{
    self, ZwpVirtualKeyboardV1,
};

use crate::system::display::wayland_link::SyncDone;

pub(super) const MANAGER_INTERFACE: &str = "zwp_virtual_keyboard_manager_v1";
pub(super) const SEAT_INTERFACE: &str = "wl_seat";

/// The registry name and version of the two globals a keyboard needs, whether
/// the seat announced a keyboard before ours existed, and whether the last
/// sync callback fired.
#[derive(Default)]
pub(super) struct VkState {
    pub(super) manager: Option<(u32, u32)>,
    pub(super) seat: Option<(u32, u32)>,
    pub(super) seat_keyboard: bool,
    pub(super) alone: bool,
    synced: bool,
}

impl SyncDone for VkState {
    fn sync_done(&mut self) -> &mut bool {
        &mut self.synced
    }
}

impl Dispatch<WlRegistry, ()> for VkState {
    fn event(
        state: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            MANAGER_INTERFACE => state.manager = Some((name, version)),
            SEAT_INTERFACE if state.seat.is_none() => state.seat = Some((name, version)),
            _ => {}
        }
    }
}

impl Dispatch<WlCallback, ()> for VkState {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.synced = true;
        }
    }
}

impl Dispatch<WlSeat, ()> for VkState {
    fn event(
        state: &mut Self,
        _: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            state.seat_keyboard = matches!(
                capabilities,
                WEnum::Value(held) if held.contains(wl_seat::Capability::Keyboard)
            );
        }
    }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for VkState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardManagerV1,
        _: zwp_virtual_keyboard_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for VkState {
    fn event(
        _: &mut Self,
        _: &ZwpVirtualKeyboardV1,
        _: zwp_virtual_keyboard_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}
