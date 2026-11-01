use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, Rect, WindowInfo, WindowOp};

use super::OpWorld;
use super::window_place::PlaceStore;
use crate::system::display::display_server::{DisplayServer, session_server};
use crate::system::focus::focus_window::FocusWorld;
use crate::system::focus::focus_world::LiveWorld as FrameWorld;
use crate::system::focus::restore_minimized::{Lifted, lift};
use crate::system::process::process_lookup::Lookup;
use crate::system::window::foreign::foreign_op;
use crate::system::window::foreign::foreign_route::through_link;
use crate::system::window::window_client::ClientWorld;
use crate::system::x11::control::control_error::ControlError;
use crate::system::x11::control::control_send::{WindowWrite, write_window};
use crate::system::x11::control::window_facts::{WindowFacts, read_facts};
use crate::system::x11::control::{ControlRead, xwayland_clients};
use crate::system::x11::stacking::stacking_source::StackingSource;
use crate::system::x11::twin_tie::Twins;
use crate::system::x11::{DisplayReader, X11Client, X11State};
use crate::system::{
    SharedBus, list_displays,
    process::{proc_stat, process_facts},
};
use crate::tree::Extents;

/// The live desktop a window operation reads and writes.
pub(super) struct Live<'a> {
    frame: FrameWorld<'a>,
    owner: agent_desktop_core::ProcessIdentity,
    deadline: Deadline,
}

impl<'a> Live<'a> {
    pub(super) fn new(
        bus: &'a SharedBus,
        win: &WindowInfo,
        deadline: Deadline,
    ) -> Result<Self, AdapterError> {
        let frame = FrameWorld::new(bus, win, deadline)
            .map_err(|error| error.with_disposition(DeliverySemantics::not_delivered()))?;
        let owner = agent_desktop_core::ProcessIdentity::new(
            win.pid,
            win.process_instance.clone().unwrap_or_default(),
        );
        Ok(Self {
            frame,
            owner,
            deadline,
        })
    }
}

impl ClientWorld for Live<'_> {
    fn global_position(&self) -> bool {
        self.frame.global_position()
    }

    fn twins(&self, win: &WindowInfo) -> Option<Twins> {
        self.frame.twins(win)
    }
}

impl OpWorld for Live<'_> {
    fn lookup(&self, pid: u32) -> Lookup {
        process_facts::lookup(pid)
    }

    fn stopped(&self, pid: u32) -> bool {
        proc_stat::is_stopped(pid)
    }

    fn clients(&self) -> ControlRead<Vec<X11Client>> {
        match session_server() {
            DisplayServer::X11(_) => match DisplayReader.clients_within(self.deadline) {
                X11State::Ready { clients, .. } => ControlRead::Ready(clients),
                X11State::NoWindowManager => ControlRead::NoManager,
                X11State::Unavailable => ControlRead::Unavailable,
            },
            DisplayServer::Wayland => xwayland_clients(self.deadline),
            DisplayServer::Unavailable(..) => ControlRead::Unavailable,
        }
    }

    fn active_client(&self) -> Option<u32> {
        self.frame.active()
    }

    fn on_wayland(&self) -> bool {
        session_server() == DisplayServer::Wayland
    }

    fn facts(&self, client: u32) -> ControlRead<WindowFacts> {
        read_facts(client, &self.owner, self.deadline)
    }

    fn write(&self, client: u32, write: &WindowWrite) -> Result<(), ControlError> {
        write_window(client, write, &self.owner, self.deadline)
    }

    fn lift(&self, client: u32) -> Result<Lifted, AdapterError> {
        lift(client, &self.owner, self.deadline)
    }

    fn overview_open(&self) -> Option<bool> {
        self.frame.overview_open()
    }

    fn remember(&self, win: &WindowInfo, client: u32, rectangle: Option<Extents>) {
        let (Some(store), Some(instance)) = (
            PlaceStore::in_runtime_dir(),
            win.process_instance.as_deref(),
        ) else {
            return;
        };
        match rectangle {
            Some(place) => {
                let alive = |pid: u32, instance: &str| matches!(process_facts::lookup(pid), Lookup::Found(facts) if facts.instance == instance);
                let _ = store.save(client, win.pid.get(), instance, place, &alive);
            }
            None => store.forget(client, instance),
        }
    }

    fn remembered(&self, win: &WindowInfo, client: u32) -> Option<Extents> {
        PlaceStore::in_runtime_dir()?.load(client, win.process_instance.as_deref()?)
    }

    fn foreign_op(&self, win: &WindowInfo, op: &WindowOp) -> Option<Result<(), AdapterError>> {
        through_link(win, self.deadline, |link, target, deadline| {
            foreign_op::apply(link, target, op, deadline)
        })
    }

    fn displays(&self) -> Vec<Rect> {
        list_displays(self.deadline)
            .map(|displays| displays.into_iter().map(|display| display.bounds).collect())
            .unwrap_or_default()
    }
}
