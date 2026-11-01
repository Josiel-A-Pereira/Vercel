use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::{self, WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    self, EVT_TOPLEVEL_OPCODE, ZwlrForeignToplevelManagerV1,
};

use super::foreign_record::ToplevelRecord;
use super::foreign_tracked::Tracked;
use crate::system::display::display_wayland_state::Global;
use crate::system::display::wayland_link::SyncDone;

/// What the compositor has said about its toplevels so far, filled in by the
/// event handlers while the connection is dispatched.
#[derive(Default)]
pub(super) struct ForeignState {
    pub(super) globals: Vec<Global>,
    pub(super) seat: Option<WlSeat>,
    pub(super) tracked: Vec<Tracked>,
    synced: bool,
}

impl SyncDone for ForeignState {
    fn sync_done(&mut self) -> &mut bool {
        &mut self.synced
    }
}

impl ForeignState {
    pub(super) fn records(&self) -> Vec<ToplevelRecord> {
        self.tracked
            .iter()
            .filter_map(|tracked| tracked.shown.clone())
            .collect()
    }

    pub(super) fn handle_of(&self, id: u32) -> Option<&ZwlrForeignToplevelHandleV1> {
        self.tracked
            .iter()
            .find(|tracked| tracked.handle.id().protocol_id() == id && tracked.shown.is_some())
            .map(|tracked| &tracked.handle)
    }

    pub(super) fn tracked_mut(&mut self, id: u32) -> Option<&mut Tracked> {
        self.tracked
            .iter_mut()
            .find(|tracked| tracked.handle.id().protocol_id() == id)
    }

    pub(super) fn settle(&mut self, id: u32) {
        if let Some(tracked) = self.tracked_mut(id) {
            let record = tracked.pending.settle(id, tracked.shown.as_ref());
            tracked.shown = Some(record);
        }
    }

    pub(super) fn forget(&mut self, id: u32) {
        let Some(at) = self
            .tracked
            .iter()
            .position(|tracked| tracked.handle.id().protocol_id() == id)
        else {
            return;
        };
        let gone = self.tracked.remove(at);
        gone.handle.destroy();
    }
}

impl Dispatch<WlRegistry, ()> for ForeignState {
    fn event(
        state: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.globals.push(Global {
                name,
                interface,
                version,
            });
        }
    }
}

impl Dispatch<WlCallback, ()> for ForeignState {
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

impl Dispatch<WlSeat, ()> for ForeignState {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: wl_seat::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for ForeignState {
    fn event(
        state: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.tracked.push(Tracked {
                handle: toplevel,
                shown: None,
                pending: Default::default(),
            });
        }
    }

    event_created_child!(ForeignState, ZwlrForeignToplevelManagerV1, [
        EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ())
    ]);
}

#[cfg(test)]
#[path = "foreign_state_tests.rs"]
mod tests;
