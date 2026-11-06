use agent_desktop_core::{AdapterError, AppInfo, Deadline, ErrorCode, ProcessIdentity};

pub(crate) use crate::system::app::close_outcome::CloseOutcome;

#[cfg(target_os = "macos")]
pub(crate) fn pid_from_element(
    element: &crate::tree::AXElement,
    deadline: Deadline,
) -> Option<i32> {
    crate::tree::properties::ax_ipc::pid(element, deadline).ok()
}

const PROTECTED_PROCESSES: &[&str] = &["loginwindow", "windowserver", "dock", "launchd", "finder"];

pub(crate) fn is_protected_process(identifier: &str) -> bool {
    let lower = identifier.to_lowercase();
    PROTECTED_PROCESSES
        .iter()
        .any(|protected| lower == *protected || lower.split('.').any(|part| part == *protected))
}

fn ensure_not_protected(id: &str) -> Result<(), AdapterError> {
    if is_protected_process(id) {
        return Err(AdapterError::new(
            ErrorCode::InvalidArgs,
            format!("'{id}' is a protected system process and cannot be closed"),
        )
        .with_suggestion(
            "Target a regular application; session-critical processes are never closed.",
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(crate) fn close_app_impl(
    app: &AppInfo,
    force: bool,
    deadline: Deadline,
) -> Result<CloseOutcome, AdapterError> {
    ensure_not_protected(&app.name)?;
    if let Some(bundle_id) = &app.bundle_id {
        ensure_not_protected(bundle_id)?;
    }
    let instance = app.process_instance.as_deref().ok_or_else(|| {
        AdapterError::new(
            ErrorCode::InvalidArgs,
            "Exact close requires a process instance token",
        )
    })?;
    let identity = crate::system::process::process_identity::require_core(&ProcessIdentity {
        pid: app.pid,
        instance: instance.to_owned(),
    })?;
    terminate_running_application(&app.name, identity, force, deadline)
}

#[cfg(target_os = "macos")]
fn terminate_running_application(
    id: &str,
    identity: crate::system::process::process_identity::ProcessIdentity,
    force: bool,
    deadline: Deadline,
) -> Result<CloseOutcome, AdapterError> {
    if deadline.is_expired() {
        return Err(before_termination_request(deadline.timeout_error()));
    }
    crate::system::cocoa_runtime::ensure_cocoa_multithreaded()
        .map_err(before_termination_request)?;
    if !identity
        .still_matches()
        .map_err(before_termination_request)?
    {
        return Ok(CloseOutcome::AlreadyGone);
    }
    let outcome = crate::system::appkit_bridge::terminate(
        identity.pid(),
        identity.launch_time_seconds(),
        force,
    )?;
    match outcome {
        crate::system::appkit_bridge::TerminationOutcome::Missing => {
            return if identity
                .still_matches()
                .map_err(before_termination_request)?
            {
                Err(AdapterError::new(
                    ErrorCode::AppUnresponsive,
                    "NSRunningApplication could not resolve the verified process instance",
                )
                .with_disposition(agent_desktop_core::DeliverySemantics::not_delivered()))
            } else {
                Ok(CloseOutcome::AlreadyGone)
            };
        }
        crate::system::appkit_bridge::TerminationOutcome::Rejected => {
            if identity
                .still_matches()
                .map_err(before_termination_request)?
            {
                return Err(termination_request_not_accepted(id, identity.pid(), force));
            }
            return Ok(CloseOutcome::AlreadyGone);
        }
        crate::system::appkit_bridge::TerminationOutcome::IdentityMismatch => {
            return Err(AdapterError::new(
                ErrorCode::StaleRef,
                "NSRunningApplication changed identity before termination delivery",
            )
            .with_disposition(agent_desktop_core::DeliverySemantics::not_delivered()));
        }
        crate::system::appkit_bridge::TerminationOutcome::Accepted => {}
    }
    wait_for_exit(id, identity, force, deadline)
}

#[cfg(target_os = "macos")]
fn termination_request_not_accepted(id: &str, pid: i32, force: bool) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        format!("The native termination API did not accept the request for '{id}'"),
    )
    .with_details(serde_json::json!({
        "pid": pid,
        "force": force,
    }))
    .with_disposition(agent_desktop_core::DeliverySemantics::not_delivered())
}

#[cfg(target_os = "macos")]
fn wait_for_exit(
    id: &str,
    identity: crate::system::process::process_identity::ProcessIdentity,
    force: bool,
    deadline: Deadline,
) -> Result<CloseOutcome, AdapterError> {
    loop {
        if !identity
            .still_matches()
            .map_err(after_termination_request)?
        {
            return Ok(CloseOutcome::Exited);
        }
        if deadline.is_expired() {
            return timeout_after_termination(id, identity, force, deadline);
        }
        let pause = deadline
            .remaining_slice(std::time::Duration::from_millis(25))
            .map_err(after_termination_request)?;
        std::thread::sleep(pause.min(std::time::Duration::from_millis(25)));
    }
}

#[cfg(target_os = "macos")]
fn timeout_after_termination(
    id: &str,
    identity: crate::system::process::process_identity::ProcessIdentity,
    force: bool,
    deadline: Deadline,
) -> Result<CloseOutcome, AdapterError> {
    if let Some(titles) = pending_dialog_titles(&identity) {
        return Err(AdapterError::dialog_pending(
            id,
            identity.pid(),
            force,
            &titles,
        ));
    }
    if !identity
        .still_matches()
        .map_err(after_termination_request)?
    {
        return Ok(CloseOutcome::Exited);
    }
    Err(deadline
        .timeout_error()
        .with_details(serde_json::json!({
            "app": id,
            "pid": identity.pid(),
            "force": force,
        }))
        .with_disposition(agent_desktop_core::DeliverySemantics::delivered_unverified()))
}

/// Titles of the app's modal prompts, if any are standing when the exit wait
/// times out. A short bounded probe: a prompt the quit request opened is
/// already listed by `list-surfaces`, and a probe failure says nothing about
/// the app, so it falls back to the plain timeout below.
#[cfg(target_os = "macos")]
fn pending_dialog_titles(
    identity: &crate::system::process::process_identity::ProcessIdentity,
) -> Option<Vec<String>> {
    let probe_deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
    let surfaces =
        crate::tree::surface_inventory::list_surfaces_for_process(*identity, probe_deadline)
            .ok()?;
    let titles = dialog_titles(&surfaces);
    (!titles.is_empty()).then_some(titles)
}

/// Titles of the modal prompts (alerts and sheets) among the app's surfaces,
/// in surface order; an untitled prompt contributes an empty string so the
/// count still reports it. Popovers and menus are not prompts and never block
/// a quit.
fn dialog_titles(surfaces: &[agent_desktop_core::SurfaceInfo]) -> Vec<String> {
    surfaces
        .iter()
        .filter(|surface| matches!(surface.kind.as_str(), "alert" | "sheet"))
        .map(|surface| surface.title.clone().unwrap_or_default())
        .collect()
}

#[cfg(target_os = "macos")]
fn after_termination_request(error: AdapterError) -> AdapterError {
    error.with_disposition(agent_desktop_core::DeliverySemantics::delivered_unverified())
}

#[cfg(target_os = "macos")]
fn before_termination_request(error: AdapterError) -> AdapterError {
    error.with_disposition(agent_desktop_core::DeliverySemantics::not_delivered())
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn close_app_impl(
    _app: &AppInfo,
    _force: bool,
    _deadline: Deadline,
) -> Result<CloseOutcome, AdapterError> {
    Err(AdapterError::not_supported("close_app"))
}

#[cfg(test)]
#[path = "app_ops_tests.rs"]
mod tests;
