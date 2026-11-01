use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_port::ToplevelPort;
use super::foreign_proof::{Proof, prove_owned};
use super::foreign_refusal::{control_failure, unproven};
use super::foreign_target::ForeignTarget;

/// Proves the target against the toplevels the port holds now.
fn prove_now(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<u32, AdapterError> {
    let owners = port
        .owners(deadline)
        .map_err(|error| control_failure(error, deadline))?;
    if !target.names.is_current() {
        return Err(AdapterError::stale_ref_because(
            "The target process instance changed or could not be verified",
        )
        .with_disposition(agent_desktop_core::DeliverySemantics::not_delivered()));
    }
    match prove_owned(
        target.title,
        &target.names,
        &port.records(),
        owners.as_ref(),
    ) {
        Proof::One(id) => Ok(id),
        other => Err(unproven(&other)),
    }
}

/// Syncs once, then proves the target.
pub(in crate::system) fn locate(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<u32, AdapterError> {
    port.refresh(deadline)
        .map_err(|error| control_failure(error, deadline))?;
    prove_now(port, target, deadline)
}
