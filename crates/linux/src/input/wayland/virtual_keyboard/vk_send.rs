//! Delivers keys through the virtual keyboard once the route is known.

use std::sync::Arc;

use agent_desktop_core::{AdapterError, Deadline, ErrorCode};
use serde_json::json;

use super::typable::refuse_untypable;
use super::vk_port::VkPort;
use super::vk_session::VkSession;
use crate::input::delivery::{Delivery, Gate};
use crate::input::delivery_channel::DeliveryChannel;
use crate::input::keyboard::keysym_step::KeysymStep;
use crate::input::wayland::keysym_send::run_chunks;
use crate::input::x11::progress::Progress;
use crate::input::x11::signal_release;
use crate::input::x11::worker_slot;

/// Types `live` on an opened virtual keyboard. `chunks` is the whole request,
/// checked before the session uploads a keymap, so refused text puts nothing
/// on the wire. The keyboard is released when the delivery ends.
pub(crate) fn send_on<P: VkPort>(
    port: P,
    chunks: &[Vec<KeysymStep>],
    live: &[&[KeysymStep]],
    gate: Gate<'_>,
    deadline: Deadline,
) -> Result<Delivery, AdapterError> {
    refuse_untypable(chunks)?;
    let slot = worker_slot::shared();
    let _claim = slot.claim(deadline)?;
    let progress = Arc::new(Progress::default());
    let _armed = signal_release::arm(Arc::clone(&progress), Arc::clone(&slot));
    deliver(port, live, &progress, gate, deadline)
}

/// Runs the delivery and drops the session before returning, so the keyboard
/// is destroyed before a stop signal is allowed to end the process.
fn deliver<P: VkPort>(
    port: P,
    live: &[&[KeysymStep]],
    progress: &Progress,
    gate: Gate<'_>,
    deadline: Deadline,
) -> Result<Delivery, AdapterError> {
    let mut session = VkSession::new(port);
    let mut stoppable = || {
        if progress.is_cancelled() {
            Err(stop_requested())
        } else {
            gate()
        }
    };
    run_chunks(&mut session, live, &mut stoppable, deadline).map(|delivery| Delivery {
        channel: DeliveryChannel::VirtualKeyboard,
        ..delivery
    })
}

fn stop_requested() -> AdapterError {
    AdapterError::new(
        ErrorCode::ActionFailed,
        "Key delivery stopped because the process received a stop signal",
    )
    .with_details(json!({ "cause": "stop_signal" }))
}

#[cfg(test)]
#[path = "vk_send_tests.rs"]
mod tests;
