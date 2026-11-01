use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_flags::ToplevelFlags;
use super::foreign_names::ProcessNames;
use super::foreign_port::ToplevelPort;
use super::foreign_record::ToplevelRecord;
use super::foreign_request::ToplevelRequest;
use super::foreign_target::ForeignTarget;
use crate::system::x11::control::control_error::ControlError;

/// What a compositor does to the toplevels it holds when a request arrives.
pub(super) type Reaction = fn(&mut Vec<ToplevelRecord>, u32, ToplevelRequest);

/// A scripted `ToplevelPort`: each refresh shows the next scripted list (the
/// last one stays), every send is recorded and then answered by `reaction`.
pub(super) struct FakePort {
    shown: RefCell<Vec<ToplevelRecord>>,
    later: RefCell<VecDeque<Vec<ToplevelRecord>>>,
    sent: RefCell<Vec<(u32, ToplevelRequest)>>,
    refreshes: Cell<usize>,
    reaction: Cell<Reaction>,
    refresh_error: Cell<Option<ControlError>>,
    send_error: Cell<Option<ControlError>>,
}

pub(super) fn record(id: u32, title: &str, app_id: &str) -> ToplevelRecord {
    ToplevelRecord {
        id,
        title: title.to_owned(),
        app_id: app_id.to_owned(),
        flags: ToplevelFlags::default(),
    }
}

pub(super) fn activated(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.activated = true;
    record
}

pub(super) fn minimized(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.minimized = true;
    record
}

pub(super) fn maximized(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.maximized = true;
    record
}

pub(super) fn fullscreen(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.fullscreen = true;
    record
}

pub(super) fn thunar() -> ProcessNames {
    ProcessNames::from_names(vec!["thunar".to_owned()])
}

pub(super) fn target(title: &str) -> ForeignTarget<'_> {
    ForeignTarget {
        title,
        names: thunar(),
    }
}

pub(super) fn deadline() -> Deadline {
    Deadline::detached_after(300).expect("a 300 ms deadline is representable")
}

pub(super) fn cause(error: &AdapterError) -> String {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("cause"))
        .and_then(|cause| cause.as_str())
        .unwrap_or_default()
        .to_owned()
}

fn apply(records: &mut [ToplevelRecord], id: u32, request: ToplevelRequest, minimize: bool) {
    if let ToplevelRequest::Activate = request {
        for each in records.iter_mut() {
            each.flags.activated = each.id == id;
        }
        return;
    }
    let Some(window) = records.iter_mut().find(|each| each.id == id) else {
        return;
    };
    match request {
        ToplevelRequest::SetMaximized(on) => window.flags.maximized = on,
        ToplevelRequest::SetMinimized(on) if minimize => window.flags.minimized = on,
        _ => {}
    }
}

/// A compositor that does what it is asked, and removes a closed window.
pub(super) fn obeys(records: &mut Vec<ToplevelRecord>, id: u32, request: ToplevelRequest) {
    if request == ToplevelRequest::Close {
        records.retain(|each| each.id != id);
    }
    apply(records, id, request, true);
}

/// Hyprland: activate and maximize work, minimize changes nothing.
pub(super) fn hyprland(records: &mut Vec<ToplevelRecord>, id: u32, request: ToplevelRequest) {
    if request == ToplevelRequest::Close {
        records.retain(|each| each.id != id);
    }
    apply(records, id, request, false);
}

/// A compositor that takes every request and changes nothing.
pub(super) fn ignores(_: &mut Vec<ToplevelRecord>, _: u32, _: ToplevelRequest) {}

/// A window that closes the moment it is asked anything.
pub(super) fn vanishes(records: &mut Vec<ToplevelRecord>, id: u32, _: ToplevelRequest) {
    records.retain(|each| each.id != id);
}

impl FakePort {
    pub(super) fn showing(records: Vec<ToplevelRecord>) -> Self {
        Self {
            shown: RefCell::new(records),
            later: RefCell::new(VecDeque::new()),
            sent: RefCell::new(Vec::new()),
            refreshes: Cell::new(0),
            reaction: Cell::new(obeys),
            refresh_error: Cell::new(None),
            send_error: Cell::new(None),
        }
    }

    /// The list the next refresh shows after the ones already scripted.
    pub(super) fn then_showing(self, records: Vec<ToplevelRecord>) -> Self {
        self.later.borrow_mut().push_back(records);
        self
    }

    pub(super) fn reacting(self, reaction: Reaction) -> Self {
        self.reaction.set(reaction);
        self
    }

    pub(super) fn failing_refresh(self, error: ControlError) -> Self {
        self.refresh_error.set(Some(error));
        self
    }

    pub(super) fn failing_send(self, error: ControlError) -> Self {
        self.send_error.set(Some(error));
        self
    }

    pub(super) fn sent(&self) -> Vec<(u32, ToplevelRequest)> {
        self.sent.borrow().clone()
    }

    pub(super) fn refreshes(&self) -> usize {
        self.refreshes.get()
    }
}

impl ToplevelPort for FakePort {
    fn refresh(&self, _: Deadline) -> Result<(), ControlError> {
        self.refreshes.set(self.refreshes.get() + 1);
        if let Some(error) = self.refresh_error.get() {
            return Err(error);
        }
        if let Some(next) = self.later.borrow_mut().pop_front() {
            *self.shown.borrow_mut() = next;
        }
        Ok(())
    }

    fn records(&self) -> Vec<ToplevelRecord> {
        self.shown.borrow().clone()
    }

    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        if let Some(error) = self.send_error.get() {
            return Err(error);
        }
        if !self.shown.borrow().iter().any(|each| each.id == id) {
            return Err(ControlError::BadWindow);
        }
        self.sent.borrow_mut().push((id, request));
        (self.reaction.get())(&mut self.shown.borrow_mut(), id, request);
        Ok(())
    }
}
