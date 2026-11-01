use super::tests::entry;
use super::*;

#[test]
fn test_serialize_with_size_check_rejects_oversized() {
    let mut map = RefMap::new();
    let big_name = "x".repeat(2048);
    for _ in 0..600 {
        map.allocate(entry("button", Some(&big_name)));
    }

    let err = map.serialize_with_size_check().unwrap_err();
    assert_eq!(err.code(), "TREE_TOO_LARGE");
    assert!(
        err.to_string().contains(&MAX_REFMAP_BYTES.to_string()),
        "got: {err}"
    );
    let suggestion = err.suggestion().unwrap_or_default();
    assert!(
        suggestion.contains("--max-depth")
            && suggestion.contains("--skeleton")
            && suggestion.contains("--root")
            && suggestion.contains("find"),
        "got: {suggestion}"
    );

    let payload = crate::response::output::ErrorPayload::from_app_error(&err);
    assert_eq!(payload.code, "TREE_TOO_LARGE");
    assert!(payload.recovery.is_none());
    assert_eq!(
        payload.disposition,
        crate::DeliverySemantics::not_delivered()
    );
    let details = payload
        .details
        .expect("an oversized refmap reports its size");
    assert_eq!(details["kind"], "refmap_too_large");
    assert_eq!(details["limit_bytes"], MAX_REFMAP_BYTES);
    assert!(details["serialized_bytes"].as_u64().unwrap() > MAX_REFMAP_BYTES);
    assert_eq!(details["retryable"], true);
}

#[test]
fn test_serialize_with_size_check_accepts_normal() {
    let mut map = RefMap::new();
    for _ in 0..50 {
        map.allocate(entry("button", Some("OK")));
    }

    let result = map.serialize_with_size_check();
    let value: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(value["inner"]["@e1"]["role"], "button");
    assert_eq!(value["inner"]["@e1"]["name"], "OK");
}

/// An oversized refmap used to surface as a bare `INTERNAL`, which reads as
/// a crash. The caller can act on it, so it carries its own code, the size
/// it measured against the cap, and a safe disposition for a narrower retry.
#[test]
fn tree_too_large_names_the_size_the_cap_and_a_safe_narrower_retry() {
    let error = tree_too_large(1_200_000);
    assert_eq!(error.code, crate::ErrorCode::TreeTooLarge);
    assert_eq!(error.code.as_str(), "TREE_TOO_LARGE");
    assert!(error.message.contains("1200000") && error.message.contains("1048576"));
    let suggestion = error
        .suggestion
        .as_deref()
        .expect("tree_too_large should carry a suggestion");
    assert!(
        suggestion.contains("--max-depth")
            && suggestion.contains("--skeleton")
            && suggestion.contains("--root")
    );
    let details = error
        .details
        .as_ref()
        .expect("tree_too_large should carry size details");
    assert_eq!(details["kind"], "refmap_too_large");
    assert_eq!(details["serialized_bytes"], 1_200_000);
    assert_eq!(details["limit_bytes"], MAX_REFMAP_BYTES);
    assert_eq!(error.disposition, crate::DeliverySemantics::not_delivered());
    assert!(error.is_explicitly_retryable());
}
