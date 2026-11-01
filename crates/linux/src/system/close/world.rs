use std::time::{Duration, Instant};

use agent_desktop_core::{AdapterError, Deadline};

use crate::system::display::display_server::{DisplayServer, session_server};
use crate::system::menu::surfaces;
use crate::system::process::liveness::{self, Liveness};
use crate::system::process::process_lookup::Lookup;
use crate::system::signal::Signal;
use crate::system::signal::signal_sent::Sent;
use crate::system::window::foreign::foreign_close_route::ForeignCloseRoute;
use crate::system::window::foreign::foreign_frame::Frame;
use crate::system::window::foreign::foreign_link::ForeignLink;
use crate::system::window::foreign::foreign_names::ProcessNames;
use crate::system::window::window_op::window_op_verdict::POLL;
use crate::system::window::window_ops::listed_of_process;
use crate::system::x11::control;
use crate::system::x11::control::ControlRead;
use crate::system::x11::control::control_error::ControlError;
use crate::system::x11::window::toplevel::Toplevel;
use crate::system::{
    process::{process_facts, protected},
    signal,
};

const LATE_READ_MS: u64 = 300;
const NESTED_LOOK_MS: u64 = 300;

/// What the close flow touches outside itself. The live implementation reads
/// `/proc`, talks to the display server and signals through a descriptor; a
/// test supplies a process and a clock of its own.
pub(super) trait World {
    fn is_protected(&self) -> Result<bool, AdapterError>;
    fn facts(&self) -> Lookup;
    fn liveness(&self) -> Liveness;
    fn elapsed(&self) -> Duration;
    fn remaining(&self) -> Duration;
    fn pause(&self);
    fn toplevels(&self) -> ControlRead<Vec<Toplevel>>;
    fn toplevels_late(&self) -> ControlRead<Vec<Toplevel>>;
    fn nested_dialogs(&self) -> Vec<String>;
    fn request_close(&self, window: u32) -> Result<(), ControlError>;
    fn signal(&self, signal: Signal) -> Result<Sent, AdapterError>;
    fn timeout_error(&self) -> AdapterError;
    fn on_wayland(&self) -> bool;

    /// How many windows the last read could not prove were the process's own.
    fn contested_windows(&self) -> usize {
        0
    }

    /// Whether the windows were read from the compositor's toplevel list.
    fn has_foreign_route(&self) -> bool {
        false
    }
}

pub(super) struct Live<'a> {
    pub(super) bus: &'a crate::system::SharedBus,
    pub(super) pid: u32,
    pub(super) name: &'a str,
    pub(super) instance: &'a str,
    pub(super) deadline: Deadline,
    pub(super) started: Instant,
    pub(super) foreign: ForeignCloseRoute<ForeignLink>,
}

impl Live<'_> {
    /// The X11 answer, unless this is a Wayland session where that answer
    /// holds no window and the compositor lists toplevels itself.
    fn read_toplevels(&self, deadline: Deadline) -> ControlRead<Vec<Toplevel>> {
        let x11 = control::toplevels_of(self.pid, deadline);
        let listed = matches!(&x11, ControlRead::Ready(windows) if !windows.is_empty());
        if listed || !self.on_wayland() {
            self.foreign.release();
            return x11;
        }
        match self.foreign.reach(|| ForeignLink::open(deadline)) {
            Ok(true) => {}
            Ok(false) => return x11,
            Err(_) => return ControlRead::Unavailable,
        }
        let names = ProcessNames::of(self.pid, self.name);
        let frames = || {
            listed_of_process(self.bus, self.pid, deadline)
                .map(|windows| {
                    windows
                        .into_iter()
                        .map(|window| Frame {
                            title: window.info.title,
                            prompt: window.dialog || window.modal,
                        })
                        .collect()
                })
                .map_err(|_| ControlError::NoDisplay)
        };
        match self.foreign.standing(&names, frames, deadline) {
            Ok(windows) => ControlRead::Ready(windows),
            Err(_) => ControlRead::Unavailable,
        }
    }
}

impl World for Live<'_> {
    fn is_protected(&self) -> Result<bool, AdapterError> {
        protected::is_protected(self.bus, self.pid, self.instance, self.deadline)
    }

    fn facts(&self) -> Lookup {
        process_facts::lookup(self.pid)
    }

    fn liveness(&self) -> Liveness {
        liveness::liveness(self.pid, self.instance)
    }

    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    fn remaining(&self) -> Duration {
        self.deadline.remaining()
    }

    fn pause(&self) {
        std::thread::sleep(self.deadline.remaining().min(POLL));
    }

    fn toplevels(&self) -> ControlRead<Vec<Toplevel>> {
        self.read_toplevels(self.deadline)
    }

    fn toplevels_late(&self) -> ControlRead<Vec<Toplevel>> {
        match Deadline::detached_after(LATE_READ_MS) {
            Ok(deadline) => self.read_toplevels(deadline),
            Err(_) => ControlRead::Unavailable,
        }
    }

    fn nested_dialogs(&self) -> Vec<String> {
        let look = self.deadline.capped(Duration::from_millis(NESTED_LOOK_MS));
        surfaces::nested_dialog_titles(self.bus, self.pid, look).unwrap_or_default()
    }

    fn request_close(&self, window: u32) -> Result<(), ControlError> {
        if self.foreign.in_use() {
            return self.foreign.close(window, self.deadline);
        }
        control::request_close(window, self.pid, self.instance, self.deadline)
    }

    fn signal(&self, signal: Signal) -> Result<Sent, AdapterError> {
        signal::send(self.pid, self.instance, signal)
    }

    fn timeout_error(&self) -> AdapterError {
        self.deadline.timeout_error()
    }

    fn on_wayland(&self) -> bool {
        matches!(session_server(), DisplayServer::Wayland)
    }

    fn contested_windows(&self) -> usize {
        self.foreign.contested()
    }

    fn has_foreign_route(&self) -> bool {
        self.foreign.in_use()
    }
}
