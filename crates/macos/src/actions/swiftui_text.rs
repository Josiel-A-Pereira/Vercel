//! On macOS 27 a SwiftUI text field commits an accessibility write to its
//! value or selection into the app only while it has keyboard focus.
//! Unfocused, the write changes what the field displays and reads back, and
//! the app never sees it: a read-back would "verify" text the app does not
//! hold. Such a field is focused first when the policy allows focus, and
//! refused before any write otherwise.
//!
//! SwiftUI-hosted fields are recognised by attributes AppKit and Chromium
//! fields do not expose (`AXAutoInteractable`, `AXTraits`). Both must read
//! successfully; `AXRespondsToInteraction` is listed too but fails to read.

use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode, InteractionPolicy};

use crate::tree::AXElement;

const FIRST_AFFECTED_MAJOR: u32 = 27;
const TEXT_ROLES: &[&str] = &["AXTextField", "AXComboBox", "AXSearchField"];
const SWIFTUI_MARKERS: &[&str] = &["AXAutoInteractable", "AXTraits"];

/// Make a text write to `element` reach the app, or refuse it unsent.
pub(crate) fn prepare_text_write(
    element: &AXElement,
    policy: InteractionPolicy,
    deadline: Deadline,
) -> Result<(), AdapterError> {
    if !write_needs_focus(element, deadline) {
        return Ok(());
    }
    if policy.allow_focus_steal {
        crate::actions::ax_helpers::set_ax_bool_or_err(element, "AXFocused", true, deadline)?;
        return Ok(());
    }
    Err(AdapterError::new(
        ErrorCode::PolicyDenied,
        "On macOS 27 this SwiftUI text field takes text only while it has keyboard focus, and headless mode never focuses it",
    )
    .with_suggestion("Run the command with --headed, or focus the field with 'agent-desktop --headed focus <ref>' first")
    .with_details(serde_json::json!({ "kind": "swiftui_text_needs_focus", "retryable": false }))
    .with_disposition(DeliverySemantics::not_delivered()))
}

fn write_needs_focus(element: &AXElement, deadline: Deadline) -> bool {
    if macos_major().is_none_or(|major| major < FIRST_AFFECTED_MAJOR) {
        return false;
    }
    let read_string = |attribute| {
        crate::tree::properties::attributes::copy_string_attr_result(element, attribute, deadline)
            .ok()
            .flatten()
    };
    let read_bool = |attribute| {
        crate::tree::properties::attributes::copy_bool_attr_result(element, attribute, deadline)
    };
    read_string("AXRole").is_some_and(|role| TEXT_ROLES.contains(&role.as_str()))
        && SWIFTUI_MARKERS
            .iter()
            .all(|marker| reads_successfully(element, marker, deadline))
        && read_bool("AXFocused") != Ok(Some(true))
}

fn reads_successfully(element: &AXElement, attribute: &str, deadline: Deadline) -> bool {
    use core_foundation::{base::TCFType, string::CFString};
    let name = CFString::new(attribute);
    let (error, raw) = crate::tree::properties::ax_ipc::copy_attribute_value(
        element,
        name.as_concrete_TypeRef(),
        deadline,
    );
    if !raw.is_null() {
        unsafe { core_foundation::base::CFRelease(raw) };
    }
    error == accessibility_sys::kAXErrorSuccess
}

fn macos_major() -> Option<u32> {
    let mut buffer = [0_u8; 32];
    let mut length = buffer.len();
    let name = c"kern.osproductversion";
    let status = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut length,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 {
        return None;
    }
    parse_major(std::str::from_utf8(&buffer[..length]).ok()?)
}

fn parse_major(version: &str) -> Option<u32> {
    version
        .trim_end_matches('\0')
        .split('.')
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
#[path = "swiftui_text_tests.rs"]
mod tests;
