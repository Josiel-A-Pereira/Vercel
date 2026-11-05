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
