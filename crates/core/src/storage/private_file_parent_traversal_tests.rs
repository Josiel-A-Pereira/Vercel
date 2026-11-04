/// Regression tests for CVE: Windows private file parent path traversal.
///
/// The `ensure_directory_path()` function on non-Unix (Windows) platforms
/// must reject parent-directory traversal (`..` components) and symlinks/
/// junction points, matching the security guarantees of the Unix implementation.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// Helper to call the platform's ensure_directory_path via ensure_private_portable.
    /// We test through the public-facing ensure_private_portable since
    /// ensure_directory_path is a private function.
    fn test_ensure_private(path: &Path) -> std::io::Result<()> {
        super::ensure_private_portable(path)
    }

    /// Parent-directory traversal (`..`) must be rejected on all platforms.
    ///
    /// Before the fix: on Windows, `ensure_directory_path` simply called
    /// `create_dir_all`, which happily follows `..` components, enabling
    /// path traversal attacks.
    ///
    /// After the fix: `..` components are explicitly rejected with an
    /// InvalidData error, matching the Unix behavior.
    #[test]
    fn rejects_parent_directory_traversal() {
        let base = std::env::temp_dir().join("ad-traversal-test");
        let _ = std::fs::create_dir_all(&base);
        let traversal_path = base.join("legit").join("..").join("..").join("escaped");

        let result = test_ensure_private(&traversal_path);
        assert!(
            result.is_err(),
            "ensure_private must reject paths containing '..' on all platforms"
        );
        if let Err(ref e) = result {
            assert_eq!(
                e.kind(),
                std::io::ErrorKind::InvalidData,
                "error kind must be InvalidData for path traversal rejection"
            );
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Symlinks/junction points in the path must be rejected on all platforms.
    ///
    /// On Unix: symlinks owned by non-root users are rejected.
    /// On Windows: NTFS junctions and symlinks must also be rejected.
    #[cfg(windows)]
    #[test]
    fn rejects_junction_points_in_path() {
        use std::process::Command;

        let base = std::env::temp_dir().join("ad-junction-test");
        let real_dir = base.join("real");
        let junction = base.join("junction");
        let target = base.join("target");

        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&real_dir).expect("create real dir");
        std::fs::create_dir_all(&target).expect("create target dir");

        // Create an NTFS junction point
        let output = Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output();

        if let Ok(output) = output {
            if output.status.success() {
                let test_path = junction.join("private-dir");
                let result = test_ensure_private(&test_path);
                assert!(
                    result.is_err(),
                    "ensure_private must reject paths through junction points"
                );
            }
        }

        // Cleanup
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Non-directory entries in the path must be rejected.
    #[test]
    fn rejects_file_in_path() {
        let base = std::env::temp_dir().join("ad-file-in-path-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create base");

        let file_path = base.join("not-a-dir");
        std::fs::write(&file_path, b"x").expect("create file");

        let test_path = file_path.join("child");
        let result = test_ensure_private(&test_path);
        // This should fail because "not-a-dir" is a file, not a directory
        assert!(
            result.is_err(),
            "ensure_private must reject paths where a component is a file"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A clean directory path with no traversal or symlinks must succeed.
    #[test]
    fn accepts_clean_directory_path() {
        let base = std::env::temp_dir().join("ad-clean-path-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create base");

        let result = test_ensure_private(&base);
        // Note: this may fail if the temp dir has group/other permissions on Unix,
        // which is expected. The test is most meaningful on Windows.
        #[cfg(windows)]
        assert!(
            result.is_ok(),
            "ensure_private must accept clean directory paths"
        );

        // Cleanup
        let _ = std::fs::remove_dir_all(&base);
    }
}

