use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, WindowInfo};

use super::foreign_link::ForeignLink;
use super::foreign_names::ProcessNames;
use super::foreign_target::ForeignTarget;

fn reached<L>(opened: Result<Option<L>, AdapterError>) -> Option<Result<L, AdapterError>> {
    match opened {
        Ok(Some(link)) => Some(Ok(link)),
        Ok(None) => None,
        Err(error) => Some(Err(
            error.with_disposition(DeliverySemantics::not_delivered())
        )),
    }
}

/// Opens a link to the compositor's toplevel list, or answers `None` when the
/// session offers no foreign toplevel manager. A link that cannot be opened in
/// time is a failure with nothing delivered.
pub(in crate::system) fn open_link(
    deadline: Deadline,
) -> Option<Result<ForeignLink, AdapterError>> {
    reached(ForeignLink::open(deadline))
}

/// Runs `act` on the compositor's toplevel list, or answers `None` when the
/// session offers no foreign toplevel manager so the caller keeps its own
/// answer. A link that cannot be opened in time is a failure with nothing
/// delivered.
pub(in crate::system) fn with_link<T>(
    deadline: Deadline,
    act: impl FnOnce(&ForeignLink) -> Result<T, AdapterError>,
) -> Option<Result<T, AdapterError>> {
    let link = match open_link(deadline)? {
        Ok(link) => link,
        Err(error) => return Some(Err(error)),
    };
    Some(act(&link))
}

/// Runs `act` on the compositor's toplevel list for the window, or answers
/// `None` when the session offers no foreign toplevel manager so the caller
/// keeps its own refusal. A link that cannot be opened in time is a failure
/// with nothing delivered.
pub(in crate::system) fn through_link<T>(
    win: &WindowInfo,
    deadline: Deadline,
    act: impl FnOnce(&ForeignLink, &ForeignTarget<'_>, Deadline) -> Result<T, AdapterError>,
) -> Option<Result<T, AdapterError>> {
    with_link(deadline, |link| {
        let target = ForeignTarget {
            title: &win.title,
            names: ProcessNames::of(win.pid.get(), &win.app)
                .with_instance(win.process_instance.as_deref().unwrap_or_default()),
        };
        act(link, &target, deadline)
    })
}

#[cfg(test)]
#[path = "foreign_route_tests.rs"]
mod tests;
