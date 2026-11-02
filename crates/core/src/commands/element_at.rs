//! `element-at`: the element under a screen point, as a ref an agent can act
//! on semantically instead of synthesizing a pointer click at the point.
//!
//! A snapshot of the target window supplies every ref with its screen
//! rectangle. Refs whose rectangle contains the point are tried from the
//! smallest up, and the platform hit test decides which one the point
//! reaches. When none can be proved, the smallest candidate is returned with
//! what the hit test saw, so the caller chooses between a semantic action on
//! that ref and physical input.

use crate::{
    AccessibilityNode, AdapterError, AppError, HitTestResult, Point, Rect,
    adapter::{PlatformAdapter, TreeOptions},
    context::CommandContext,
    snapshot,
};
use serde_json::{Value, json};

/// How many candidates are hit-tested before the smallest is returned unproved.
const MAX_HIT_TESTS: usize = 8;

pub struct ElementAtArgs {
    pub app: Option<String>,
    pub window_id: Option<String>,
    pub point: Point,
    pub max_depth: u8,
    pub timeout_ms: Option<u64>,
}

pub fn execute(
    args: ElementAtArgs,
    adapter: &dyn PlatformAdapter,
    context: &CommandContext,
) -> Result<Value, AppError> {
    validate_point(&args.point)?;
    let opts = TreeOptions {
        max_depth: args.max_depth,
        include_bounds: true,
        interactive_only: true,
        compact: true,
        ..TreeOptions::default()
    };
    let timeout_ms = args
        .timeout_ms
        .unwrap_or(snapshot::DEFAULT_SNAPSHOT_TIMEOUT_MS);
    let result = snapshot::run_with_context(
        adapter,
        &opts,
        &snapshot::SnapshotTarget {
            app_name: args.app.as_deref(),
            window_id: args.window_id.as_deref(),
        },
        context,
        timeout_ms,
    )?;
    let candidates = candidates_at(&result.tree, &args.point);
    let deadline = crate::Deadline::after(timeout_ms)?;
    let chosen = pick(&candidates, |node| {
        let local_ref = local_ref_of(node)?;
        let entry = result.refmap.get(&local_ref).ok_or_else(|| {
            AdapterError::new(crate::ErrorCode::StaleRef, "Ref is not in the snapshot")
        })?;
        let handle = adapter.resolve_element_strict(entry, deadline)?;
        adapter.hit_test(&handle, args.point.clone(), deadline)
    });
    Ok(response(&result, &args.point, &candidates, chosen))
}

fn validate_point(point: &Point) -> Result<(), AppError> {
    if point.x.is_finite() && point.y.is_finite() {
        return Ok(());
    }
    Err(AppError::invalid_input(
        "Coordinates must be finite numbers",
    ))
}

fn local_ref_of(node: &AccessibilityNode) -> Result<String, AdapterError> {
    let ref_id = node.ref_id.as_deref().ok_or_else(|| {
        AdapterError::new(crate::ErrorCode::ElementNotFound, "Candidate has no ref")
    })?;
    crate::refs::ref_token::resolve_ref_target(ref_id)
        .map(|(_, local_ref)| local_ref)
        .map_err(|error| AdapterError::new(crate::ErrorCode::StaleRef, error.to_string()))
}

/// Refs whose screen rectangle contains `point`, smallest first. Among equal
/// areas the deeper element comes first, since a child drawn over its parent
/// is what the point lands on.
pub(crate) fn candidates_at<'a>(
    tree: &'a AccessibilityNode,
    point: &Point,
) -> Vec<(&'a AccessibilityNode, Rect)> {
    let mut found = Vec::new();
    collect(tree, point, &mut found);
    found.reverse();
    found.sort_by(|(_, a), (_, b)| area(a).total_cmp(&area(b)));
    found
}

fn collect<'a>(
    node: &'a AccessibilityNode,
    point: &Point,
    found: &mut Vec<(&'a AccessibilityNode, Rect)>,
) {
    if node.ref_id.is_some()
        && let Some(bounds) = node.presentation.bounds
        && contains(&bounds, point)
    {
        found.push((node, bounds));
    }
    for child in &node.children {
        collect(child, point, found);
    }
}

fn contains(bounds: &Rect, point: &Point) -> bool {
    bounds.width > 0.0
        && bounds.height > 0.0
        && point.x >= bounds.x
        && point.y >= bounds.y
        && point.x < bounds.x + bounds.width
        && point.y < bounds.y + bounds.height
}

fn area(bounds: &Rect) -> f64 {
    bounds.width * bounds.height
}

/// The candidate the point reaches, else the smallest one with what its hit
/// test reported. A probe error counts as an unknown hit: unavailable
/// evidence never rules a candidate out.
pub(crate) fn pick(
    candidates: &[(&AccessibilityNode, Rect)],
    mut probe: impl FnMut(&AccessibilityNode) -> Result<HitTestResult, AdapterError>,
) -> Option<(usize, HitTestResult)> {
    let mut smallest = None;
    for (index, (node, _)) in candidates.iter().enumerate().take(MAX_HIT_TESTS) {
        let hit = probe(node).unwrap_or(HitTestResult::Unknown);
        if hit == HitTestResult::ReachesTarget {
            return Some((index, hit));
        }
        if smallest.is_none() {
            smallest = Some((index, hit));
        }
    }
    smallest
}

fn response(
    result: &snapshot::SnapshotResult,
    point: &Point,
    candidates: &[(&AccessibilityNode, Rect)],
    chosen: Option<(usize, HitTestResult)>,
) -> Value {
    let mut response = json!({
        "snapshot_id": result.snapshot_id,
        "app": result.window.app,
        "window_id": result.window.id,
        "point": { "x": point.x, "y": point.y },
        "candidates": candidates.len(),
        "match": Value::Null,
    });
    attach_completeness(&mut response, result.complete, result.nodes_observed);
    let Some((index, hit)) = chosen else {
        return response;
    };
    let (node, bounds) = candidates[index];
    response["match"] = json!({
        "ref_id": node.ref_id,
        "role": node.role,
        "name": node.identity.name,
        "bounds": bounds,
        "available_actions": node.presentation.available_actions,
    });
    match hit {
        HitTestResult::ReachesTarget => response["hit"] = json!("reaches_target"),
        HitTestResult::InterceptedBy { role, name, bounds } => {
            response["hit"] = json!("intercepted");
            response["intercepted_by"] = json!({ "role": role, "name": name, "bounds": bounds });
        }
        HitTestResult::Unknown => response["hit"] = json!("unknown"),
    }
    response
}

/// Reports whether the snapshot behind the candidates saw the whole window,
/// with the same fields `snapshot` uses. A partial snapshot can miss the
/// element under the point, so a null match or an ancestor match from it is
/// not evidence that nothing more specific is there.
pub(crate) fn attach_completeness(response: &mut Value, complete: bool, nodes_observed: usize) {
    response["complete"] = json!(complete);
    if !complete {
        response["truncated"] = json!(true);
        response["nodes_observed"] = json!(nodes_observed);
    }
}

#[cfg(test)]
#[path = "element_at_tests.rs"]
mod tests;
