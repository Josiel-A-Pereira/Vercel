use std::{borrow::Cow, io::Read};

use agent_desktop_core::AppError;

use super::preparation::MAX_BATCH_JSON_BYTES;

/// The `JSON` argument that makes `batch` read its entries from standard input.
pub(super) const STDIN_ARGUMENT: &str = "-";

/// Returns the batch JSON: the argument itself, or, when the argument is `-`, standard input
/// read to its end. A value read from stdin never appears in the process list. Reading stops
/// one byte past the input limit, so an oversized input fails the same limit check as an
/// oversized argument.
pub(super) fn resolve<'a>(argument: &'a str, stdin: impl Read) -> Result<Cow<'a, str>, AppError> {
    if argument != STDIN_ARGUMENT {
        return Ok(Cow::Borrowed(argument));
    }
    let mut bytes = Vec::new();
    stdin
        .take(MAX_BATCH_JSON_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::invalid_input(format!("Could not read batch JSON from stdin: {error}"))
        })?;
    String::from_utf8(bytes)
        .map(Cow::Owned)
        .map_err(|_| AppError::invalid_input("Batch JSON on stdin must be UTF-8"))
}

#[cfg(test)]
#[path = "input_tests.rs"]
mod tests;
