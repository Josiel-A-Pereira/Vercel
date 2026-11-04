use super::super::child_read_budget::ChildReadBudget;
use super::{ChildRead, ChildReadStatus, ChildSourceAvailability, read_attribute_children};
use crate::tree::AXElement;
use accessibility_sys::kAXErrorSuccess;
use core_foundation::{
    base::{CFRelease, CFType, TCFType},
    number::CFNumber,
    string::CFString,
};
use std::time::Instant;

const TABLE_ROLE: &str = "AXTable";

pub(super) fn read(
    element: &AXElement,
    role: Option<&str>,
    budget: ChildReadBudget,
    deadline: Instant,
    visible_table_rows_only: bool,
) -> ChildRead {
    if role != Some(TABLE_ROLE) || !visible_table_rows_only {
        return read_standard(element, role, budget, deadline);
    }

    let count_only_budget = ChildReadBudget {
        max_elements: 0,
        boundary: true,
    };
    if budget.max_elements == 0 {
        let rows = read_attribute_children(element, "AXRows", budget, deadline);
        if rows.source_availability == ChildSourceAvailability::Available {
            return rows;
        }
        return read_standard(element, role, budget, deadline);
    }

    let mut visible = read_attribute_children(element, "AXVisibleRows", budget, deadline);
    if visible.source_availability != ChildSourceAvailability::Available {
        return read_standard_with_status(element, role, budget, deadline, visible.status);
    }

    let rows = read_attribute_children(element, "AXRows", count_only_budget, deadline);
    let offset = source_index_offset(&visible.elements, deadline, &mut visible.status);
    match finalize_visible_table_read(visible, rows, offset) {
        Ok(read) => read,
        Err(status) => read_standard_with_status(element, role, budget, deadline, status),
    }
}

fn read_standard_with_status(
    element: &AXElement,
    role: Option<&str>,
    budget: ChildReadBudget,
    deadline: Instant,
    prior_status: ChildReadStatus,
) -> ChildRead {
    let mut read = read_standard(element, role, budget, deadline);
    read.status.merge(prior_status);
    read
}

fn read_standard(
    element: &AXElement,
    role: Option<&str>,
    budget: ChildReadBudget,
    deadline: Instant,
) -> ChildRead {
    super::super::child_source::read_resilient(
        crate::tree::properties::element::child_attributes(role),
        deadline,
        |attribute| read_attribute_children(element, attribute, budget, deadline),
    )
}

fn source_index_offset(
    elements: &[AXElement],
    deadline: Instant,
    status: &mut ChildReadStatus,
) -> Option<usize> {
    contiguous_source_index_offset(elements.iter().map(|row| {
        status.attempts += 1;
        row_index(row, deadline)
    }))
}

fn contiguous_source_index_offset(
    indexes: impl IntoIterator<Item = Option<usize>>,
) -> Option<usize> {
    let mut offset = None;
    for (position, index) in indexes.into_iter().enumerate() {
        let candidate = index?.checked_sub(position)?;
        if offset.is_some_and(|expected| expected != candidate) {
            return None;
        }
        offset = Some(candidate);
    }
    Some(offset.unwrap_or(0))
}

fn source_range_within_total(offset: usize, count: usize, total: usize) -> bool {
    offset.checked_add(count).is_some_and(|end| end <= total)
}

fn finalize_visible_table_read(
    mut visible: ChildRead,
    rows: ChildRead,
    source_index_offset: Option<usize>,
) -> Result<ChildRead, ChildReadStatus> {
    let exact_total = (rows.source_availability == ChildSourceAvailability::Available
        && rows.complete
        && !rows.status.count_changed)
        .then_some(rows.total_count);
    visible.status.merge(rows.status);
    let Some(total) = exact_total else {
        return Err(visible.status);
    };
    let Some(source_index_offset) = source_index_offset else {
        return Err(visible.status);
    };
    if !source_range_within_total(source_index_offset, visible.elements.len(), total) {
        return Err(visible.status);
    }
    visible.total_count = total;
    visible.complete = visible.complete
        && source_index_offset == 0
        && visible.elements.len() == total
        && !visible.status.count_changed;
    visible.prefix_certain = source_index_offset == 0;
    visible.source_index_offset = Some(source_index_offset);
    Ok(visible)
}

fn row_index(element: &AXElement, deadline: Instant) -> Option<usize> {
    let attribute = CFString::new("AXIndex");
    let (error, value) = crate::tree::properties::ax_ipc::copy_attribute_value(
        element,
        attribute.as_concrete_TypeRef(),
        deadline,
    );
    if error != kAXErrorSuccess || value.is_null() {
        if !value.is_null() {
            unsafe { CFRelease(value) };
        }
        return None;
    }
    let value = unsafe { CFType::wrap_under_create_rule(value) };
    usize::try_from(value.downcast::<CFNumber>()?.to_i64()?).ok()
}

#[cfg(test)]
#[path = "table_child_read_tests.rs"]
mod tests;
