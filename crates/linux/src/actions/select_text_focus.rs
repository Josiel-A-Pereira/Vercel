use std::collections::{HashSet, VecDeque};

use agent_desktop_core::{AdapterError, Deadline};
use zbus::Connection;

use crate::system::SharedBus;
use crate::system::window::window_id;
use crate::system::{ACCESSIBLE_INTERFACE, focused_window, within};
use crate::tree::vocabulary::state_bits::{FOCUSED, StateBits};
use crate::tree::{AtspiElement, call, list_children};

const MAX_FOCUS_NODES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct GlobalFocus {
    pub(super) window: String,
    pub(super) element: String,
}

impl GlobalFocus {
    pub(super) fn effects_from(&self, before: &Self) -> (bool, bool) {
        (self != before, self.window != before.window)
    }
}

async fn focused_element(
    connection: &Connection,
    frame: AtspiElement,
    deadline: Deadline,
) -> Result<Option<String>, AdapterError> {
    let mut pending = VecDeque::from([frame]);
    let mut seen = HashSet::new();
    let mut focused = None;
    while let Some(element) = pending.pop_front() {
        deadline.ensure_remaining()?;
        if !seen.insert(element.object_identity()) {
            continue;
        }
        if seen.len() > MAX_FOCUS_NODES {
            return Ok(None);
        }
        let states = match call(connection, &element, ACCESSIBLE_INTERFACE, "GetState", &()).await {
            Ok(reply) => match reply.body().deserialize::<Vec<u32>>() {
                Ok(states) => states,
                Err(_) => return Ok(None),
            },
            Err(_) => return Ok(None),
        };
        if StateBits::new(&states).has(FOCUSED) {
            if focused.replace(element.object_identity()).is_some() {
                return Ok(None);
            }
        }
        let children = match list_children(connection, &element).await {
            Ok(children) => children,
            Err(_) => return Ok(None),
        };
        pending.extend(
            children
                .into_iter()
                .map(|(bus, path)| AtspiElement::new(bus, path, element.pid())),
        );
    }
    Ok(focused)
}

pub(super) fn capture(
    bus: &SharedBus,
    deadline: Deadline,
) -> Result<Option<GlobalFocus>, AdapterError> {
    let Some(window) = focused_window(bus, deadline)? else {
        return Ok(None);
    };
    let Ok((bus_name, path)) = window_id::decode(&window.id) else {
        return Ok(None);
    };
    let frame = AtspiElement::new(bus_name, path, window.pid.get());
    let connection = bus.get(deadline)?.inner().clone();
    let element = within(deadline, focused_element(&connection, frame, deadline))?;
    Ok(element.map(|element| GlobalFocus {
        window: window.id,
        element,
    }))
}

#[cfg(test)]
#[path = "select_text_focus_tests.rs"]
mod tests;
