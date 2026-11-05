use std::time::{Duration, Instant};

use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};
use serde_json::json;

use crate::input::deliver::ensure_budget;
use crate::input::physical::target_world::PhysicalWorld;

use crate::system::process::process_facts;
use crate::system::window::active_watch::ActiveWatch;
use crate::system::{ACCESSIBLE_INTERFACE, listed_app_name};
use crate::system::{SharedBus, within};
use crate::tree::call;
use crate::tree::properties::absence::gone_error;
use crate::tree::properties::liveness::Liveness;
use crate::tree::vocabulary::state_bits::{ACTIVE, StateBits};
use crate::tree::{AtspiElement, Field, ReadPurpose, read_node_for};

/// Reads the frame's own `ACTIVE` state. `None` means the state set could not
/// be read, which is not the same as the frame being inactive.
pub(crate) fn frame_active(
    bus: &SharedBus,
    frame: &AtspiElement,
    deadline: Deadline,
) -> Result<Option<bool>, AdapterError> {
    let shared = bus.get(deadline)?;
    within(deadline, async {
        let (role, states) = futures_util::future::join(
            call(shared.inner(), frame, ACCESSIBLE_INTERFACE, "GetRole", &()),
            call(shared.inner(), frame, ACCESSIBLE_INTERFACE, "GetState", &()),
        )
        .await;
        let role = role.and_then(|reply| reply.body().deserialize::<u32>());
        let liveness = Liveness::of(role.as_ref().err());
        for error in [role.as_ref().err(), states.as_ref().err()]
            .into_iter()
            .flatten()
        {
            if crate::system::is_connection_failure(error) {
                return Err(crate::system::classify(error));
            }
            if let Some(error) = gone_error(error, liveness) {
                return Err(error);
            }
        }
        if role.is_err() {
            return Ok(None);
        }
        Ok(states
            .ok()
            .and_then(|reply| reply.body().deserialize::<Vec<u32>>().ok())
            .and_then(|words| active_of(&Field::Present(words))))
    })
}

pub(crate) fn active_of(states: &Field<Vec<u32>>) -> Option<bool> {
    states
        .present()
        .map(|words| StateBits::new(words).has(ACTIVE))
}

/// The frame's title as the window listing reads it, which is the title the
/// compositor's toplevel is matched on.
fn frame_title(
    bus: &SharedBus,
    frame: &AtspiElement,
    deadline: Deadline,
) -> Result<String, AdapterError> {
    let shared = bus.get(deadline)?;
    within(deadline, async {
        let (node, _) = read_node_for(shared.inner(), frame, ReadPurpose::Frame).await?;
        Ok(node.properties.name.present().cloned().unwrap_or_default())
    })
}

/// Pins the compositor's activated toplevel for the frame on one kept link.
/// `None` when the session has no compositor window list; a window the list
/// does not prove activated refuses, as the first check does.
pub(crate) fn compositor_watch(
    bus: &SharedBus,
    frame: &AtspiElement,
    deadline: Deadline,
) -> Result<Option<Box<dyn ActiveWatch>>, AdapterError> {
    let pid = frame.pid();
    let app = process_facts::read(pid)
        .map(|facts| listed_app_name(&facts))
        .unwrap_or_default();
    match crate::system::window::watch_active(
        pid,
        &app,
        || frame_title(bus, frame, deadline),
        deadline,
    ) {
        None => Ok(None),
        Some(Err(error)) => Err(error),
        Some(Ok(Some(watch))) => Ok(Some(watch)),
        Some(Ok(None)) => Err(window_inactive(COMPOSITOR_CAUSE)),
    }
}

/// What the compositor says about the frame: `None` when the session has no
/// compositor window list, else whether its toplevel is activated. The
/// connection is opened per read; a gate reads at most once per key press, and
/// no cache is kept because a stale answer is exactly what the gate refuses.
pub(crate) fn compositor_active(
    bus: &SharedBus,
    frame: &AtspiElement,
    deadline: Deadline,
) -> Result<Option<bool>, AdapterError> {
    let pid = frame.pid();
    let app = process_facts::read(pid)
        .map(|facts| listed_app_name(&facts))
        .unwrap_or_default();
    crate::system::window::compositor_active(
        pid,
        &app,
        || frame_title(bus, frame, deadline),
        deadline,
    )
    .transpose()
}

const ACTIVE_BUDGET: Duration = Duration::from_millis(1500);
const ACTIVE_POLL: Duration = Duration::from_millis(50);
const COMPOSITOR_CAUSE: &str = "compositor_not_activated";

fn poll_until(
    world: &impl PhysicalWorld,
    budget: Duration,
    deadline: Deadline,
    done: fn(Option<bool>) -> bool,
    mut read: impl FnMut() -> Result<Option<bool>, AdapterError>,
) -> Result<Option<bool>, AdapterError> {
    let reads = budget.as_millis() / ACTIVE_POLL.as_millis();
    let mut last = None;
    for attempt in 0..=reads {
        ensure_budget(deadline)?;
        last = read()?;
        if done(last) {
            break;
        }
        if attempt < reads {
            world.pause(ACTIVE_POLL);
        }
    }
    Ok(last)
}

pub(crate) fn poll_active(
    world: &impl PhysicalWorld,
    element: &AtspiElement,
    budget: Duration,
    deadline: Deadline,
) -> Result<Option<bool>, AdapterError> {
    poll_until(
        world,
        budget,
        deadline,
        |read| read == Some(true),
        || world.frame_active(element, deadline),
    )
}

fn poll_compositor(
    world: &impl PhysicalWorld,
    element: &AtspiElement,
    budget: Duration,
    deadline: Deadline,
) -> Result<Option<bool>, AdapterError> {
    poll_until(
        world,
        budget,
        deadline,
        |read| read != Some(false),
        || world.compositor_active(element, deadline),
    )
}

/// The focus gate of one keyboard delivery. The first check waits for the
/// frame to become active, and for the compositor to agree where it lists its
/// windows, and refuses the call before any key is sent. The first check then
/// pins the toplevel the compositor proved on one kept link; later checks,
/// made between chunks and keys, read once, ask only whether that toplevel is
/// still there and activated, and stop the delivery where it is.
pub(crate) struct ActiveGate<'a, W: PhysicalWorld> {
    world: &'a W,
    element: &'a AtspiElement,
    deadline: Deadline,
    checked: bool,
    watch: Option<Box<dyn ActiveWatch>>,
}

impl<'a, W: PhysicalWorld> ActiveGate<'a, W> {
    pub(crate) fn new(world: &'a W, element: &'a AtspiElement, deadline: Deadline) -> Self {
        Self {
            world,
            element,
            deadline,
            checked: false,
            watch: None,
        }
    }

    pub(crate) fn check(&mut self) -> Result<(), AdapterError> {
        let first = !self.checked;
        self.checked = true;
        let budget = if first { ACTIVE_BUDGET } else { Duration::ZERO };
        let started = Instant::now();
        match poll_active(self.world, self.element, budget, self.deadline)? {
            Some(true) => {}
            read if first => return Err(window_inactive(cause_of(read))),
            _ => return Err(window_lost("lost")),
        }
        if let Some(watch) = &self.watch {
            return if watch.active(self.deadline)? {
                Ok(())
            } else {
                Err(window_lost(COMPOSITOR_CAUSE))
            };
        }
        let left = budget.saturating_sub(started.elapsed());
        match poll_compositor(self.world, self.element, left, self.deadline)? {
            Some(false) if first => Err(window_inactive(COMPOSITOR_CAUSE)),
            Some(false) => Err(window_lost(COMPOSITOR_CAUSE)),
            Some(true) => {
                if first {
                    self.watch = self.world.compositor_watch(self.element, self.deadline)?;
                }
                Ok(())
            }
            None => Ok(()),
        }
    }
}

fn cause_of(read: Option<bool>) -> &'static str {
    match read {
        Some(_) => "not_active",
        None => "unreadable",
    }
}

fn window_inactive(cause: &'static str) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "The target window could not be confirmed active, so no key was sent",
    )
    .with_suggestion(
        "A pending system dialog can hold the keyboard; dismiss it, or focus the window, then retry",
    )
    .with_details(json!({
        "check": "window_active",
        "cause": cause,
        "physical_delivery_started": false,
    }))
    .with_disposition(DeliverySemantics::not_delivered())
}

fn window_lost(cause: &'static str) -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "The target window could no longer be confirmed active while keys were being sent",
    )
    .with_suggestion("Focus the window again, then retry the remaining input")
    .with_details(json!({ "check": "window_active", "cause": cause }))
}

#[cfg(test)]
#[path = "frame_active_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "frame_active_live_tests.rs"]
mod live_tests;
