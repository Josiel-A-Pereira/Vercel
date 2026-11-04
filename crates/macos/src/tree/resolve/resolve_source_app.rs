use agent_desktop_core::{AdapterError, RefEntry};

use crate::tree::AXElement;
use crate::tree::resolve::resolve_read_context::ResolveReadContext;

/// Window-owned sources are checked by application name. A process-owned source is already
/// identified by its saved pid and process instance, which resolution verifies first.
#[cfg(target_os = "macos")]
pub(in crate::tree) fn verify_source_application(
    application: &AXElement,
    entry: &RefEntry,
    context: &mut ResolveReadContext,
) -> Result<(), AdapterError> {
    if entry.source.is_process_owned() {
        return Ok(());
    }
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
    if source_app_matches(expected, actual.as_deref(), || {
        let pid = crate::system::process::process_identity::to_pid_t(entry.process.pid)?;
        let records = crate::system::window::cg_window::window_records_until(
            context.deadline,
            crate::system::window::cg_window::WindowRecordScope::Pid(pid),
        )?;
        Ok(source_names_for_pid(
            pid,
            records
                .iter()
                .map(|record| (record.pid, record.app_name.as_str())),
        ))
    })? {
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
) -> Result<bool, AdapterError> {
    if title.is_some_and(|title| agent_desktop_core::app_name_matches(title, expected)) {
        return Ok(true);
    }
    Ok(inventory_names()?
        .iter()
        .any(|name| agent_desktop_core::app_name_matches(name, expected)))
}

fn source_names_for_pid<'a>(
    pid: i32,
    records: impl IntoIterator<Item = (i32, &'a str)>,
) -> Vec<String> {
    records
        .into_iter()
        .filter(|(owner, _)| *owner == pid)
        .map(|(_, name)| name.to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_title_uses_inventory_name() {
        for title in [None, Some(""), Some(" \t\n")] {
            assert!(source_app_matches("Fixture", title, || Ok(vec!["fIXTURE".into()])).unwrap());
        }
    }

    #[test]
    fn blank_title_rejects_different_or_missing_inventory_name() {
        for names in [vec![], vec!["Other".into()]] {
            assert!(!source_app_matches("Fixture", Some(" "), || Ok(names)).unwrap());
        }
    }

    #[test]
    fn blank_title_matches_saved_cg_owner_names_for_same_pid() {
        let records = [(7, "CG Owner"), (7, "CG Alias"), (8, "Other PID")];
        for expected in ["CG Owner", "CG Alias"] {
            assert!(
                source_app_matches(expected, None, || Ok(source_names_for_pid(7, records)))
                    .unwrap()
            );
        }
        for expected in ["Workspace Display Name", "Other PID"] {
            assert!(
                !source_app_matches(expected, None, || Ok(source_names_for_pid(7, records)))
                    .unwrap()
            );
        }
    }

    #[test]
    fn matching_title_never_reads_inventory() {
        assert!(
            source_app_matches("Fixture", Some("fIXTURE"), || {
                panic!("matching AXTitle must not read inventory")
            })
            .unwrap()
        );
    }

    #[test]
    fn different_title_accepts_same_pid_inventory_alias() {
        let records = [(7, "Google Chrome"), (8, "Other PID")];
        assert!(
            source_app_matches("Google Chrome", Some("Chrome"), || {
                Ok(source_names_for_pid(7, records))
            })
            .unwrap()
        );
        assert!(
            !source_app_matches("Other PID", Some("Chrome"), || {
                Ok(source_names_for_pid(7, records))
            })
            .unwrap()
        );
    }

    #[test]
    fn different_title_rejects_missing_or_different_inventory_names() {
        for names in [vec![], vec!["Other".into()]] {
            assert!(!source_app_matches("Fixture", Some("Alias"), || Ok(names)).unwrap());
        }
    }

    #[test]
    fn inventory_read_error_propagates() {
        let error = source_app_matches("Fixture", None, || Err(AdapterError::timeout("inventory")))
            .unwrap_err();
        assert_eq!(error.code, agent_desktop_core::ErrorCode::Timeout);
        assert_eq!(error.message, "inventory");
    }
}
