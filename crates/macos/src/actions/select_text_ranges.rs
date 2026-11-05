use std::ops::Range;

use accessibility_sys::{
    AXValueGetType, AXValueGetTypeID, AXValueGetValue, kAXErrorSuccess, kAXValueTypeCFRange,
};
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use core_foundation::{
    base::{CFGetTypeID, CFTypeRef, TCFType},
    string::CFString,
};
use core_foundation_sys::base::CFRange;

use crate::tree::AXElement;

const RANGE_ATTRIBUTE: &str = "AXSelectedTextRange";
const RANGES_ATTRIBUTE: &str = "AXSelectedTextRanges";

pub(super) struct SelectionRead {
    pub(super) count: Option<usize>,
    pub(super) only_range: Option<Range<usize>>,
}

pub(super) fn read_selection_ranges(
    element: &AXElement,
    deadline: Deadline,
) -> Result<SelectionRead, AdapterError> {
    let attribute = CFString::new(RANGES_ATTRIBUTE);
    let (error, raw) = crate::tree::properties::ax_ipc::copy_attribute_value(
        element,
        attribute.as_concrete_TypeRef(),
        deadline,
    );
    if error != kAXErrorSuccess {
        release_if_present(raw);
        if crate::tree::properties::ax_absence::is_absent_attribute_error(error) {
            return Ok(SelectionRead {
                count: None,
                only_range: None,
            });
        }
        return Err(crate::system::focus::map_ax_read_error(
            error,
            "read selected text ranges",
        ));
    }
    let Some(array) = crate::tree::properties::cf_type::created_cf_array(raw) else {
        return Err(malformed("Selected text ranges are not an array"));
    };
    let count = array.len() as usize;
    let only_range = if count == 1 {
        let item = array
            .get(0)
            .ok_or_else(|| malformed("Selected text range is missing"))?;
        Some(decode_range(item.as_CFTypeRef())?)
    } else {
        None
    };
    Ok(SelectionRead {
        count: Some(count),
        only_range,
    })
}

pub(super) fn read_single_selection(
    element: &AXElement,
    deadline: Deadline,
) -> Result<Range<usize>, AdapterError> {
    let attribute = CFString::new(RANGE_ATTRIBUTE);
    let (error, raw) = crate::tree::properties::ax_ipc::copy_attribute_value(
        element,
        attribute.as_concrete_TypeRef(),
        deadline,
    );
    if error != kAXErrorSuccess {
        release_if_present(raw);
        return Err(crate::system::focus::map_ax_read_error(
            error,
            "read selected text range",
        ));
    }
    let range = decode_range(raw);
    release_if_present(raw);
    range
}

pub(super) fn selection_matches(
    selected: &SelectionRead,
    single: &Range<usize>,
    expected: &Range<usize>,
) -> bool {
    single == expected
        && selected.count.is_none_or(|count| {
            if expected.is_empty() {
                count <= 1 && (count == 0 || selected.only_range.as_ref() == Some(expected))
            } else {
                count == 1 && selected.only_range.as_ref() == Some(expected)
            }
        })
}

fn decode_range(raw: CFTypeRef) -> Result<Range<usize>, AdapterError> {
    if raw.is_null()
        || unsafe { CFGetTypeID(raw) } != unsafe { AXValueGetTypeID() }
        || unsafe { AXValueGetType(raw as _) } != kAXValueTypeCFRange
    {
        return Err(malformed("Selected text range has an invalid type"));
    }
    let mut range = CFRange {
        location: 0,
        length: 0,
    };
    if !unsafe {
        AXValueGetValue(
            raw as _,
            kAXValueTypeCFRange,
            (&mut range as *mut CFRange).cast(),
        )
    } {
        return Err(malformed("Selected text range could not be decoded"));
    }
    let start = usize::try_from(range.location)
        .map_err(|_| malformed("Selected text range has a negative offset"))?;
    let length = usize::try_from(range.length)
        .map_err(|_| malformed("Selected text range has a negative length"))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| malformed("Selected text range overflows"))?;
    Ok(start..end)
}

fn malformed(message: &str) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

fn release_if_present(value: CFTypeRef) {
    if !value.is_null() {
        unsafe { core_foundation::base::CFRelease(value) };
    }
}
