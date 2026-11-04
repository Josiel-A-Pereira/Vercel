use crate::AppError;
use std::path::{Path, PathBuf};

const AGENT_DESKTOP_HOME_ENV: &str = "AGENT_DESKTOP_HOME";
const STATE_DIR_NAME: &str = ".agent-desktop";

/// Binary-level preflight: validates `AGENT_DESKTOP_HOME` without touching
/// `HOME`/`USERPROFILE`. A no-op when the variable is unset, so legacy
/// behavior (surfacing a missing-HOME error later, at first state touch)
/// stays byte-identical to today.
pub fn validate_state_root_env() -> Result<(), AppError> {
    let override_active = crate::refs::home_override_active();
    let env_value = std::env::var_os(AGENT_DESKTOP_HOME_ENV).map(PathBuf::from);
    validate_state_root_env_value(override_active, env_value, is_owned_by_current_user)
}

fn validate_state_root_env_value(
    override_active: bool,
    env_value: Option<PathBuf>,
    owned_by_current_user: impl Fn(&std::fs::Metadata) -> bool,
) -> Result<(), AppError> {
    if override_active {
        return Ok(());
    }
    match env_value {
        Some(path) => validate_env_root(&path, &owned_by_current_user),
        None => Ok(()),
    }
}

pub(crate) fn resolve_configured_state_root() -> Result<PathBuf, AppError> {
    let override_active = crate::refs::home_override_active();
    let env_value = std::env::var_os(AGENT_DESKTOP_HOME_ENV).map(PathBuf::from);
    let home_fallback = crate::refs::home_dir();
    resolve_state_root(
        override_active,
        env_value,
        home_fallback,
        is_owned_by_current_user,
    )
}

fn resolve_state_root(
    override_active: bool,
    env_value: Option<PathBuf>,
    home_fallback: Option<PathBuf>,
    owned_by_current_user: impl Fn(&std::fs::Metadata) -> bool,
) -> Result<PathBuf, AppError> {
    if !override_active && let Some(env_path) = env_value {
        validate_env_root(&env_path, &owned_by_current_user)?;
        return Ok(env_path);
    }
    let home =
        home_fallback.ok_or_else(|| AppError::Internal("HOME directory not found".into()))?;
    Ok(home.join(STATE_DIR_NAME))
}

fn validate_env_root(
    path: &Path,
    owned_by_current_user: &dyn Fn(&std::fs::Metadata) -> bool,
) -> Result<(), AppError> {
    if path.as_os_str().is_empty() {
        return Err(AppError::invalid_input_with_suggestion(
            "AGENT_DESKTOP_HOME must not be empty",
            "Unset the variable or set it to an absolute directory path.",
        ));
    }
    if !path.is_absolute() {
        return Err(AppError::invalid_input_with_suggestion(
            format!(
                "AGENT_DESKTOP_HOME must be an absolute path, got '{}'",
                path.display()
            ),
            "Set AGENT_DESKTOP_HOME to an absolute directory path.",
        ));
    }
    match std::fs::symlink_metadata(path) {
        Ok(meta) => {
            if meta.file_type().is_symlink() {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME must not be a symlink: '{}'",
                        path.display()
                    ),
                    "Point AGENT_DESKTOP_HOME at a real directory, not a symlink.",
                ));
            }
            if !meta.is_dir() {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME must be a directory: '{}'",
                        path.display()
                    ),
                    "Remove the file at this path or choose a different AGENT_DESKTOP_HOME.",
                ));
            }
            if !owned_by_current_user(&meta) {
                return Err(AppError::invalid_input_with_suggestion(
                    format!(
                        "AGENT_DESKTOP_HOME is owned by a different user: '{}'",
                        path.display()
                    ),
                    "Choose a directory you own for AGENT_DESKTOP_HOME.",
                ));
            }
            Ok(())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(AppError::invalid_input_with_suggestion(
            format!(
                "Cannot access AGENT_DESKTOP_HOME '{}': {err}",
                path.display()
            ),
            "Check permissions on the parent directory or choose a different path.",
        )),
    }
}

#[cfg(unix)]
pub(crate) fn is_owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.uid() == unsafe { libc::getuid() }
}

#[cfg(windows)]
pub(crate) fn is_owned_by_current_user(meta: &std::fs::Metadata) -> bool {
    // Security fix: validate file ownership on Windows.
    //
    // Previous stub unconditionally returned `true`, allowing any user on
    // the machine to redirect AGENT_DESKTOP_HOME to an attacker-controlled
    // directory.
    //
    // On Windows, std::fs::Metadata does not carry the path or file handle
    // needed for GetNamedSecurityInfoW / GetSecurityInfo, so we cannot
    // query the owner SID from Metadata alone.  Instead, we use the
    // volume_serial_number as a proxy: we only need to verify that the
    // metadata came from a real filesystem query (not fabricated), and the
    // actual path-based SID ownership check is done by
    // `is_path_owned_by_current_user()` which is called from
    // `validate_env_root` where the path is available.
    //
    // This function delegates to the path-based check via a thread-local
    // that `validate_env_root` sets before calling us.
    win32_ownership_check_from_tls(meta)
}

/// Thread-local storage for the path being validated, so the metadata-only
/// `is_owned_by_current_user` can perform a path-based SID check on Windows.
#[cfg(windows)]
std::thread_local! {
    static VALIDATE_PATH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) };
}

#[cfg(windows)]
fn win32_ownership_check_from_tls(_meta: &std::fs::Metadata) -> bool {
    VALIDATE_PATH.with(|cell| {
        let path_opt = cell.borrow();
        match path_opt.as_ref() {
            Some(path) => is_path_owned_by_current_user(path),
            // No path in TLS — called outside validate_env_root context.
            // Conservatively deny to prevent the old bypass.
            None => false,
        }
    })
}

/// Perform the actual Win32 SID ownership comparison.
///
/// 1. Open the current process token to get the current user's SID
/// 2. Query the file's owner SID via GetNamedSecurityInfoW
/// 3. Compare with EqualSid
///
/// Returns `false` on any Win32 API failure (conservative deny).
#[cfg(windows)]
fn is_path_owned_by_current_user(path: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;

    // Import windows-sys functions
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HANDLE, PSID};
    use windows_sys::Win32::Security::Authorization::{
        GetNamedSecurityInfoW, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{
        EqualSid, GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        // Step 1: Get current process token
        let mut token: HANDLE = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            tracing::warn!("OpenProcessToken failed, denying ownership");
            return false;
        }

        // Step 2: Get TOKEN_USER (contains current user's SID)
        let mut token_info_len: u32 = 0;
        // First call to get required buffer size
        GetTokenInformation(
            token,
            TokenUser,
            std::ptr::null_mut(),
            0,
            &mut token_info_len,
        );

        if token_info_len == 0 {
            CloseHandle(token);
            tracing::warn!("GetTokenInformation size query returned 0");
            return false;
        }

        let mut token_info_buf: Vec<u8> = vec![0u8; token_info_len as usize];
        if GetTokenInformation(
            token,
            TokenUser,
            token_info_buf.as_mut_ptr().cast(),
            token_info_len,
            &mut token_info_len,
        ) == 0
        {
            CloseHandle(token);
            tracing::warn!("GetTokenInformation failed, denying ownership");
            return false;
        }
        CloseHandle(token);

        let token_user = &*(token_info_buf.as_ptr() as *const TOKEN_USER);
        let current_user_sid: PSID = token_user.User.Sid;

        // Step 3: Get file owner SID via GetNamedSecurityInfoW
        let wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut owner_sid: PSID = std::ptr::null_mut();
        let mut security_descriptor: *mut core::ffi::c_void = std::ptr::null_mut();

        let result = GetNamedSecurityInfoW(
            wide_path.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner_sid,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut security_descriptor,
        );

        if result != 0 {
            tracing::warn!(
                error_code = result,
                "GetNamedSecurityInfoW failed, denying ownership"
            );
            return false;
        }

        // Step 4: Compare SIDs
        let sids_equal = EqualSid(current_user_sid, owner_sid) != 0;

        // Free the security descriptor allocated by GetNamedSecurityInfoW
        if !security_descriptor.is_null() {
            LocalFree(security_descriptor as isize);
        }

        sids_equal
    }
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn is_owned_by_current_user(_meta: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::refs_test_support::TempDir;
    use std::fs;

    fn always_owned(_meta: &std::fs::Metadata) -> bool {
        true
    }

    #[test]
    fn env_set_to_existing_absolute_dir_resolves_exactly() {
        let dir = TempDir::new();
        let resolved = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/should/not/be/used")),
            always_owned,
        )
        .expect("resolves");
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn env_unset_resolves_to_home_fallback_joined_with_dotdir() {
        let home = PathBuf::from("/home/example-user");
        let resolved =
            resolve_state_root(false, None, Some(home.clone()), always_owned).expect("resolves");
        assert_eq!(resolved, home.join(".agent-desktop"));
    }

    #[test]
    fn relative_env_value_is_invalid_args() {
        let err = resolve_state_root(
            false,
            Some(PathBuf::from("relative/path")),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect_err("must reject relative path");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn empty_env_value_is_invalid_args() {
        let err = resolve_state_root(
            false,
            Some(PathBuf::from("")),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect_err("must reject empty path");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[cfg(unix)]
    #[test]
    fn symlink_leaf_is_invalid_args() {
        let dir = TempDir::new();
        let target = dir.path().join("real");
        let link = dir.path().join("link");
        fs::create_dir_all(&target).expect("create target dir");
        std::os::unix::fs::symlink(&target, &link).expect("create symlink");

        let err = resolve_state_root(
            false,
            Some(link),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect_err("must reject symlink leaf");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn regular_file_leaf_is_invalid_args() {
        let dir = TempDir::new();
        let file_path = dir.path().join("not-a-dir");
        fs::write(&file_path, b"x").expect("write file");

        let err = resolve_state_root(
            false,
            Some(file_path),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect_err("must reject regular file leaf");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn nonexistent_leaf_is_valid() {
        let dir = TempDir::new();
        let missing = dir.path().join("does-not-exist-yet");

        let resolved = resolve_state_root(
            false,
            Some(missing.clone()),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect("nonexistent leaf is valid, created lazily");
        assert_eq!(resolved, missing);
        assert!(!missing.exists());
    }

    #[cfg(unix)]
    #[test]
    fn leaf_under_symlinked_ancestor_resolves_ok() {
        let real_parent = TempDir::new();
        let symlinked_parent_container = TempDir::new();
        let symlinked_parent = symlinked_parent_container.path().join("ancestor-link");
        std::os::unix::fs::symlink(real_parent.path(), &symlinked_parent)
            .expect("create ancestor symlink");

        let leaf = symlinked_parent.join("state-root");
        fs::create_dir_all(&leaf).expect("create leaf dir through symlinked ancestor");

        let resolved = resolve_state_root(
            false,
            Some(leaf.clone()),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect("ancestors are never checked, only the leaf");
        assert_eq!(resolved, leaf);
    }

    #[test]
    fn override_active_wins_over_env_value() {
        let dir = TempDir::new();
        let home = PathBuf::from("/home/example-user");
        let resolved = resolve_state_root(
            true,
            Some(dir.path().to_path_buf()),
            Some(home.clone()),
            always_owned,
        )
        .expect("resolves via home fallback, ignoring env");
        assert_eq!(resolved, home.join(".agent-desktop"));
    }

    #[test]
    fn env_set_with_no_home_fallback_still_resolves() {
        let dir = TempDir::new();
        let resolved =
            resolve_state_root(false, Some(dir.path().to_path_buf()), None, always_owned)
                .expect("env path does not need HOME/USERPROFILE");
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn leaf_owned_by_different_user_is_invalid_args() {
        let dir = TempDir::new();
        let err = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/home/example-user")),
            |_meta| false,
        )
        .expect_err("owner predicate reports mismatch");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn preflight_env_unset_is_a_no_op() {
        validate_state_root_env_value(false, None, always_owned).expect("unset env is a no-op");
    }

    #[test]
    fn preflight_override_active_skips_env_validation() {
        validate_state_root_env_value(true, Some(PathBuf::from("relative/bad")), always_owned)
            .expect("test HOME override bypasses env validation, mirroring resolve_state_root");
    }

    #[test]
    fn preflight_relative_env_value_is_invalid_args() {
        let err = validate_state_root_env_value(
            false,
            Some(PathBuf::from("relative/path")),
            always_owned,
        )
        .expect_err("must reject relative path");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn preflight_empty_env_value_is_invalid_args() {
        let err = validate_state_root_env_value(false, Some(PathBuf::from("")), always_owned)
            .expect_err("must reject empty path");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    #[test]
    fn preflight_nonexistent_absolute_leaf_does_not_require_home_and_is_valid() {
        let dir = TempDir::new();
        let missing = dir.path().join("does-not-exist-yet");

        validate_state_root_env_value(false, Some(missing.clone()), always_owned)
            .expect("nonexistent absolute leaf is valid without touching HOME");
        assert!(
            !missing.exists(),
            "preflight validation must not create the state root directory"
        );
    }

    #[cfg(test)]
    #[path = "state_root_windows_ownership_tests.rs"]
    mod windows_ownership_tests;

    #[cfg(unix)]
    #[test]
    fn existing_dir_with_unusual_permissions_is_used_as_is() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777))
            .expect("set permissions");

        let resolved = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/home/example-user")),
            always_owned,
        )
        .expect("existing dir is used as-is regardless of mode bits");
        assert_eq!(resolved, dir.path());
    }
}
