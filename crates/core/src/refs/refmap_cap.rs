use super::MAX_REFMAP_BYTES;
use crate::{AdapterError, AppError, DeliverySemantics, ErrorCode};
use serde_json::json;

/// `TREE_TOO_LARGE` for a refmap that serialized to `serialized_bytes`, above
/// `MAX_REFMAP_BYTES`. The observation finished and nothing reached the
/// application, so the disposition is `not_delivered` and a narrower retry is
/// safe. `details` carries the measured size and the cap so the caller can
/// judge how far to narrow.
pub(crate) fn tree_too_large(serialized_bytes: u64) -> AdapterError {
    AdapterError::new(
        ErrorCode::TreeTooLarge,
        format!(
            "RefMap serialized to {serialized_bytes} bytes, above the {MAX_REFMAP_BYTES} byte limit for one snapshot"
        ),
    )
    .with_suggestion(
        "Narrow the observation and retry: snapshot with a lower --max-depth, or snapshot --skeleton \
         and then --root @<snapshot_id>:eN to drill into one subtree; for find, tighten the query or lower --limit",
    )
    .with_details(json!({
        "kind": "refmap_too_large",
        "serialized_bytes": serialized_bytes,
        "limit_bytes": MAX_REFMAP_BYTES,
        "retryable": true,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

/// Adds the depth an observation ran at to a `TREE_TOO_LARGE` error raised
/// while its refmap was being saved, so the caller can choose a lower
/// `--max-depth` from the envelope instead of guessing. Any other error
/// passes through unchanged.
pub(crate) fn note_observed_depth(error: AppError, max_depth: u8) -> AppError {
    match error {
        AppError::Adapter(mut adapter) if adapter.code == ErrorCode::TreeTooLarge => {
            if let Some(serde_json::Value::Object(details)) = adapter.details.as_mut() {
                details.insert("max_depth".to_owned(), json!(max_depth));
            }
            AppError::Adapter(adapter)
        }
        other => other,
    }
}

#[cfg(test)]
#[path = "refmap_cap_tests.rs"]
mod tests;
