use super::{
    ChildRead, ChildReadStatus, ChildSourceAvailability, contiguous_source_index_offset,
    finalize_visible_table_read, source_range_within_total,
};
use crate::tree::AXElement;

fn child_read(element_count: usize, total_count: usize, complete: bool) -> ChildRead {
    ChildRead {
        elements: (0..element_count)
            .map(|_| AXElement(std::ptr::null_mut()))
            .collect(),
        source_index_offset: None,
        total_count,
        complete,
        source_availability: ChildSourceAvailability::Available,
        prefix_certain: complete,
        status: ChildReadStatus::default(),
    }
}

fn accepted_visible_read(
    visible: ChildRead,
    rows: ChildRead,
    source_index_offset: Option<usize>,
) -> ChildRead {
    match finalize_visible_table_read(visible, rows, source_index_offset) {
        Ok(read) => read,
        Err(_) => panic!("visible table read should be accepted"),
    }
}

#[test]
fn visible_rows_keep_their_original_table_indexes() {
    assert_eq!(
        contiguous_source_index_offset([Some(12), Some(13), Some(14)]),
        Some(12)
    );
}

#[test]
fn full_visible_rows_start_at_zero() {
    assert_eq!(
        contiguous_source_index_offset([Some(0), Some(1), Some(2)]),
        Some(0)
    );
}

#[test]
fn empty_visible_rows_have_a_zero_offset() {
    assert_eq!(contiguous_source_index_offset([]), Some(0));
}

#[test]
fn noncontiguous_or_reordered_rows_require_the_fallback() {
    assert_eq!(
        contiguous_source_index_offset([Some(12), Some(14), Some(15)]),
        None
    );
    assert_eq!(
        contiguous_source_index_offset([Some(12), Some(11), Some(13)]),
        None
    );
    assert_eq!(
        contiguous_source_index_offset([Some(usize::MAX), Some(0)]),
        None
    );
    assert_eq!(contiguous_source_index_offset([Some(12), None]), None);
}

#[test]
fn visible_row_range_must_fit_the_reported_table_size() {
    assert!(source_range_within_total(12, 3, 20));
    assert!(source_range_within_total(15, 5, 20));
    assert!(!source_range_within_total(18, 3, 20));
    assert!(!source_range_within_total(usize::MAX, 1, usize::MAX));
}

#[test]
fn visible_slice_preserves_the_total_and_native_offset() {
    let visible = child_read(3, 3, true);
    let rows = child_read(0, 20, true);

    let read = accepted_visible_read(visible, rows, Some(12));

    assert_eq!(read.elements.len(), 3);
    assert_eq!(read.total_count, 20);
    assert_eq!(read.source_index_offset, Some(12));
    assert!(!read.complete);
    assert!(!read.prefix_certain);
    assert!(read.truncated());
}

#[test]
fn a_complete_visible_table_remains_complete() {
    let read = accepted_visible_read(child_read(3, 3, true), child_read(0, 3, true), Some(0));

    assert!(read.complete);
    assert!(read.prefix_certain);
    assert!(!read.truncated());
}

#[test]
fn a_visible_prefix_is_not_mistaken_for_a_complete_table() {
    let read = accepted_visible_read(child_read(3, 3, true), child_read(0, 20, true), Some(0));

    assert!(!read.complete);
    assert!(read.prefix_certain);
    assert_eq!(read.total_count, 20);
}

#[test]
fn unstable_or_invalid_table_ranges_fall_back() {
    let mut changed_rows = child_read(0, 20, false);
    changed_rows.status.count_changed = true;
    let changed = finalize_visible_table_read(child_read(3, 3, true), changed_rows, Some(12));
    assert!(changed.is_err());

    let out_of_range =
        finalize_visible_table_read(child_read(3, 3, true), child_read(0, 14, true), Some(12));
    assert!(out_of_range.is_err());

    let missing_offset =
        finalize_visible_table_read(child_read(3, 3, true), child_read(0, 20, true), None);
    assert!(missing_offset.is_err());
}
