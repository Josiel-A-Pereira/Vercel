//! Locates a requested prefix under a provider's own Character unit.

use agent_desktop_core::AdapterError;
use std::cmp::Ordering;

pub(super) fn resolve_endpoint(
    expected: &str,
    mut prefix_text: impl FnMut(usize) -> Result<String, AdapterError>,
) -> Result<Option<usize>, AdapterError> {
    if expected.is_empty() {
        return Ok(Some(0));
    }
    let candidates = [
        expected.chars().count(),
        expected.encode_utf16().count(),
        expected.len(),
    ];
    for count in candidates {
        if prefix_text(count)? == expected {
            return Ok(Some(count));
        }
    }
    let mut low = 0usize;
    let mut high = candidates.into_iter().max().unwrap_or(0);
    while low <= high {
        let middle = low + (high - low) / 2;
        let observed = prefix_text(middle)?;
        match observed.as_str().cmp(expected) {
            Ordering::Equal => return Ok(Some(middle)),
            Ordering::Less => low = middle + 1,
            Ordering::Greater if middle == 0 => break,
            Ordering::Greater => high = middle - 1,
        }
    }
    Ok(None)
}
