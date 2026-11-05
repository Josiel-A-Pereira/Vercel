//! The keymap as the sealed in-memory file the compositor maps.

use std::fs::File;
use std::io::Write;
use std::os::fd::OwnedFd;

use agent_desktop_core::{AdapterError, DeliverySemantics, ErrorCode};
use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};

fn keymap_error(message: String) -> AdapterError {
    AdapterError::new(ErrorCode::ActionFailed, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

/// The keymap bytes with the trailing NUL the protocol counts in `size`.
pub(super) fn with_nul(text: &str) -> Vec<u8> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// A sealed memfd holding the keymap and its size including the NUL.
pub(super) fn sealed_keymap(text: &str) -> Result<(OwnedFd, u32), AdapterError> {
    let bytes = with_nul(text);
    let size = u32::try_from(bytes.len())
        .map_err(|_| keymap_error("The keymap is too large to upload".to_owned()))?;
    let descriptor = memfd_create(
        "ad-virtual-keymap",
        MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING,
    )
    .map_err(|error| keymap_error(format!("The keymap file could not be made: {error}")))?;
    let mut file = File::from(descriptor);
    file.write_all(&bytes)
        .map_err(|error| keymap_error(format!("The keymap could not be written: {error}")))?;
    let descriptor = OwnedFd::from(file);
    fcntl_add_seals(
        &descriptor,
        SealFlags::SHRINK | SealFlags::GROW | SealFlags::WRITE | SealFlags::SEAL,
    )
    .map_err(|error| keymap_error(format!("The keymap file could not be sealed: {error}")))?;
    Ok((descriptor, size))
}

#[cfg(test)]
#[path = "keymap_file_tests.rs"]
mod tests;
