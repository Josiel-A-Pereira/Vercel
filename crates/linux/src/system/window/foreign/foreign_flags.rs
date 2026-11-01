const MAXIMIZED: u32 = 0;
const MINIMIZED: u32 = 1;
const ACTIVATED: u32 = 2;
const FULLSCREEN: u32 = 3;

/// The state a compositor reports for one toplevel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::system) struct ToplevelFlags {
    pub(in crate::system) maximized: bool,
    pub(in crate::system) minimized: bool,
    pub(in crate::system) activated: bool,
    pub(in crate::system) fullscreen: bool,
}

impl ToplevelFlags {
    /// Decodes the protocol's state array of little-endian words. Bytes past
    /// the last whole word and words this module does not know are ignored.
    pub(in crate::system) fn from_state_words(bytes: &[u8]) -> Self {
        let mut flags = Self::default();
        for chunk in bytes.chunks_exact(4) {
            let Ok(word) = <[u8; 4]>::try_from(chunk) else {
                continue;
            };
            match u32::from_le_bytes(word) {
                MAXIMIZED => flags.maximized = true,
                MINIMIZED => flags.minimized = true,
                ACTIVATED => flags.activated = true,
                FULLSCREEN => flags.fullscreen = true,
                _ => {}
            }
        }
        flags
    }
}

#[cfg(test)]
#[path = "foreign_flags_tests.rs"]
mod tests;
