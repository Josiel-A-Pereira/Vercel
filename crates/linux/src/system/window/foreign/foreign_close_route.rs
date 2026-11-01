use std::cell::{Cell, OnceCell, RefCell};

use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_close::read_standing;
use super::foreign_frame::Frame;
use super::foreign_names::ProcessNames;
use super::foreign_port::ToplevelPort;
use super::foreign_request::ToplevelRequest;
use crate::system::x11::control::control_error::ControlError;
use crate::system::x11::window::toplevel::Toplevel;
use crate::system::x11::window::window_kind::WindowKind;

/// The compositor's toplevel list as one close reads it: opened on first use,
/// remembering what a close needs between reads.
///
/// The frames are read once, before any close request, because a prompt
/// raised by the request may itself show up as a frame and would then pass for
/// a proven window. The ids on screen at the first close request are kept so a
/// toplevel that appears later is told apart from one that was always there,
/// and an id once listed as a prompt is never sent a close request.
pub(in crate::system) struct ForeignCloseRoute<P: ToplevelPort> {
    link: OnceCell<Option<P>>,
    frames: OnceCell<Vec<Frame>>,
    in_use: Cell<bool>,
    contested: Cell<usize>,
    known: RefCell<Vec<u32>>,
    prompts: RefCell<Vec<u32>>,
}

impl<P: ToplevelPort> Default for ForeignCloseRoute<P> {
    fn default() -> Self {
        Self {
            link: OnceCell::new(),
            frames: OnceCell::new(),
            in_use: Cell::new(false),
            contested: Cell::new(0),
            known: RefCell::new(Vec::new()),
            prompts: RefCell::new(Vec::new()),
        }
    }
}

impl<P: ToplevelPort> ForeignCloseRoute<P> {
    #[cfg(test)]
    pub(in crate::system) fn over(port: P) -> Self {
        let route = Self::default();
        let _ = route.link.set(Some(port));
        route
    }

    /// Opens the link on the first call. `Ok(false)` is a session without a
    /// foreign toplevel manager; an `Err` is an open that missed its deadline
    /// and is tried again on the next call.
    pub(in crate::system) fn reach(
        &self,
        open: impl FnOnce() -> Result<Option<P>, AdapterError>,
    ) -> Result<bool, AdapterError> {
        if self.link.get().is_none() {
            let _ = self.link.set(open()?);
        }
        Ok(self.link.get().is_some_and(Option::is_some))
    }

    /// A read answered from another source: the route no longer carries
    /// close requests until a read answers through it again.
    pub(in crate::system) fn release(&self) {
        self.in_use.set(false);
        self.contested.set(0);
    }

    /// Whether the last read answered through this route.
    pub(in crate::system) fn in_use(&self) -> bool {
        self.in_use.get()
    }

    /// The windows the last read could not prove.
    pub(in crate::system) fn contested(&self) -> usize {
        self.contested.get()
    }

    /// One bounded refresh, then the windows of the process the compositor
    /// lists. Nothing is sent.
    pub(in crate::system) fn standing(
        &self,
        names: &ProcessNames,
        read_frames: impl FnOnce() -> Result<Vec<Frame>, ControlError>,
        deadline: Deadline,
    ) -> Result<Vec<Toplevel>, ControlError> {
        let port = self.port()?;
        let fresh;
        let frames: &[Frame] = match self.frames.get() {
            Some(frames) => frames,
            None => {
                fresh = read_frames()?;
                if !fresh.is_empty() {
                    let _ = self.frames.set(fresh.clone());
                }
                &fresh
            }
        };
        port.refresh(deadline)?;
        let standing = read_standing(&port.records(), frames, names, &self.known.borrow());
        self.in_use.set(true);
        self.contested.set(standing.contested);
        let mut prompts = self.prompts.borrow_mut();
        for window in &standing.windows {
            if window.kind == WindowKind::Dialog && !prompts.contains(&window.id) {
                prompts.push(window.id);
            }
        }
        Ok(standing.windows)
    }

    /// Asks one toplevel to close. The first request first records every id on
    /// screen. A toplevel read as a prompt is refused as an unknown window.
    pub(in crate::system) fn close(&self, id: u32, deadline: Deadline) -> Result<(), ControlError> {
        let port = self.port()?;
        if self.prompts.borrow().contains(&id) {
            return Err(ControlError::BadWindow);
        }
        if self.known.borrow().is_empty() {
            port.refresh(deadline)?;
            *self.known.borrow_mut() = port.records().iter().map(|record| record.id).collect();
        }
        port.send(id, ToplevelRequest::Close)
    }

    fn port(&self) -> Result<&P, ControlError> {
        self.link
            .get()
            .and_then(Option::as_ref)
            .ok_or(ControlError::NoDisplay)
    }
}

#[cfg(test)]
#[path = "foreign_close_route_tests.rs"]
mod tests;
