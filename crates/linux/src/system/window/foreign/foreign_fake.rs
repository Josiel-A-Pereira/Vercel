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
pub(in crate::system) type Reaction = fn(&mut Vec<ToplevelRecord>, u32, ToplevelRequest);

/// A scripted `ToplevelPort`: each refresh shows the next scripted list (the
/// last one stays), every send is recorded and then answered by `reaction`.
pub(in crate::system) struct FakePort {
    shown: RefCell<Vec<ToplevelRecord>>,
    later: RefCell<VecDeque<Vec<ToplevelRecord>>>,
    sent: RefCell<Vec<(u32, ToplevelRequest)>>,
    refreshes: Cell<usize>,
    reaction: Cell<Reaction>,
    errors: Cell<(Option<ControlError>, Option<ControlError>)>,
    mapped: bool,
}

pub(in crate::system) fn record(id: u32, title: &str, app_id: &str) -> ToplevelRecord {
    ToplevelRecord {
        id,
        title: title.to_owned(),
        app_id: app_id.to_owned(),
        flags: ToplevelFlags::default(),
    }
}

pub(in crate::system) fn activated(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.activated = true;
    record
}

pub(in crate::system) fn minimized(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.minimized = true;
    record
}

pub(in crate::system) fn maximized(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.maximized = true;
    record
}

pub(in crate::system) fn fullscreen(mut record: ToplevelRecord) -> ToplevelRecord {
    record.flags.fullscreen = true;
    record
}

pub(in crate::system) fn thunar() -> ProcessNames {
    ProcessNames::from_names(vec!["thunar".to_owned()])
}

pub(in crate::system) fn target(title: &str) -> ForeignTarget<'_> {
    ForeignTarget {
        title,
        names: thunar(),
    }
}

pub(in crate::system) fn deadline() -> Deadline {
    Deadline::detached_after(300).expect("a 300 ms deadline is representable")
}

pub(in crate::system) fn cause(error: &AdapterError) -> String {
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
pub(in crate::system) fn obeys(
    records: &mut Vec<ToplevelRecord>,
    id: u32,
    request: ToplevelRequest,
) {
    if request == ToplevelRequest::Close {
        records.retain(|each| each.id != id);
    }
    apply(records, id, request, true);
}

/// Hyprland: activate and maximize work, minimize changes nothing.
pub(in crate::system) fn hyprland(
    records: &mut Vec<ToplevelRecord>,
    id: u32,
    request: ToplevelRequest,
) {
    if request == ToplevelRequest::Close {
        records.retain(|each| each.id != id);
    }
    apply(records, id, request, false);
}

/// A compositor that takes every request and changes nothing.
pub(in crate::system) fn ignores(_: &mut Vec<ToplevelRecord>, _: u32, _: ToplevelRequest) {}

/// A window that closes the moment it is asked anything.
pub(in crate::system) fn vanishes(records: &mut Vec<ToplevelRecord>, id: u32, _: ToplevelRequest) {
    records.retain(|each| each.id != id);
}

impl FakePort {
    pub(in crate::system) fn showing(records: Vec<ToplevelRecord>) -> Self {
        Self {
            shown: RefCell::new(records),
            later: RefCell::new(VecDeque::new()),
            sent: RefCell::new(Vec::new()),
            refreshes: Cell::new(0),
            reaction: Cell::new(obeys),
            errors: Cell::new((None, None)),
            mapped: false,
        }
    }

    pub(in crate::system) fn mapped(mut self) -> Self {
        self.mapped = true;
        self
    }

    /// The list the next refresh shows after the ones already scripted.
    pub(in crate::system) fn then_showing(self, records: Vec<ToplevelRecord>) -> Self {
        self.later.borrow_mut().push_back(records);
        self
    }

    pub(in crate::system) fn reacting(self, reaction: Reaction) -> Self {
        self.reaction.set(reaction);
        self
    }

    pub(in crate::system) fn failing_refresh(self, error: ControlError) -> Self {
        self.errors.set((Some(error), self.errors.get().1));
        self
    }

    pub(in crate::system) fn failing_send(self, error: ControlError) -> Self {
        self.errors.set((self.errors.get().0, Some(error)));
        self
    }

    pub(in crate::system) fn sent(&self) -> Vec<(u32, ToplevelRequest)> {
        self.sent.borrow().clone()
    }

    pub(in crate::system) fn refreshes(&self) -> usize {
        self.refreshes.get()
    }
}

impl ToplevelPort for FakePort {
    fn refresh(&self, _: Deadline) -> Result<(), ControlError> {
        self.refreshes.set(self.refreshes.get() + 1);
        if let Some(error) = self.errors.get().0 {
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

    fn owners(
        &self,
        _: Deadline,
    ) -> Result<Option<std::collections::HashMap<u32, u32>>, ControlError> {
        Ok(self
            .mapped
            .then(|| self.records().iter().map(|record| (record.id, 7)).collect()))
    }

    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        if let Some(error) = self.errors.get().1 {
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
