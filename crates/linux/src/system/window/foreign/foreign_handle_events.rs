use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    self, ZwlrForeignToplevelHandleV1,
};

use super::foreign_flags::ToplevelFlags;
use super::foreign_state::ForeignState;

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for ForeignState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Event;
        let Some(id) = state.roster.serial_of(handle) else {
            return;
        };
        match event {
            Event::Title { title } => {
                if let Some(tracked) = state.roster.tracked_mut(id) {
                    tracked.pending.title = Some(title);
                }
            }
            Event::AppId { app_id } => {
                if let Some(tracked) = state.roster.tracked_mut(id) {
                    tracked.pending.app_id = Some(app_id);
                }
            }
            Event::State { state: words } => {
                if let Some(tracked) = state.roster.tracked_mut(id) {
                    tracked.pending.flags = Some(ToplevelFlags::from_state_words(&words));
                }
            }
            Event::Done => state.roster.settle(id),
            Event::Closed => state.forget(id),
            _ => {}
        }
    }
}
