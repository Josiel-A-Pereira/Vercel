use std::cell::RefCell;
use std::collections::HashMap;
use std::os::unix::net::UnixStream;

use agent_desktop_core::{AdapterError, Deadline};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, EventQueue};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use super::foreign_hypr_ipc;
use super::foreign_hypr_mapping;
use super::foreign_hypr_protocol::hyprland_toplevel_mapping_manager_v1::HyprlandToplevelMappingManagerV1;
use super::foreign_port::ToplevelPort;
use super::foreign_record::ToplevelRecord;
use super::foreign_request::ToplevelRequest;
use super::foreign_state::ForeignState;
use crate::system::display::display_env::DisplayEnv;
use crate::system::display::wayland_link::{connect, roundtrip};
use crate::system::x11::control::control_error::ControlError;

const MANAGER_INTERFACE: &str = "zwlr_foreign_toplevel_manager_v1";
const SEAT_INTERFACE: &str = "wl_seat";
const MANAGER_VERSION: u32 = 3;
const SEAT_VERSION: u32 = 1;

/// A live connection to the compositor's foreign toplevel manager.
pub(in crate::system) struct ForeignLink {
    connection: Connection,
    queue: RefCell<EventQueue<ForeignState>>,
    state: RefCell<ForeignState>,
    _manager: ZwlrForeignToplevelManagerV1,
    mapping: Option<HyprlandToplevelMappingManagerV1>,
}

impl ForeignLink {
    /// Connects and reads the toplevels already open, or `None` when the
    /// session has no compositor socket or the compositor offers no foreign
    /// toplevel manager. Only the deadline itself is an error.
    pub(in crate::system) fn open(deadline: Deadline) -> Result<Option<Self>, AdapterError> {
        let Some(stream) = connect(&DisplayEnv::from_process()) else {
            return Ok(None);
        };
        Self::open_on(stream, deadline)
    }

    fn open_on(stream: UnixStream, deadline: Deadline) -> Result<Option<Self>, AdapterError> {
        let Ok(connection) = Connection::from_socket(stream) else {
            return Ok(None);
        };
        let mut queue = connection.new_event_queue::<ForeignState>();
        let qh = queue.handle();
        let mut state = ForeignState::default();
        let registry = connection.display().get_registry(&qh, ());
        if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
            return Ok(None);
        }
        let Some(manager_global) = state
            .globals
            .iter()
            .find(|global| global.interface == MANAGER_INTERFACE)
            .cloned()
        else {
            return Ok(None);
        };
        let manager: ZwlrForeignToplevelManagerV1 = registry.bind(
            manager_global.name,
            manager_global.version.min(MANAGER_VERSION),
            &qh,
            (),
        );
        let mapping = state
            .globals
            .iter()
            .find(|global| global.interface == "hyprland_toplevel_mapping_manager_v1")
            .map(|global| registry.bind(global.name, 1, &qh, ()));
        if let Some(seat_global) = state
            .globals
            .iter()
            .find(|global| global.interface == SEAT_INTERFACE)
        {
            let seat: WlSeat = registry.bind(
                seat_global.name,
                seat_global.version.min(SEAT_VERSION),
                &qh,
                (),
            );
            state.seat = Some(seat);
        }
        if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
            return Ok(None);
        }
        Ok(Some(Self {
            connection,
            queue: RefCell::new(queue),
            state: RefCell::new(state),
            _manager: manager,
            mapping,
        }))
    }
}

/// The answer when the mapping global exists but the compositor's client list
/// cannot be read: no toplevel has a proven owner, so every one is unproven
/// instead of falling back to the title and app id heuristic.
fn unreadable_clients(expired: bool) -> Result<Option<HashMap<u32, u32>>, ControlError> {
    if expired {
        Err(ControlError::TimedOut)
    } else {
        Ok(Some(HashMap::new()))
    }
}

impl ToplevelPort for ForeignLink {
    fn refresh(&self, deadline: Deadline) -> Result<(), ControlError> {
        let mut queue = self.queue.borrow_mut();
        let mut state = self.state.borrow_mut();
        match roundtrip(&self.connection, &mut queue, &mut state, deadline) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ControlError::NoDisplay),
            Err(_) => Err(ControlError::TimedOut),
        }
    }

    fn records(&self) -> Vec<ToplevelRecord> {
        self.state.borrow().records()
    }

    fn owners(&self, deadline: Deadline) -> Result<Option<HashMap<u32, u32>>, ControlError> {
        let Some(manager) = &self.mapping else {
            return Ok(None);
        };
        let Some(clients) = foreign_hypr_ipc::clients(deadline) else {
            return unreadable_clients(deadline.is_expired());
        };
        let mut queue = self.queue.borrow_mut();
        let mut state = self.state.borrow_mut();
        state.addresses.clear();
        let handles: Vec<(u32, ZwlrForeignToplevelHandleV1)> = state
            .roster
            .shown_handles()
            .into_iter()
            .map(|(serial, handle)| (serial, handle.clone()))
            .collect();
        for (serial, handle) in &handles {
            manager.get_window_for_toplevel_wlr(handle, &queue.handle(), *serial);
        }
        match roundtrip(&self.connection, &mut queue, &mut state, deadline) {
            Ok(true) => {}
            Ok(false) => return Err(ControlError::NoDisplay),
            Err(_) => return Err(ControlError::TimedOut),
        }
        let Some(after) = foreign_hypr_ipc::clients(deadline) else {
            return if deadline.is_expired() {
                Err(ControlError::TimedOut)
            } else {
                Err(ControlError::NoDisplay)
            };
        };
        let clients = foreign_hypr_mapping::stable_clients(&clients, &after);
        let addresses = handles
            .iter()
            .filter_map(|(id, handle)| {
                if !state
                    .handle_of(*id)
                    .is_some_and(|current| current == handle)
                {
                    return None;
                }
                state.addresses.get(id).map(|address| (*id, *address))
            })
            .collect();
        Ok(Some(foreign_hypr_mapping::join(&addresses, &clients)))
    }

    fn address(&self, id: u32) -> Option<u64> {
        self.state.borrow().addresses.get(&id).copied()
    }

    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        {
            let state = self.state.borrow();
            let handle = state.handle_of(id).ok_or(ControlError::BadWindow)?;
            match request {
                ToplevelRequest::Activate => {
                    let seat = state.seat.as_ref().ok_or(ControlError::NoDisplay)?;
                    handle.activate(seat);
                }
                ToplevelRequest::Close => handle.close(),
                ToplevelRequest::SetMaximized(true) => handle.set_maximized(),
                ToplevelRequest::SetMaximized(false) => handle.unset_maximized(),
                ToplevelRequest::SetMinimized(true) => handle.set_minimized(),
                ToplevelRequest::SetMinimized(false) => handle.unset_minimized(),
            }
        }
        self.connection
            .flush()
            .map_err(|_| ControlError::SendFailed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::window::foreign::foreign_fake::record;
    use crate::system::window::foreign::foreign_names::ProcessNames;
    use crate::system::window::foreign::foreign_proof::{Proof, prove_owned};

    #[test]
    fn an_unreadable_client_list_with_the_mapping_global_proves_nothing() {
        let owners = unreadable_clients(false).unwrap();
        assert!(owners.is_some());
        let names = ProcessNames::from_names(vec!["thunar".to_owned()]);
        let proof = prove_owned(
            "Home",
            &names,
            &[record(1, "Home", "thunar")],
            owners.as_ref(),
        );
        assert!(matches!(proof, Proof::Unrelated(_)), "{proof:?}");
        assert!(unreadable_clients(true).is_err());
    }
}
