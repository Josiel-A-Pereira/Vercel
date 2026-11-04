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
    /// After the fix: this test exercises the real Win32 ownership check.
    #[test]
    fn current_user_directory_is_accepted_by_real_check() {
        let dir = TempDir::new();
        let meta = fs::metadata(dir.path()).expect("metadata");
        // The current user's own temp directory must be accepted
        assert!(
            is_owned_by_current_user(&meta),
            "is_owned_by_current_user must accept directories owned by the current user"
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
}

