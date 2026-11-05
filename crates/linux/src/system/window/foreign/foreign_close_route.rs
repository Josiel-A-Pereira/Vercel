use std::cell::{Cell, OnceCell, RefCell};

use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_close::read_standing;
use super::foreign_close_evidence::CloseEvidence;
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
/// Frames and toplevels are refreshed on each read. The first close retains
/// the ids already present, and a later target toplevel is treated as a
/// prompt. A selected window is checked again immediately before each send.
pub(in crate::system) struct ForeignCloseRoute<P: ToplevelPort> {
    link: OnceCell<Option<P>>,
    in_use: Cell<bool>,
    contested: Cell<usize>,
    sent: RefCell<Vec<u32>>,
    evidence: RefCell<CloseEvidence>,
}

impl<P: ToplevelPort> Default for ForeignCloseRoute<P> {
    fn default() -> Self {
        Self {
            link: OnceCell::new(),
            in_use: Cell::new(false),
            contested: Cell::new(0),
            sent: RefCell::new(Vec::new()),
            evidence: RefCell::new(CloseEvidence::default()),
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
        self.sent.borrow_mut().clear();
        let mut evidence = self.evidence.borrow_mut();
        evidence.unproven.clear();
        evidence.dialogs.clear();
        evidence.selected.clear();
        evidence.frames.clear();
    }

    /// Whether the last read answered through this route.
    pub(in crate::system) fn in_use(&self) -> bool {
        self.in_use.get()
    }

    /// The windows the last read could not prove.
    pub(in crate::system) fn contested(&self) -> usize {
        self.contested.get()
    }

    pub(in crate::system) fn unproven(&self) -> Vec<String> {
        self.evidence.borrow().unproven.clone()
    }

    pub(in crate::system) fn dialogs(&self) -> Vec<String> {
        self.evidence.borrow().dialogs.clone()
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
        let frames = read_frames()?;
        port.refresh(deadline)?;
        let owners = port.owners(deadline)?;
        let mut evidence = self.evidence.borrow_mut();
        let records = port.records();
        let standing = read_standing(&records, &frames, names, &evidence.known, owners.as_ref());
        evidence.selected = records
            .into_iter()
            .filter(|record| standing.windows.iter().any(|window| window.id == record.id))
            .map(|record| {
                (
                    record.id,
                    (
                        record.clone(),
                        owners.as_ref().and_then(|_| port.address(record.id)),
                    ),
                )
            })
            .collect();
        evidence.names = Some(names.clone());
        evidence.frames = frames;
        evidence.unproven = standing.unproven.clone();
        evidence.dialogs = standing.dialogs.clone();
        self.in_use.set(true);
        self.contested.set(standing.contested);
        for window in &standing.windows {
            if window.kind == WindowKind::Dialog && !evidence.prompts.contains(&window.id) {
                evidence.prompts.push(window.id);
            }
        }
        Ok(standing.windows)
    }

    /// Asks one toplevel to close. The first request first records every id on
    /// screen. A toplevel read as a prompt is refused as an unknown window.
    pub(in crate::system) fn close(&self, id: u32, deadline: Deadline) -> Result<(), ControlError> {
        let port = self.port()?;
        let mut evidence = self.evidence.borrow_mut();
        if evidence.prompts.contains(&id) {
            return Err(ControlError::BadWindow);
        }
        port.refresh(deadline)?;
        let owners = port.owners(deadline)?;
        let records = port.records();
        let names = evidence.names.as_ref().ok_or(ControlError::StaleWindow)?;
        let (before, address) = evidence.selected.get(&id).ok_or(ControlError::BadWindow)?;
        let current = records
            .iter()
            .find(|record| record.id == id)
            .ok_or(ControlError::BadWindow)?;
        if before.title != current.title
            || before.app_id != current.app_id
            || *address != owners.as_ref().and_then(|_| port.address(id))
        {
            return Err(ControlError::StaleWindow);
        }
        if !names.is_current() {
            return Err(ControlError::StaleWindow);
        }
        let sent = self.sent.borrow();
        let remaining: Vec<_> = match owners.as_ref() {
            Some(owners) => records
                .iter()
                .filter(|record| !sent.contains(&record.id) || owners.contains_key(&record.id))
                .cloned()
                .collect(),
            None => records.clone(),
        };
        drop(sent);
        let standing = read_standing(
            &remaining,
            &evidence.frames,
            names,
            &evidence.known,
            owners.as_ref(),
        );
        if standing.contested > 0
            || !standing.unproven.is_empty()
            || !standing.dialogs.is_empty()
            || standing
                .windows
                .iter()
                .any(|window| window.kind == WindowKind::Dialog)
            || !standing.windows.iter().any(|window| window.id == id)
        {
            return Err(ControlError::StaleWindow);
        }
        if evidence.known.is_empty() {
            evidence.known = records.iter().map(|record| record.id).collect();
        }
        let result = port.send(id, ToplevelRequest::Close);
        self.sent.borrow_mut().push(id);
        result
    }

    pub(in crate::system) fn port(&self) -> Result<&P, ControlError> {
        self.link
            .get()
            .and_then(Option::as_ref)
            .ok_or(ControlError::NoDisplay)
    }
}

#[cfg(test)]
#[path = "foreign_close_route_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "foreign_close_sent_tests.rs"]
mod sent_tests;
