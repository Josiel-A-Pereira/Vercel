use std::cell::Cell;

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode, WindowOp};

use super::*;
use crate::system::window::foreign::foreign_fake::{
    FakePort, cause, deadline, maximized, record, target,
};
use crate::system::window::foreign::foreign_op::apply;
use crate::system::window::foreign::foreign_record::ToplevelRecord;
use crate::system::window::foreign::foreign_request::ToplevelRequest;
use crate::system::x11::control::control_error::ControlError;

/// A port whose syncs work until `good` of them are used, then fail.
struct Dropping {
    inner: FakePort,
    good: Cell<usize>,
}

impl ToplevelPort for Dropping {
    fn refresh(&self, limit: Deadline) -> Result<(), ControlError> {
        if self.good.get() == 0 {
            return Err(ControlError::NoDisplay);
        }
        self.good.set(self.good.get() - 1);
        self.inner.refresh(limit)
    }

    fn records(&self) -> Vec<ToplevelRecord> {
        self.inner.records()
    }

    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        self.inner.send(id, request)
    }
}

#[test]
fn a_failed_sync_is_never_reached_even_when_the_cached_state_holds() {
    let port = FakePort::showing(vec![maximized(record(1, "Home", "thunar"))])
        .failing_refresh(ControlError::TimedOut);
    let outcome = await_flags(&port, 1, |flags| flags.maximized, deadline());
    assert_eq!(outcome, Awaited::Unsynced(ControlError::TimedOut));
}

#[test]
fn a_sync_lost_after_the_send_is_an_unknown_outcome_not_a_success() {
    let port = Dropping {
        inner: FakePort::showing(vec![record(1, "Home", "thunar")]),
        good: Cell::new(1),
    };
    let error: AdapterError = apply(&port, &target("Home"), &WindowOp::Maximize, deadline())
        .expect_err("the readback could not be synced");
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(cause(&error), "no_display");
    assert_eq!(error.disposition, DeliverySemantics::uncertain());
    assert_eq!(
        port.inner.sent(),
        vec![(1, ToplevelRequest::SetMaximized(true))]
    );
}

#[test]
fn a_sync_that_runs_out_with_the_limit_leaves_an_unmet_state_unmet() {
    let spent = Deadline::detached_after(1).expect("a 1 ms deadline");
    std::thread::sleep(std::time::Duration::from_millis(5));
    let port = FakePort::showing(vec![record(1, "Home", "thunar")])
        .failing_refresh(ControlError::TimedOut);
    let outcome = await_flags(&port, 1, |flags| flags.maximized, spent);
    assert_eq!(outcome, Awaited::Unmet(record(1, "Home", "thunar").flags));
    let holding = FakePort::showing(vec![maximized(record(1, "Home", "thunar"))])
        .failing_refresh(ControlError::TimedOut);
    let outcome = await_flags(&holding, 1, |flags| flags.maximized, spent);
    assert_eq!(outcome, Awaited::Unsynced(ControlError::TimedOut));
}
