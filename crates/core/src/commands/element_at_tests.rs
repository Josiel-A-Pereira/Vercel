use super::{attach_completeness, candidates_at, pick};
use crate::{AccessibilityNode, AdapterError, HitTestResult, NodePresentation, Point, Rect};

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

fn node(
    ref_id: Option<&str>,
    bounds: Option<Rect>,
    children: Vec<AccessibilityNode>,
) -> AccessibilityNode {
    AccessibilityNode {
        ref_id: ref_id.map(str::to_string),
        role: "button".into(),
        identity: Default::default(),
        presentation: NodePresentation {
            bounds,
            ..Default::default()
        },
        children_count: None,
        subtree_truncated: false,
        children,
    }
}

fn window() -> AccessibilityNode {
    node(
        None,
        Some(rect(0.0, 0.0, 800.0, 600.0)),
        vec![
            node(
                Some("@s1:e1"),
                Some(rect(100.0, 100.0, 400.0, 300.0)),
                vec![
                    node(Some("@s1:e2"), Some(rect(120.0, 120.0, 80.0, 30.0)), vec![]),
                    node(None, Some(rect(120.0, 120.0, 10.0, 10.0)), vec![]),
                ],
            ),
            node(Some("@s1:e3"), Some(rect(600.0, 100.0, 50.0, 50.0)), vec![]),
            node(Some("@s1:e4"), None, vec![]),
        ],
    )
}

fn refs(candidates: &[(&AccessibilityNode, Rect)]) -> Vec<String> {
    candidates
        .iter()
        .filter_map(|(node, _)| node.ref_id.clone())
        .collect()
}

#[test]
fn only_refs_containing_the_point_are_candidates_smallest_first() {
    let tree = window();
    let candidates = candidates_at(&tree, &Point { x: 130.0, y: 130.0 });
    assert_eq!(refs(&candidates), ["@s1:e2", "@s1:e1"]);
}

#[test]
fn a_point_on_the_far_edge_is_outside() {
    let tree = window();
    assert!(candidates_at(&tree, &Point { x: 650.0, y: 120.0 }).is_empty());
    assert_eq!(
        refs(&candidates_at(&tree, &Point { x: 649.0, y: 120.0 })),
        ["@s1:e3"]
    );
}

#[test]
fn equal_areas_put_the_deeper_element_first() {
    let tree = node(
        None,
        None,
        vec![node(
            Some("@s1:e1"),
            Some(rect(0.0, 0.0, 50.0, 50.0)),
            vec![node(
                Some("@s1:e2"),
                Some(rect(0.0, 0.0, 50.0, 50.0)),
                vec![],
            )],
        )],
    );
    assert_eq!(
        refs(&candidates_at(&tree, &Point { x: 10.0, y: 10.0 })),
        ["@s1:e2", "@s1:e1"]
    );
}

#[test]
fn the_first_candidate_the_point_reaches_is_chosen() {
    let tree = window();
    let candidates = candidates_at(&tree, &Point { x: 130.0, y: 130.0 });
    let chosen = pick(&candidates, |node| {
        Ok(match node.ref_id.as_deref() {
            Some("@s1:e1") => HitTestResult::ReachesTarget,
            _ => HitTestResult::Unknown,
        })
    });
    assert_eq!(chosen, Some((1, HitTestResult::ReachesTarget)));
}

#[test]
fn without_proof_the_smallest_is_returned_with_what_its_hit_test_saw() {
    let tree = window();
    let candidates = candidates_at(&tree, &Point { x: 130.0, y: 130.0 });
    let occluder = HitTestResult::InterceptedBy {
        role: Some("window".into()),
        name: Some("Other".into()),
        bounds: None,
    };
    let chosen = pick(&candidates, |_| Ok(occluder.clone()));
    assert_eq!(chosen, Some((0, occluder)));
}

#[test]
fn a_probe_error_counts_as_unknown() {
    let tree = window();
    let candidates = candidates_at(&tree, &Point { x: 130.0, y: 130.0 });
    let chosen = pick(&candidates, |_| {
        Err(AdapterError::not_supported("hit_test"))
    });
    assert_eq!(chosen, Some((0, HitTestResult::Unknown)));
}

#[test]
fn no_candidate_means_no_match() {
    assert_eq!(pick(&[], |_| Ok(HitTestResult::ReachesTarget)), None);
}

#[test]
fn a_complete_snapshot_says_so_without_partial_fields() {
    let mut response = serde_json::json!({ "match": null });
    attach_completeness(&mut response, true, 42);
    assert_eq!(response["complete"], true);
    assert!(response.get("truncated").is_none());
    assert!(response.get("nodes_observed").is_none());
}

#[test]
fn a_partial_snapshot_is_reported_like_snapshot_reports_it() {
    let mut response = serde_json::json!({ "match": null });
    attach_completeness(&mut response, false, 17);
    assert_eq!(response["complete"], false);
    assert_eq!(response["truncated"], true);
    assert_eq!(response["nodes_observed"], 17);
}
