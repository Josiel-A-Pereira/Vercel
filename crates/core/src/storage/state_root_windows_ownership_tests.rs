/// Regression tests for CVE: Windows state root ownership validation bypass.
///
/// The `is_owned_by_current_user()` function must not unconditionally return
/// `true` on Windows. These tests pin the expectation that foreign-owned
/// directories are rejected on every platform.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::refs::refs_test_support::TempDir;
    use std::fs;
    use std::path::PathBuf;

    /// The core invariant: `is_owned_by_current_user` must reject directories
    /// that the function receives when the `owned_by_current_user` predicate
    /// returns `false`.
    ///
    /// Before the fix, the `#[cfg(not(unix))]` stub always returned `true`,
    /// meaning any directory — including one created by an attacker — would
    /// be accepted as the state root on Windows.
    #[test]
    fn foreign_owned_directory_is_rejected_by_state_root_validation() {
        let dir = TempDir::new();
        let err = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/home/example-user")),
            |_meta| false, // Simulates foreign ownership
        )
        .expect_err("must reject directory owned by a different user on all platforms");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    /// Validates that the env-level preflight also rejects foreign-owned roots.
    #[test]
    fn preflight_rejects_foreign_owned_env_root() {
        let dir = TempDir::new();
        let err = validate_state_root_env_value(
            false,
            Some(dir.path().to_path_buf()),
            |_meta| false, // Foreign ownership
        )
        .expect_err("preflight must reject foreign-owned AGENT_DESKTOP_HOME");
        assert_eq!(err.code(), "INVALID_ARGS");
    }

    /// On Windows, the real `is_owned_by_current_user` function must NOT
    /// unconditionally return `true`. This test exercises the actual
    /// platform function against the current user's temp directory.
    ///
    /// Before the fix: this test would trivially pass because the stub
    /// returned `true` for everything.
    /// After the fix: this test exercises the real Win32 SID ownership check.
    #[cfg(windows)]
    #[test]
    fn current_user_directory_is_accepted_by_real_win32_check() {
        let dir = TempDir::new();
        // The path-based check needs the path in TLS
        VALIDATE_PATH.with(|cell| {
            *cell.borrow_mut() = Some(dir.path().to_path_buf());
        });
        let meta = fs::metadata(dir.path()).expect("metadata");
        let owned = is_owned_by_current_user(&meta);
        // Clear TLS
        VALIDATE_PATH.with(|cell| {
            *cell.borrow_mut() = None;
        });
        assert!(
            owned,
            "is_owned_by_current_user must accept directories owned by the current user \
             (Win32 SID check via GetNamedSecurityInfoW + EqualSid)"
        );
    }

    /// Regression pin: the `is_owned_by_current_user` function must not
    /// be a no-op on any platform. This test verifies the function's
    /// discriminating power by ensuring it can return `false` for at
    /// least the test-injected predicate path.
    #[test]
    fn ownership_check_has_discriminating_power() {
        let dir = TempDir::new();

        // With a predicate that always says "not owned", the validation must fail
        let result_foreign = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/fallback")),
            |_meta| false,
        );
        assert!(result_foreign.is_err(), "foreign ownership must be rejected");

        // With a predicate that says "owned", the same path must succeed
        let result_owned = resolve_state_root(
            false,
            Some(dir.path().to_path_buf()),
            Some(PathBuf::from("/fallback")),
            |_meta| true,
        );
        assert!(result_owned.is_ok(), "own ownership must be accepted");
    }

    /// On Windows, verify the path-based SID check works end-to-end
    /// via the validate_env_root flow (which sets VALIDATE_PATH TLS).
    #[cfg(windows)]
    #[test]
    fn win32_sid_check_accepts_own_temp_directory() {
        let dir = TempDir::new();
        let result = is_path_owned_by_current_user(dir.path());
        assert!(
            result,
            "is_path_owned_by_current_user must accept the current user's temp directory"
        );
    }

    /// Verify that without TLS path set, the check conservatively denies.
    #[cfg(windows)]
    #[test]
    fn win32_check_denies_without_tls_path() {
        VALIDATE_PATH.with(|cell| {
            *cell.borrow_mut() = None;
        });
        let dir = TempDir::new();
        let meta = fs::metadata(dir.path()).expect("metadata");
        let result = is_owned_by_current_user(&meta);
        assert!(
            !result,
            "is_owned_by_current_user must deny when no TLS path is set (conservative default)"
        );
    }
}
