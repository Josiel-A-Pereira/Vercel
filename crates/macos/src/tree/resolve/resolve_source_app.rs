use agent_desktop_core::{AdapterError, RefEntry};

use crate::tree::AXElement;
use crate::tree::resolve::resolve_read_context::ResolveReadContext;

#[cfg(target_os = "macos")]
pub(in crate::tree) fn verify_source_application(
    application: &AXElement,
    entry: &RefEntry,
    context: &mut ResolveReadContext,
) -> Result<(), AdapterError> {
    let Some(expected) = entry
        .source
        .source_app
        .as_deref()
        .filter(|name| !name.is_empty())
    else {
        return Ok(());
    };
    let actual = super::resolve_ax_read::read_string_with_usage(
        application,
        "AXTitle",
        context.deadline,
        &mut context.usage,
    )?;
    if source_app_matches(
        expected,
        actual.as_deref(),
        || {
            let pid = crate::system::process::process_identity::to_pid_t(entry.process.pid)?;
            let records = crate::system::window::cg_window::window_records_until(
                context.deadline,
                crate::system::window::cg_window::WindowRecordScope::Pid(pid),
            )?;
            Ok(source_names_for_process(
                pid,
                entry.process.process_instance.as_deref(),
                records.iter().map(|record| {
                    (
                        record.pid,
                        record.process_instance.as_deref(),
                        record.app_name.as_str(),
                    )
                }),
            ))
        },
        || {
            crate::tree::locator_deadline::remaining(context.deadline)?;
            let pid = crate::system::process::process_identity::to_pid_t(entry.process.pid)?;
            let names = crate::system::appkit_bridge::application_names(pid)?;
            crate::tree::locator_deadline::remaining(context.deadline)?;
            Ok(names)
        },
    )? {
        return Ok(());
    }
    Err(
        AdapterError::element_not_found("source application").with_details(serde_json::json!({
            "kind": "source_process_identity",
            "pid": entry.process.pid,
            "expected_app": expected,
            "actual_app": actual,
            "complete": true,
            "retryable": false,
        })),
    )
}

fn source_app_matches(
    expected: &str,
    title: Option<&str>,
    inventory_names: impl FnOnce() -> Result<Vec<String>, AdapterError>,
    native_names: impl FnOnce() -> Result<Vec<String>, AdapterError>,
) -> Result<bool, AdapterError> {
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        if agent_desktop_core::app_name_matches(title, expected) {
            return Ok(true);
        }
        if !native_names()?
            .iter()
            .any(|name| agent_desktop_core::app_name_matches(name, title))
        {
            return Ok(false);
        }
    }
    Ok(inventory_names()?
        .iter()
        .any(|name| agent_desktop_core::app_name_matches(name, expected)))
}

fn source_names_for_process<'a>(
    pid: i32,
    instance: Option<&str>,
    records: impl IntoIterator<Item = (i32, Option<&'a str>, &'a str)>,
) -> Vec<String> {
    records
        .into_iter()
        .filter(|(owner, current, _)| *owner == pid && instance.is_some() && *current == instance)
        .map(|(_, _, name)| name.to_owned())
        .collect()
}

#[cfg(test)]
#[path = "resolve_source_app_tests.rs"]
mod tests;
