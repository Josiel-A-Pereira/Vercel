use agent_desktop_core::{AdapterError, Deadline};

use super::foreign_active_watch::PinnedToplevel;
use super::foreign_await::flags_of;
use super::foreign_names::ProcessNames;
use super::foreign_port::ToplevelPort;
use super::foreign_proof::{Proof, prove};
use super::foreign_refusal::control_failure;
use super::foreign_route::{open_link, with_link};
use super::foreign_target::ForeignTarget;
use crate::system::window::active_watch::ActiveWatch;

/// The id of the toplevel the compositor calls activated and the list proves
/// to be this program's single window with this title, from one sync of its
/// toplevel list. A missing, ambiguous, unrelated or inactive window is
/// `None`. Nothing is sent.
pub(super) fn activated_id(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<Option<u32>, AdapterError> {
    port.refresh(deadline)
        .map_err(|error| control_failure(error, deadline))?;
    match prove(target.title, &target.names, &port.records()) {
        Proof::One(id) => Ok(flags_of(port, id)
            .is_some_and(|flags| flags.activated)
            .then_some(id)),
        Proof::Several(_) | Proof::NoWindow | Proof::Unrelated(_) => Ok(None),
    }
}

/// Whether the compositor itself calls the toplevel the target stands for
/// activated.
fn proves_active(
    port: &impl ToplevelPort,
    target: &ForeignTarget<'_>,
    deadline: Deadline,
) -> Result<bool, AdapterError> {
    Ok(activated_id(port, target, deadline)?.is_some())
}

/// Reads the compositor's own `activated` state for a window named by its
/// process and title. `None` means the session offers no foreign toplevel
/// manager, so the caller keeps its own proof; a link that cannot be opened
/// or synced is an error, never a pass. `title` is read only once a link
/// exists, so a session without a manager pays no title read.
pub(crate) fn compositor_active(
    pid: u32,
    app: &str,
    title: impl FnOnce() -> Result<String, AdapterError>,
    deadline: Deadline,
) -> Option<Result<bool, AdapterError>> {
    with_link(deadline, |link| {
        let title = title()?;
        let target = ForeignTarget {
            title: &title,
            names: ProcessNames::of(pid, app),
        };
        proves_active(link, &target, deadline)
    })
}

/// Pins the compositor's activated toplevel for a window named by its process
/// and title on one kept link, so later checks ask only about that toplevel
/// and ignore its title. The outer `None` means the session offers no foreign
/// toplevel manager; `Ok(None)` means the list does not prove the window
/// activated at this moment.
pub(crate) fn watch_active(
    pid: u32,
    app: &str,
    title: impl FnOnce() -> Result<String, AdapterError>,
    deadline: Deadline,
) -> Option<Result<Option<Box<dyn ActiveWatch>>, AdapterError>> {
    let link = match open_link(deadline)? {
        Ok(link) => link,
        Err(error) => return Some(Err(error)),
    };
    let title = match title() {
        Ok(title) => title,
        Err(error) => return Some(Err(error)),
    };
    let target = ForeignTarget {
        title: &title,
        names: ProcessNames::of(pid, app),
    };
    Some(
        PinnedToplevel::pin(link, &target, deadline)
            .map(|pinned| pinned.map(|pinned| Box::new(pinned) as Box<dyn ActiveWatch>)),
    )
}

/// Whether the compositor offers a foreign toplevel manager, so a headed
/// delivery can activate the target window before it sends keys.
pub(crate) fn offers_window_control(deadline: Deadline) -> bool {
    with_link(deadline, |_| Ok(())).is_some_and(|opened| opened.is_ok())
}

#[cfg(test)]
#[path = "foreign_active_tests.rs"]
mod tests;
