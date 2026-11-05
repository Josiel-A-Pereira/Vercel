//! One request of a group on the wire.

/// A `key` or `modifiers` request, in the order a group writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WireOp {
    Key { evdev: u32, down: bool },
    Modifiers(u32),
}
