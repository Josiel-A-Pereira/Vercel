use agent_desktop_core::Deadline;

use super::foreign_awaited::Awaited;
use super::foreign_flags::ToplevelFlags;
use super::foreign_port::ToplevelPort;
use crate::system::window::window_op::window_op_verdict::POLL;

pub(in crate::system) fn flags_of(port: &impl ToplevelPort, id: u32) -> Option<ToplevelFlags> {
    port.records()
        .into_iter()
        .find(|record| record.id == id)
        .map(|record| record.flags)
}

/// Syncs with the compositor until the toplevel's flags satisfy `holds`, the
/// toplevel goes away, a sync fails, or the limit passes. Flags are only read
/// after a sync that succeeded.
pub(in crate::system) fn await_flags(
    port: &impl ToplevelPort,
    id: u32,
    holds: impl Fn(&ToplevelFlags) -> bool,
    limit: Deadline,
) -> Awaited {
    loop {
        if let Err(error) = port.refresh(limit) {
            return Awaited::Unsynced(error);
        }
        match flags_of(port, id) {
            None => return Awaited::Gone,
            Some(flags) if holds(&flags) => return Awaited::Reached,
            Some(flags) if limit.is_expired() => return Awaited::Unmet(flags),
            Some(_) => std::thread::sleep(POLL.min(limit.remaining())),
        }
    }
}

#[cfg(test)]
#[path = "foreign_await_tests.rs"]
mod tests;
