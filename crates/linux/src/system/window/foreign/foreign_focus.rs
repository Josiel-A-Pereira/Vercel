use std::time::Duration;

use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics};

use super::foreign_await::{await_flags, flags_of};
use super::foreign_awaited::Awaited;
use super::foreign_locate::locate;
use super::foreign_port::ToplevelPort;
use super::foreign_refusal::{control_failure, window_destroyed};
use super::foreign_request::ToplevelRequest;
use super::foreign_target::ForeignTarget;
use crate::system::focus::focus_cause::{Cause, refusal};

const ACTIVATION_BUDGET: Duration = Duration::from_secs(2);

fn not_activated() -> AdapterError {
    refusal(Cause::ActivationRefused).with_disposition(DeliverySemantics::delivered_unverified())
}

/// Activates the proven toplevel of the target and verifies it from the
/// compositor's own `activated` state. A window that is already active is
/// left alone and nothing is sent.
pub(in crate::system) fn activate(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<(), AdapterError> {
    let id = locate(port, target, deadline)?;
    if flags_of(port, id).is_some_and(|flags| flags.activated) {
        return Ok(());
    }
    port.send(id, ToplevelRequest::Activate)
        .map_err(|error| control_failure(error, deadline))?;
    let limit = deadline.capped(ACTIVATION_BUDGET);
    match await_flags(port, id, |flags| flags.activated, limit) {
        Awaited::Reached => Ok(()),
        Awaited::Gone => Err(window_destroyed()),
        Awaited::Unmet(_) => Err(not_activated()),
    }
}

#[cfg(test)]
#[path = "foreign_focus_tests.rs"]
mod tests;
