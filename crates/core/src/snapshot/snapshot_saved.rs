use crate::{AppError, context::CommandContext, snapshot::SnapshotResult};
use serde_json::json;

pub(crate) fn emit_snapshot_saved(
    context: &CommandContext,
    result: &SnapshotResult,
) -> Result<(), AppError> {
    context.trace_lazy("snapshot.saved", || {
        let mut fields = json!({
            "snapshot_id": result.snapshot_id,
            "ref_count": result.refmap.len(),
        });
        if !result.window.app.is_empty() {
            fields["app"] = json!(result.window.app);
        }
        if let Some(reason) = &result.identity_unproven {
            fields["identity_unproven"] = json!(reason);
        }
        fields
    })
}
