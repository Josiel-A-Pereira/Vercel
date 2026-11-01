use super::imp::{
    is_array, is_mergeable, is_ranged_mergeable, is_windows_attribute, slice, token_bytes,
};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;

#[test]
fn token_carries_pid_magic_and_element_id() {
    let token = token_bytes(4242, 0x0102_0304_0506_0708);
    assert_eq!(&token[0..4], &4242_i32.to_ne_bytes());
    assert_eq!(&token[4..8], &[0, 0, 0, 0]);
    assert_eq!(&token[8..12], &0x636f_636f_i32.to_ne_bytes());
    assert_eq!(&token[12..20], &0x0102_0304_0506_0708_u64.to_ne_bytes());
}

#[test]
fn only_the_windows_attribute_is_extended() {
    let windows = CFString::new("AXWindows");
    let children = CFString::new("AXChildren");
    assert!(is_windows_attribute(windows.as_concrete_TypeRef()));
    assert!(!is_windows_attribute(children.as_concrete_TypeRef()));
    assert!(!is_windows_attribute(std::ptr::null()));
}

#[test]
fn application_errors_are_never_turned_into_success() {
    use accessibility_sys::{
        kAXErrorAttributeUnsupported, kAXErrorCannotComplete, kAXErrorIllegalArgument,
        kAXErrorInvalidUIElement, kAXErrorNoValue, kAXErrorSuccess,
    };
    assert!(is_mergeable(kAXErrorSuccess, false));
    assert!(is_mergeable(kAXErrorSuccess, true));
    assert!(is_mergeable(kAXErrorNoValue, true));
    assert!(is_mergeable(kAXErrorAttributeUnsupported, true));
    assert!(!is_mergeable(kAXErrorCannotComplete, true));
    assert!(!is_mergeable(kAXErrorInvalidUIElement, true));
    assert!(!is_mergeable(kAXErrorNoValue, false));
    assert!(is_ranged_mergeable(kAXErrorIllegalArgument, true));
    assert!(!is_ranged_mergeable(kAXErrorIllegalArgument, false));
    assert!(!is_mergeable(kAXErrorIllegalArgument, true));
}

#[test]
fn slice_clamps_to_the_merged_window_list() {
    use core_foundation::array::CFArray;
    use core_foundation::number::CFNumber;
    let numbers: Vec<CFNumber> = (0..5).map(CFNumber::from).collect();
    let full = CFArray::from_CFTypes(&numbers);
    let read = |index, max| {
        let sliced = slice(full.as_concrete_TypeRef(), index, max);
        let sliced = unsafe { CFArray::<CFNumber>::wrap_under_create_rule(sliced) };
        sliced
            .iter()
            .map(|n| n.to_i32().unwrap_or(-1))
            .collect::<Vec<_>>()
    };
    assert_eq!(read(1, 2), vec![1, 2]);
    assert_eq!(read(3, 10), vec![3, 4]);
    assert_eq!(read(9, 2), Vec::<i32>::new());
    assert_eq!(read(-1, 1), vec![0]);
}

#[test]
fn only_arrays_reach_the_array_api() {
    use core_foundation::array::CFArray;
    let text = CFString::new("AXWindows");
    let empty = CFArray::<CFString>::from_CFTypes(&[]);
    assert!(is_array(empty.as_CFTypeRef()));
    assert!(!is_array(text.as_CFTypeRef()));
    assert!(!is_array(std::ptr::null()));
}
