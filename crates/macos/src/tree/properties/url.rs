const AX_URL_ATTRIBUTE: &str = "AXURL";

#[cfg(target_os = "macos")]
mod imp {
    use crate::{
        actions::ax_helpers::read_failure,
        tree::{
            AXElement,
            properties::{ax_absence, ax_ipc},
        },
    };
    use accessibility_sys::kAXErrorSuccess;
    use agent_desktop_core::AdapterError;
    use core_foundation::{
        base::{CFType, TCFType},
        string::CFString,
    };
    use core_foundation_sys::{
        base::{CFRelease, CFTypeRef},
        string::CFStringGetTypeID,
        url::{CFURLGetString, CFURLGetTypeID},
    };

    /// Reads the `AXURL` attribute an element exposes for navigation: a CFURL
    /// on links, and occasionally a plain string on hand-rolled controls. An
    /// element that navigates nowhere answers absence, not an error.
    pub(crate) fn read(
        element: &AXElement,
        deadline: agent_desktop_core::Deadline,
    ) -> Result<Option<String>, AdapterError> {
        ax_ipc::prepare(element, deadline)?;
        let attribute = CFString::new(super::AX_URL_ATTRIBUTE);
        let (error, raw) =
            ax_ipc::copy_attribute_value(element, attribute.as_concrete_TypeRef(), deadline);
        if error != kAXErrorSuccess || raw.is_null() {
            release(raw);
            return if error == kAXErrorSuccess || ax_absence::is_absent_attribute_error(error) {
                Ok(None)
            } else {
                Err(read_failure(super::AX_URL_ATTRIBUTE, error))
            };
        }
        Ok(url_text(unsafe { CFType::wrap_under_create_rule(raw) }))
    }

    fn release(raw: CFTypeRef) {
        if !raw.is_null() {
            unsafe { CFRelease(raw) };
        }
    }

    fn url_text(value: CFType) -> Option<String> {
        let type_id = value.type_of();
        let string = if type_id == unsafe { CFURLGetTypeID() } {
            let raw = unsafe { CFURLGetString(value.as_concrete_TypeRef() as _) };
            if raw.is_null() {
                return None;
            }
            unsafe { CFString::wrap_under_get_rule(raw) }
        } else if type_id == unsafe { CFStringGetTypeID() } {
            unsafe { CFString::wrap_under_get_rule(value.as_concrete_TypeRef() as _) }
        } else {
            return None;
        };
        let text = string.to_string();
        (!text.trim().is_empty()).then_some(text)
    }

    #[cfg(test)]
    mod tests {
        use super::url_text;
        use core_foundation::{
            base::{CFType, TCFType},
            number::CFNumber,
            string::CFString,
        };
        use core_foundation_sys::url::CFURLCreateWithString;

        fn url_value(text: &str) -> CFType {
            let source = CFString::new(text);
            let raw = unsafe {
                CFURLCreateWithString(
                    std::ptr::null(),
                    source.as_concrete_TypeRef(),
                    std::ptr::null(),
                )
            };
            unsafe { CFType::wrap_under_create_rule(raw as _) }
        }

        #[test]
        fn created_url_reads_as_its_absolute_string() {
            assert_eq!(
                url_text(url_value("https://example.com/a")),
                Some("https://example.com/a".into())
            );
        }

        #[test]
        fn plain_string_values_are_accepted() {
            let value = CFString::new("https://example.com/b").as_CFType();

            assert_eq!(url_text(value), Some("https://example.com/b".into()));
        }

        #[test]
        fn non_url_values_and_blank_urls_answer_none() {
            assert_eq!(url_text(CFNumber::from(7).as_CFType()), None);
            assert_eq!(url_text(CFString::new("   ").as_CFType()), None);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use crate::tree::AXElement;
    use agent_desktop_core::AdapterError;

    pub(crate) fn read(
        _element: &AXElement,
        _deadline: agent_desktop_core::Deadline,
    ) -> Result<Option<String>, AdapterError> {
        Err(AdapterError::not_supported("get_live_url"))
    }
}

pub(crate) use imp::read as read_url;
