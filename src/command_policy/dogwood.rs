//! The Dogwood policy gate: builds the facts for each gated command, asks
//! the engine for a verdict before dispatch, and records the outcome after.
//! With no policy configured anywhere this module costs a few stats and
//! does nothing.
//!
//! Load or install failures fail closed for mutating commands and fail open
//! (with a warning) for read-only ones, so an agent can still observe and
//! report a broken policy folder but cannot act under it.

use std::sync::Mutex;

use agent_desktop_core::{
    AdapterError, AppError, DeliverySemantics, ErrorCode, PlatformAdapter, context::CommandContext,
};
use agent_desktop_policy::{CommandFacts, Origin, PolicyError, PolicyGate, PolicySources, Verdict};
use serde_json::json;

use super::dogwood_plan::plan_for;
use super::dogwood_target::Resolver;
use crate::cli::Commands;
use agent_desktop_policy::TargetFacts;

/// One gate per policy layer per process, kept across the entries of a
/// `batch`. A gate hands its store over whenever another process asks.
static GATES: Mutex<Vec<(PolicySources, PolicyGate)>> = Mutex::new(Vec::new());

/// Decide `cmd` against every configured policy layer. `Ok(None)` means
/// the command is exempt or no policies are configured; `Ok(Some(facts))`
/// means every layer allowed every target it lands on and the caller should
/// [`record`] the outcome after dispatch. Targets are resolved only once
/// policies are found, so with none configured the gate reads nothing but
/// the policy folders. A layer that cannot be loaded refuses mutations and
/// is skipped for reads; it never lifts another layer's verdict.
pub(crate) fn enforce(
    cmd: &Commands,
    context: &CommandContext,
    adapter: &dyn PlatformAdapter,
) -> Result<Option<Vec<CommandFacts>>, AppError> {
    let Some(plans) = plan_for(cmd) else {
        return Ok(None);
    };
    let paths = context.policy_paths();
    let found = agent_desktop_policy::discover(&paths.files, &paths.dirs, paths.env_dir.as_deref());
    if !found.is_configured() {
        return Ok(None);
    }
    for error in &found.failures {
        gate_failure(cmd, error)?;
    }
    let mut resolver = Resolver::new(context, adapter);
    let mut facts = Vec::with_capacity(plans.len());
    for plan in &plans {
        let targets = resolver
            .resolve(plan)
            .map_err(|reason| target_unknown(cmd.name(), &reason))?;
        facts.extend(
            targets
                .into_iter()
                .map(|target| facts_from(cmd, context, target)),
        );
    }
    let mut gates = GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut cached = std::mem::take(&mut *gates);
    for layer in found.layers {
        let reused = cached
            .iter()
            .position(|(_, gate)| gate.is_current(&layer))
            .map(|index| cached.swap_remove(index).1);
        match reused.map_or_else(|| PolicyGate::open(&layer), Ok) {
            Ok(gate) => gates.push((layer, gate)),
            Err(error) => gate_failure(cmd, &error)?,
        }
    }
    for (sources, gate) in gates.iter_mut() {
        for target in &facts {
            match gate.check(target) {
                Ok(Verdict::Allowed) => {}
                Ok(Verdict::Denied { policies, errors }) => {
                    return Err(denied(target, sources, policies, errors));
                }
                Err(error) => {
                    gate_failure(cmd, &error)?;
                    break;
                }
            }
        }
    }
    Ok(Some(facts))
}

/// Record how an allowed command ended at each of its targets in every
/// layer; the responses become the history temporal clauses evaluate
/// against. Best effort: the command already ran.
pub(crate) fn record(facts: &[CommandFacts], ok: bool) {
    let mut gates = GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for (_, gate) in gates.iter_mut() {
        for target in facts {
            if let Err(error) = gate.record(target, ok) {
                tracing::warn!(%error, command = target.command, "policy outcome not recorded");
            }
        }
    }
}

/// Close every policy store this process holds, which checkpoints a store
/// whose log has grown. A one-shot CLI process calls it before it exits:
/// the gates live in a static, and Rust never drops statics, so without it
/// no store would ever checkpoint and every open would replay the whole log.
pub(crate) fn release_stores() {
    GATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clear();
}

fn target_unknown(command: &str, reason: &str) -> AppError {
    AdapterError::new(
        ErrorCode::PolicyDenied,
        format!(
            "Desktop policies cannot judge '{command}': the application it would reach is unknown ({reason})"
        ),
    )
    .with_suggestion(
        "Act through an element ref; keyboard input can instead name the application with --app. Coordinates alone cannot be judged here",
    )
    .with_details(json!({ "kind": "dogwood_policy_target_unknown", "retryable": false }))
    .with_disposition(DeliverySemantics::not_delivered())
    .into()
}

fn gate_failure(cmd: &Commands, error: &PolicyError) -> Result<(), AppError> {
    if !cmd.is_mutating() {
        tracing::warn!(%error, command = cmd.name(), "policy gate unavailable; read-only command allowed");
        return Ok(());
    }
    Err(AdapterError::new(
        ErrorCode::PolicyDenied,
        format!("Desktop policies could not be enforced: {error}"),
    )
    .with_suggestion(
        "Fix the policy source (or remove it) and retry; mutating commands fail closed while policies cannot be loaded",
    )
    .with_details(json!({ "kind": "dogwood_policy_unavailable", "retryable": false }))
    .with_disposition(DeliverySemantics::not_delivered())
    .into())
}

fn denied(
    facts: &CommandFacts,
    sources: &PolicySources,
    policies: Vec<String>,
    errors: Vec<String>,
) -> AppError {
    let message = if policies.is_empty() {
        format!(
            "No installed policy permits '{}' on '{}'; a policy may permit it only after an earlier step, such as a snapshot of the app",
            facts.command,
            facts.app_or_system()
        )
    } else {
        format!(
            "'{}' on '{}' is forbidden by policy {}",
            facts.command,
            facts.app_or_system(),
            policies.join(", ")
        )
    };
    AdapterError::new(ErrorCode::PolicyDenied, message)
        .with_suggestion(layer_suggestion(sources.origin))
        .with_details(json!({
            "kind": "dogwood_policy",
            "layer": sources.origin.describe(),
            "policies": policies,
            "evaluation_errors": errors,
            "retryable": false,
        }))
        .with_disposition(DeliverySemantics::not_delivered())
        .into()
}

/// Who decides depends on the layer that refused, and an agent reports
/// the refusal to whoever owns that layer.
fn layer_suggestion(origin: Origin) -> &'static str {
    match origin {
        Origin::Home | Origin::Account => {
            "The machine owner's Dogwood policies (the state root's policies folder) refused this; no flag or variable lifts them. Report it to the user"
        }
        Origin::Env => {
            "The Dogwood policies in AGENT_DESKTOP_POLICY_DIR, set by whoever started this agent, refused this; no other layer lifts them. Report it to the user"
        }
        Origin::Cli => {
            "The Dogwood policies passed with --policy/--policy-dir refused this; no other layer lifts them. Report it to the user"
        }
    }
}

/// The commands the gate never judges, mirrored by the exempt arm of
/// [`plan_for`]. The classification test holds every CLI command to
/// "exempt here xor present in the policy catalog".
#[cfg(test)]
pub(super) const EXEMPT_COMMANDS: &[&str] = &[
    "version",
    "skills",
    "status",
    "permissions",
    "session",
    "trace",
    "cursor-overlay",
    "batch",
];

fn facts_from(cmd: &Commands, context: &CommandContext, target: TargetFacts) -> CommandFacts {
    debug_assert!(
        agent_desktop_policy::group_for(cmd.name()).is_some(),
        "gated command '{}' missing from the policy catalog",
        cmd.name()
    );
    CommandFacts {
        command: cmd.name(),
        agent: context.agent_id().unwrap_or("default").to_string(),
        target,
        headed: context.is_headed(),
        mutating: cmd.is_mutating(),
        session: context.session_id().unwrap_or_default().to_string(),
    }
}

#[cfg(test)]
#[path = "dogwood_tests.rs"]
mod dogwood_tests;
