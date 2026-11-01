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

/// Runs `act` on the compositor's toplevel list for the window, or answers
/// `None` when the session offers no foreign toplevel manager so the caller
/// keeps its own refusal. A link that cannot be opened in time is a failure
/// with nothing delivered.
pub(in crate::system) fn through_link(
    win: &WindowInfo,
    deadline: Deadline,
    act: impl FnOnce(&ForeignLink, &ForeignTarget<'_>, Deadline) -> Result<(), AdapterError>,
) -> Option<Result<(), AdapterError>> {
    let link = match reached(ForeignLink::open(deadline))? {
        Ok(link) => link,
        Err(error) => return Some(Err(error)),
    };
    let target = ForeignTarget {
        title: &win.title,
        names: ProcessNames::of(win.pid.get(), &win.app),
    };
    Some(act(&link, &target, deadline))
}

#[cfg(test)]
#[path = "foreign_route_tests.rs"]
mod tests;
