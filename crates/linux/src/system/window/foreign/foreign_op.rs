use std::time::Duration;

use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, WindowOp};

use super::foreign_await::{await_flags, flags_of};
use super::foreign_awaited::Awaited;
use super::foreign_flags::ToplevelFlags;
use super::foreign_locate::locate;
use super::foreign_port::ToplevelPort;
use super::foreign_refusal::{
    control_failure, minimize_not_applied, move_or_resize, no_window, not_applied, sync_failure,
    window_destroyed,
};
use super::foreign_request::ToplevelRequest;
use super::foreign_target::ForeignTarget;

const SETTLE_BUDGET: Duration = Duration::from_millis(500);

fn failed_readback(request: ToplevelRequest, observed: &ToplevelFlags) -> AdapterError {
    if request == ToplevelRequest::SetMinimized(true) {
        minimize_not_applied(observed)
    } else {
        not_applied(request, observed)
    }
}

fn send_and_verify(
    port: &impl ToplevelPort,
    id: u32,
    request: ToplevelRequest,
    deadline: Deadline,
    target: &ForeignTarget<'_>,
) -> Result<(), AdapterError> {
    if !target.names.is_current() {
        return Err(AdapterError::stale_ref_because(
            "The target process instance changed or could not be verified",
        )
        .with_disposition(DeliverySemantics::not_delivered()));
    }
    port.send(id, request)
        .map_err(|error| control_failure(error, deadline))?;
    let limit = deadline.capped(SETTLE_BUDGET);
    match await_flags(port, id, |flags| request.holds(flags), limit) {
        Awaited::Reached => Ok(()),
        Awaited::Gone => Err(window_destroyed()),
        Awaited::Unmet(observed) => Err(failed_readback(request, &observed)),
        Awaited::Unsynced(error) => Err(sync_failure(error, deadline)),
    }
}

fn set(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    request: ToplevelRequest,
    deadline: Deadline,
) -> Result<bool, AdapterError> {
    let id = locate(port, target, deadline)?;
    if flags_of(port, id).is_some_and(|flags| request.holds(&flags)) {
        return Ok(false);
    }
    send_and_verify(port, id, request, deadline, target).map(|()| true)
}

/// The window is proven once; every step then addresses that same toplevel,
/// so a second step never answers for a window the first step did not touch.
fn restore(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<bool, AdapterError> {
    let id = locate(port, target, deadline)?;
    let mut flags = flags_of(port, id).ok_or_else(no_window)?;
    let mut sent = false;
    if flags.minimized {
        send_and_verify(
            port,
            id,
            ToplevelRequest::SetMinimized(false),
            deadline,
            target,
        )?;
        sent = true;
        flags = flags_of(port, id).ok_or_else(window_destroyed)?;
    }
    if flags.maximized {
        send_and_verify(
            port,
            id,
            ToplevelRequest::SetMaximized(false),
            deadline,
            target,
        )
        .map_err(|error| {
            if sent && error.disposition == DeliverySemantics::not_delivered() {
                error.with_disposition(DeliverySemantics::delivered_unverified())
            } else {
                error
            }
        })?;
        sent = true;
    }
    Ok(sent)
}

/// Applies a window operation to the proven toplevel of the target. Move and
/// resize are refused before the compositor is asked anything.
pub(in crate::system) fn apply(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    op: &WindowOp,
    deadline: Deadline,
) -> Result<bool, AdapterError> {
    match op {
        WindowOp::Move { .. } | WindowOp::Resize { .. } => Err(move_or_resize()),
        WindowOp::Minimize => set(port, target, ToplevelRequest::SetMinimized(true), deadline),
        WindowOp::Maximize => set(port, target, ToplevelRequest::SetMaximized(true), deadline),
        WindowOp::Restore => restore(port, target, deadline),
    }
}

#[cfg(test)]
#[path = "foreign_op_tests.rs"]
mod tests;
