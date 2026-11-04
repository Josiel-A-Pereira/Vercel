//! `AXWindows` on an application lists only the windows on the current Space.
//! A window on another Space or in full screen is still a live accessibility
//! element; it is only missing from that list. The private
//! `_AXUIElementCreateWithRemoteToken` call rebuilds an element from its
//! (pid, element id) token, so probing a small id range recovers those
//! windows without switching Spaces, taking focus or moving the pointer.
//! The same approach is what window switchers such as AltTab rely on.

#[cfg(target_os = "macos")]
mod imp {
    use crate::tree::AXElement;
    use crate::tree::properties::ax_ipc::{self, AxDeadline};
    use crate::tree::properties::offspace_probe::{TOKEN_ID_RANGE, probe, timeout_until};
    use accessibility_sys::{
        AXUIElementCopyAttributeValue, AXUIElementGetPid, AXUIElementRef,
        kAXErrorAttributeUnsupported, kAXErrorIllegalArgument, kAXErrorInvalidUIElement,
        kAXErrorNoValue, kAXErrorSuccess,
    };
    use core_foundation::base::TCFType;
    use core_foundation::string::CFString;
    use core_foundation_sys::array::{
        CFArrayCreate, CFArrayGetCount, CFArrayGetTypeID, CFArrayGetValueAtIndex, CFArrayRef,
        kCFTypeArrayCallBacks,
    };
    use core_foundation_sys::base::CFIndex;
    use core_foundation_sys::base::{
        CFEqual, CFGetTypeID, CFRelease, CFRetain, CFTypeRef, kCFAllocatorDefault,
    };
    use core_foundation_sys::string::{CFStringGetLength, CFStringRef};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// Upper bound on one probe, so an application that answers slowly or
    /// not at all costs at most this much. A probe also takes at most half of
    /// the remaining deadline, so the native read it extends still completes.
    const PROBE_BUDGET: Duration = Duration::from_millis(750);
    const CACHE_TTL: Duration = Duration::from_secs(3);

    /// `next` is the first id not probed yet, or `None` once the whole range
    /// is covered.
    struct Entry {
        pid: i32,
        at: Instant,
        windows: Vec<usize>,
        next: Option<u64>,
    }

    static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

    /// Hot path: every attribute read passes through, so the length test
    /// rejects nearly all of them before any string is compared.
    pub(crate) fn is_windows_attribute(attribute: CFStringRef) -> bool {
        if attribute.is_null() || unsafe { CFStringGetLength(attribute) } != 9 {
            return false;
        }
        let name = unsafe { CFString::wrap_under_get_rule(attribute) };
        name == "AXWindows"
    }

    /// Only an absent or empty window list is extended. An error such as
    /// `kAXErrorCannotComplete` or `kAXErrorInvalidUIElement` describes the
    /// application, so it reaches the caller unchanged.
    pub(super) fn is_mergeable(error: i32, value_is_null: bool) -> bool {
        error == kAXErrorSuccess
            || (value_is_null
                && (error == kAXErrorNoValue || error == kAXErrorAttributeUnsupported))
    }

    /// A ranged read of an empty window list fails with
    /// `kAXErrorIllegalArgument`, so that error is extended too.
    pub(super) fn is_ranged_mergeable(error: i32, values_is_null: bool) -> bool {
        is_mergeable(error, values_is_null) || (values_is_null && error == kAXErrorIllegalArgument)
    }

    pub(super) fn is_array(value: CFTypeRef) -> bool {
        !value.is_null() && unsafe { CFGetTypeID(value) == CFArrayGetTypeID() }
    }

    /// False only when the element is gone, so a window closed since it was
    /// cached is never handed out. A slow or failed read keeps it.
    fn is_alive(element: CFTypeRef, until: Instant) -> bool {
        let element = element as AXUIElementRef;
        if !timeout_until(element, until) {
            return true;
        }
        let name = CFString::new("AXRole");
        let mut value: CFTypeRef = std::ptr::null();
        let error = unsafe {
            AXUIElementCopyAttributeValue(element, name.as_concrete_TypeRef(), &mut value)
        };
        if !value.is_null() {
            unsafe { CFRelease(value) };
        }
        error != kAXErrorInvalidUIElement
    }

    fn forget(pid: i32) {
        let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
        if let Some(index) = cache.iter().position(|entry| entry.pid == pid) {
            release_all(&cache.swap_remove(index).windows);
        }
    }

    fn release_all(windows: &[usize]) {
        for &window in windows {
            unsafe { CFRelease(window as CFTypeRef) };
        }
    }

    fn retain_all(windows: &[usize]) {
        for &window in windows {
            unsafe { CFRetain(window as CFTypeRef) };
        }
    }

    fn cached(pid: i32) -> Option<(Vec<usize>, Option<u64>, Instant)> {
        let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
        cache.retain(|entry| {
            let fresh = entry.at.elapsed() < CACHE_TTL;
            if !fresh {
                release_all(&entry.windows);
            }
            fresh
        });
        let entry = cache.iter().find(|entry| entry.pid == pid)?;
        retain_all(&entry.windows);
        Some((entry.windows.clone(), entry.next, entry.at))
    }

    /// Whether a probe that reached `reach` replaces the saved entry
    /// `current` (its start and reach). A probe resumed from the entry that
    /// started at `base` saves only into that same entry, so it never brings
    /// back an expired or evicted entry or overwrites a newer one.
    pub(super) fn publishes(
        base: Option<Instant>,
        current: Option<(Instant, u64)>,
        reach: u64,
    ) -> bool {
        match (base, current) {
            (_, None) => base.is_none(),
            (Some(base), Some((at, saved))) => base == at && reach > saved,
            (None, Some((_, saved))) => reach > saved,
        }
    }

    /// Retained window elements for `pid`, owned by the caller. The probe runs
    /// without the cache lock, so a slow application never delays a read of
    /// another one. A probe that runs out of time saves its position, so the
    /// next read continues from there instead of starting again.
    fn windows_for_pid(pid: i32, deadline: Instant) -> Vec<usize> {
        let now = Instant::now();
        let (mut windows, from, base) = match cached(pid) {
            Some((windows, None, _)) => return windows,
            Some((windows, Some(next), at)) => (windows, next, Some(at)),
            None => (Vec::new(), 0, None),
        };
        let share = deadline.saturating_duration_since(now) / 2;
        let until = now + share.min(PROBE_BUDGET);
        let (found, next) = probe(pid, from, until);
        windows.extend(found);
        let reach = |next: Option<u64>| next.unwrap_or(TOKEN_ID_RANGE);
        let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
        let index = cache.iter().position(|entry| entry.pid == pid);
        let current = index.map(|index| (cache[index].at, reach(cache[index].next)));
        if !publishes(base, current, reach(next)) {
            return windows;
        }
        if let Some(index) = index {
            release_all(&cache.swap_remove(index).windows);
        }
        retain_all(&windows);
        cache.push(Entry {
            pid,
            at: base.unwrap_or(now),
            windows: windows.clone(),
            next,
        });
        windows
    }

    /// Returns the `AXWindows` array of `app` with windows from other Spaces
    /// appended, as a new array owned by the caller, or `None` when there is
    /// nothing to add. `original` keeps its ownership.
    fn merged(app: AXUIElementRef, original: CFArrayRef, deadline: Instant) -> Option<CFArrayRef> {
        if !original.is_null() && !is_array(original as CFTypeRef) {
            return None;
        }
        let mut pid = 0_i32;
        if unsafe { AXUIElementGetPid(app, &mut pid) } != kAXErrorSuccess || pid <= 0 {
            return None;
        }
        let extras = windows_for_pid(pid, deadline);
        let mut items: Vec<CFTypeRef> = Vec::new();
        if !original.is_null() {
            let count = unsafe { CFArrayGetCount(original) };
            for index in 0..count {
                items.push(unsafe { CFArrayGetValueAtIndex(original, index) });
            }
        }
        let known = items.len();
        let mut stale = false;
        for &extra in &extras {
            let extra = extra as CFTypeRef;
            if items
                .iter()
                .any(|&item| unsafe { CFEqual(item, extra) } != 0)
            {
                continue;
            }
            if is_alive(extra, deadline) {
                items.push(extra);
            } else {
                stale = true;
            }
        }
        if stale {
            forget(pid);
        }
        let added = items.len() > known;
        let result = added.then(|| unsafe {
            CFArrayCreate(
                kCFAllocatorDefault,
                items.as_ptr(),
                items.len() as isize,
                &kCFTypeArrayCallBacks,
            )
        });
        for extra in extras {
            unsafe { CFRelease(extra as CFTypeRef) };
        }
        result
    }

    /// `AXWindows` read through [`ax_ipc::copy_attribute_value`], extended with
    /// windows from other Spaces. Every other attribute passes through.
    pub(crate) fn extend_value(
        element: &AXElement,
        attribute: CFStringRef,
        (error, value): (i32, CFTypeRef),
        deadline: impl AxDeadline,
    ) -> (i32, CFTypeRef) {
        if !is_windows_attribute(attribute) || !is_mergeable(error, value.is_null()) {
            return (error, value);
        }
        let Some(merged) = deadline
            .absolute()
            .ok()
            .and_then(|until| merged(element.0, value as CFArrayRef, until))
        else {
            return (error, value);
        };
        if !value.is_null() {
            unsafe { CFRelease(value) };
        }
        (kAXErrorSuccess, merged as CFTypeRef)
    }

    /// The ranged `AXWindows` read, served from the extended list.
    pub(crate) fn extend_values(
        element: &AXElement,
        attribute: CFStringRef,
        (error, values): (i32, CFArrayRef),
        (index, max_values): (CFIndex, CFIndex),
        deadline: impl AxDeadline,
    ) -> (i32, CFArrayRef) {
        if !is_windows_attribute(attribute) || !is_ranged_mergeable(error, values.is_null()) {
            return (error, values);
        }
        let (full_error, full) = ax_ipc::copy_attribute_value(element, attribute, deadline);
        if full_error != kAXErrorSuccess || !is_array(full) {
            if !full.is_null() {
                unsafe { CFRelease(full) };
            }
            return (error, values);
        }
        if !values.is_null() {
            unsafe { CFRelease(values as CFTypeRef) };
        }
        let sliced = slice(full as CFArrayRef, index, max_values);
        unsafe { CFRelease(full) };
        (kAXErrorSuccess, sliced)
    }

    /// The `AXWindows` count, matching the extended list.
    pub(crate) fn extend_count(
        element: &AXElement,
        attribute: CFStringRef,
        (error, count): (i32, CFIndex),
        deadline: impl AxDeadline,
    ) -> Option<usize> {
        if !is_windows_attribute(attribute) || !is_mergeable(error, count == 0) {
            return None;
        }
        let (full_error, full) = ax_ipc::copy_attribute_value(element, attribute, deadline);
        if !is_array(full) {
            if !full.is_null() {
                unsafe { CFRelease(full) };
            }
            return None;
        }
        let merged = unsafe { CFArrayGetCount(full as CFArrayRef) };
        unsafe { CFRelease(full) };
        (full_error == kAXErrorSuccess)
            .then(|| usize::try_from(merged).ok())
            .flatten()
    }

    /// A new array holding `full[index..index + max_values]`, clamped.
    pub(crate) fn slice(full: CFArrayRef, index: CFIndex, max_values: CFIndex) -> CFArrayRef {
        let count = unsafe { CFArrayGetCount(full) };
        let start = index.clamp(0, count);
        let end = start.saturating_add(max_values.max(0)).min(count);
        let items: Vec<CFTypeRef> = (start..end)
            .map(|i| unsafe { CFArrayGetValueAtIndex(full, i) })
            .collect();
        unsafe {
            CFArrayCreate(
                kCFAllocatorDefault,
                items.as_ptr(),
                items.len() as CFIndex,
                &kCFTypeArrayCallBacks,
            )
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) use imp::{extend_count, extend_value, extend_values};

#[cfg(all(test, target_os = "macos"))]
#[path = "offspace_windows_tests.rs"]
mod tests;
