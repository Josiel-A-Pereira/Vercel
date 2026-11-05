use super::*;

/// An oversized refmap used to surface as a bare `INTERNAL`, which reads as
/// a crash. The caller can act on it, so it carries its own code, the size
/// it measured against the cap, and a safe disposition for a narrower retry.
#[test]
fn tree_too_large_names_the_size_the_cap_and_a_safe_narrower_retry() {
    let error = tree_too_large(1_200_000);
    assert_eq!(error.code, ErrorCode::TreeTooLarge);
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
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(error.is_explicitly_retryable());
}

#[test]
fn note_observed_depth_adds_the_depth_only_to_a_tree_too_large_error() {
    let too_large: AppError = tree_too_large(2_000_000).into();
    let AppError::Adapter(noted) = note_observed_depth(too_large, 8) else {
        panic!("expected an adapter error");
    };
    assert_eq!(noted.code, ErrorCode::TreeTooLarge);
    let details = noted.details.as_ref().expect("size details");
    assert_eq!(details["max_depth"], 8);
    assert_eq!(details["serialized_bytes"], 2_000_000);
    assert!(
        noted.is_explicitly_retryable(),
        "adding the depth must not drop the retryable stamp"
    );

    let other: AppError = AdapterError::internal("unrelated").into();
    let AppError::Adapter(untouched) = note_observed_depth(other, 8) else {
        panic!("expected an adapter error");
    };
    assert!(untouched.details.is_none());
}
