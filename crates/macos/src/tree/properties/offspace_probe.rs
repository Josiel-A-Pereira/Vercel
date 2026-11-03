//! The remote-token probe behind [`super::offspace_windows`]: it rebuilds
//! the elements of one process from (pid, element id) tokens and keeps the
//! ones that are windows.

#[cfg(target_os = "macos")]
mod imp {
    use accessibility_sys::{
        AXUIElementCopyAttributeValue, AXUIElementRef, AXUIElementSetMessagingTimeout,
        kAXErrorSuccess,
    };
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::base::{CFRelease, CFTypeRef, kCFAllocatorDefault};
    use core_foundation_sys::data::{CFDataCreate, CFDataRef};
    use std::time::{Duration, Instant};

    unsafe extern "C" {
        fn _AXUIElementCreateWithRemoteToken(data: CFDataRef) -> AXUIElementRef;
    }

    /// Element ids are small sequential integers per process; windows sit low
    /// in the range because they are created early. AltTab probes the same
    /// range.
    pub(crate) const TOKEN_ID_RANGE: u64 = 1000;
    const PROBE_TIMEOUT: Duration = Duration::from_millis(50);
    const WINDOW_SUBROLES: &[&str] = &[
        "AXStandardWindow",
        "AXDialog",
        "AXFloatingWindow",
        "AXSystemDialog",
    ];

    fn string_attr(element: AXUIElementRef, attribute: &str) -> Option<String> {
        let name = CFString::new(attribute);
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &mut value)
        };
        if error != kAXErrorSuccess || value.is_null() {
            return None;
        }
        let value = unsafe { core_foundation::base::CFType::wrap_under_create_rule(value) };
        value.downcast::<CFString>().map(|s| s.to_string())
    }

    /// The remote token layout: pid, 4 zero bytes, the `coco` magic, then the
    /// 64-bit element id.
    pub(crate) fn token_bytes(pid: i32, id: u64) -> [u8; 20] {
        let mut token = [0_u8; 20];
        token[0..4].copy_from_slice(&pid.to_ne_bytes());
        token[8..12].copy_from_slice(&0x636f_636f_i32.to_ne_bytes());
        token[12..20].copy_from_slice(&id.to_ne_bytes());
        token
    }

    pub(crate) fn timeout_until(element: AXUIElementRef, until: Instant) -> bool {
        let remaining = until.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        let timeout = remaining.min(PROBE_TIMEOUT).as_secs_f32();
        unsafe { AXUIElementSetMessagingTimeout(element, timeout) };
        true
    }

    /// Window elements found for `pid` from id `from`, each retained, and the
    /// first id not probed before `until`.
    pub(crate) fn probe(pid: i32, from: u64, until: Instant) -> (Vec<usize>, Option<u64>) {
        let mut found = Vec::new();
        for id in from..TOKEN_ID_RANGE {
            if Instant::now() >= until {
                return (found, Some(id));
            }
            let token = token_bytes(pid, id);
            let data = unsafe { CFDataCreate(kCFAllocatorDefault, token.as_ptr(), 20) };
            if data.is_null() {
                continue;
            }
            let element = unsafe { _AXUIElementCreateWithRemoteToken(data) };
            unsafe { CFRelease(data as CFTypeRef) };
            if element.is_null() {
                continue;
            }
            match classify(element, until) {
                Some(true) => found.push(element as usize),
                verdict => {
                    unsafe { CFRelease(element as CFTypeRef) };
                    if verdict.is_none() {
                        return (found, Some(id));
                    }
                }
            }
        }
        (found, None)
    }

    /// Whether `element` is a window, or `None` when time ran out before the
    /// answer, so the id is probed again on the next read.
    fn classify(element: AXUIElementRef, until: Instant) -> Option<bool> {
        let read = |attribute| {
            let value = timeout_until(element, until)
                .then(|| string_attr(element, attribute))
                .flatten();
            (value.is_some() || Instant::now() < until).then_some(value)
        };
        if read("AXRole")?.as_deref() != Some("AXWindow") {
            return Some(false);
        }
        Some(read("AXSubrole")?.is_some_and(|subrole| WINDOW_SUBROLES.contains(&subrole.as_str())))
    }
}

#[cfg(all(test, target_os = "macos"))]
pub(crate) use imp::token_bytes;
#[cfg(target_os = "macos")]
pub(crate) use imp::{TOKEN_ID_RANGE, probe, timeout_until};
