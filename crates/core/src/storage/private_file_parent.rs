use std::path::Path;

pub(crate) fn ensure_private(path: &Path) -> std::io::Result<()> {
    crate::storage::private_file_ops::with_active_ops(|ops| ops.ensure_private(path))
}

pub(crate) fn ensure_private_portable(path: &Path) -> std::io::Result<()> {
    ensure_directory_path(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(super::private_file::permission_denied(
            "private file parent must be a real directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != unsafe { libc::geteuid() } {
            return Err(super::private_file::permission_denied(
                "private file parent is not owned by the effective user",
            ));
        }
        if metadata.mode() & 0o077 != 0 {
            return Err(super::private_file::permission_denied(
                "private file parent is accessible by group or other users",
            ));
        }
    }
    Ok(())
}

pub(crate) fn ensure_user(path: &Path) -> std::io::Result<()> {
    ensure_directory_path(path)?;
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_dir() {
        return Err(super::private_file::permission_denied(
            "user output parent must be a directory",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn ensure_directory_path(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};

    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => continue,
            std::path::Component::ParentDir => {
                return Err(super::private_file::invalid_input(
                    "private file parent must not contain parent traversal",
                ));
            }
            _ => current.push(component.as_os_str()),
        }
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() && metadata.uid() != 0 => {
                return Err(super::private_file::permission_denied(
                    "private file parent path must not contain user-controlled symlinks",
                ));
            }
            Ok(metadata) if !metadata.file_type().is_symlink() && !metadata.is_dir() => {
                return Err(super::private_file::permission_denied(
                    "private file parent path must contain only directories",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match std::fs::DirBuilder::new().mode(0o700).create(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                let metadata = std::fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(super::private_file::permission_denied(
                        "private file parent creation raced with a non-directory path",
                    ));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn ensure_directory_path(path: &Path) -> std::io::Result<()> {
    use std::path::Component;

    // Security fix: validate each path component on Windows, mirroring the
    // Unix implementation's security guarantees.  The previous stub called
    // `create_dir_all` with no checks, enabling:
    //   1. Parent-directory traversal via `..` components
    //   2. Symlink / junction-point following
    //   3. TOCTOU races during directory creation

    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                return Err(super::private_file::invalid_input(
                    "private file parent must not contain parent traversal",
                ));
            }
            _ => current.push(component.as_os_str()),
        }

        match std::fs::symlink_metadata(&current) {
            Ok(metadata) => {
                // Reject symlinks
                if metadata.file_type().is_symlink() {
                    return Err(super::private_file::permission_denied(
                        "private file parent path must not contain symlinks",
                    ));
                }
                // Reject NTFS reparse points (junctions, mount points)
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err(super::private_file::permission_denied(
                            "private file parent path must not contain reparse points (junctions)",
                        ));
                    }
                }
                // Reject non-directory entries
                if !metadata.is_dir() {
                    return Err(super::private_file::permission_denied(
                        "private file parent path must contain only directories",
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Create the missing directory
                match std::fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                // Re-validate after creation to prevent TOCTOU races
                let metadata = std::fs::symlink_metadata(&current)?;
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(super::private_file::permission_denied(
                        "private file parent creation raced with a non-directory path",
                    ));
                }
                {
                    use std::os::windows::fs::MetadataExt;
                    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
                    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                        return Err(super::private_file::permission_denied(
                            "private file parent creation raced with a reparse point",
                        ));
                    }
                }
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn ensure_directory_path(path: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(path)
}

#[cfg(test)]
#[path = "private_file_parent_traversal_tests.rs"]
mod traversal_tests;
