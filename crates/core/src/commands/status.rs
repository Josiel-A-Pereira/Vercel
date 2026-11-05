//! Reports state without writing it. A missing selected snapshot permits a fresh
//! read only when the latest pointer changed, under one inherited deadline.

use crate::{
    AppError, PermissionReport,
    adapter::PlatformAdapter,
    commands::permissions::{self, PermissionsArgs},
    context::CommandContext,
    refs::refs_store::RefStore,
};
use serde_json::{Value, json};

pub fn execute_with_report_with_context(
    adapter: &dyn PlatformAdapter,
    report: &PermissionReport,
    context: &CommandContext,
) -> Result<Value, AppError> {
    let permissions = permissions::execute_with_report(
        PermissionsArgs {
            request: false,
            guide: false,
        },
        adapter,
        report,
    )?;

    let store = RefStore::for_session(context.session_id())?;
    let (snapshot_id, ref_count) = match snapshot_status(&store)? {
        Some((id, count)) => (Some(id), Some(count)),
        None => (None, None),
    };
    let session_id = context.session_id().map(str::to_string);
    let tracing = context.trace_enabled();
    let artifacts = session_id
        .as_deref()
        .and_then(|id| crate::session::read_manifest(id).ok().flatten())
        .map(|manifest| manifest.artifacts);

    let mut body = json!({
        "platform": std::env::consts::OS,
        "version": env!("CARGO_PKG_VERSION"),
        "permissions": permissions,
        "snapshot_id": snapshot_id,
        "ref_count": ref_count,
        "session_id": session_id,
        "tracing": tracing,
        "supported_surfaces": adapter
            .supported_surfaces()
            .into_iter()
            .map(|surface| surface.as_str())
            .collect::<Vec<_>>(),
    });
    if let Some(artifacts) = artifacts {
        body["artifacts"] = json!(artifacts);
    }
    if let Ok(state_root) = crate::storage::state_root::resolve_configured_state_root() {
        body["state_root"] = json!(state_root.to_string_lossy());
    }
    Ok(body)
}

fn snapshot_status(store: &RefStore) -> Result<Option<(String, usize)>, AppError> {
    let Some(mut id) = store.latest_snapshot_id()? else {
        return Ok(None);
    };
    let mut deadline: Option<crate::Deadline> = None;
    loop {
        if let Some(deadline) = deadline {
            deadline.ensure_remaining()?;
        }
        match store.load_snapshot(&id) {
            Ok(map) => return Ok(Some((id, map.len()))),
            Err(error) => {
                if !matches!(&error, AppError::Adapter(error) if error.code == crate::ErrorCode::SnapshotNotFound)
                {
                    return Err(error);
                }
                let Some(replacement) = store.latest_snapshot_id()? else {
                    return Err(error);
                };
                if replacement == id {
                    return Err(error);
                }
                if deadline.is_none() {
                    deadline = Some(crate::Deadline::standard()?);
                }
                id = replacement;
            }
        }
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "status_rollover_tests.rs"]
mod status_rollover_tests;
