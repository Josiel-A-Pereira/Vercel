//! The home directory of the account the process runs as, read from the
//! system's account records rather than `HOME`, `USERPROFILE` or
//! `AGENT_DESKTOP_HOME`. Those are the caller's to set, and an agent that
//! could point them elsewhere would leave the owner's policies folder
//! behind.

use std::path::PathBuf;

#[cfg(unix)]
pub(crate) fn account_home() -> Option<PathBuf> {
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let mut buffer = vec![0_u8; 16 * 1024];
    let mut record: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let status = unsafe {
        libc::getpwuid_r(
            libc::geteuid(),
            &mut record,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if status != 0 || found.is_null() || record.pw_dir.is_null() {
        return None;
    }
    let dir = unsafe { CStr::from_ptr(record.pw_dir) };
    let home = PathBuf::from(OsStr::from_bytes(dir.to_bytes()));
    home.is_absolute().then_some(home)
}

#[cfg(windows)]
pub(crate) fn account_home() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Profile, SHGetKnownFolderPath};

    let mut path: windows_sys::core::PWSTR = std::ptr::null_mut();
    let status =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Profile, 0, std::ptr::null_mut(), &mut path) };
    if status != 0 || path.is_null() {
        if !path.is_null() {
            unsafe { CoTaskMemFree(path.cast()) };
        }
        return None;
    }
    let length = (0..)
        .take_while(|&index| unsafe { *path.add(index) } != 0)
        .count();
    let wide = unsafe { std::slice::from_raw_parts(path, length) }.to_vec();
    unsafe { CoTaskMemFree(path.cast()) };
    let home = PathBuf::from(std::ffi::OsString::from_wide(&wide));
    home.is_absolute().then_some(home)
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn account_home() -> Option<PathBuf> {
    None
}

#[cfg(test)]
#[path = "account_home_tests.rs"]
mod tests;
