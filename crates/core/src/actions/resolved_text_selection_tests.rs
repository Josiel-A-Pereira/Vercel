use crate::TextSelection;
use serde_json::json;

#[test]
fn verified_selection_reports_scalar_offsets_and_only_known_counts() {
    let selection = TextSelection::Range { start: 1, end: 2 }
        .resolve("😀a")
        .unwrap();
    let unknown = selection.verified_result("native selection", None);
    let details = unknown.details.unwrap();
    assert_eq!(
        details["range"],
        json!({"start": 1, "end": 2, "unit": "unicode_scalar"})
    );
    assert!(details.get("selection_count").is_none());
    let known = selection.verified_result("native selection", Some(1));
    assert_eq!(known.details.unwrap()["selection_count"], json!(1));
}
