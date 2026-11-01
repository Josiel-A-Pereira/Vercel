use std::os::unix::net::UnixStream;

use agent_desktop_core::{AdapterError, Deadline};
use wayland_client::Connection;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;

use crate::system::display::display_env::DisplayEnv;
use crate::system::display::display_mutter::LogicalMonitor;
use crate::system::display::display_wayland_assemble::assemble;
use crate::system::display::display_wayland_record::OutputRecord;
use crate::system::display::display_wayland_state::WaylandState;
use crate::system::display::wayland_link::{connect, roundtrip};

const OUTPUT_INTERFACE: &str = "wl_output";
const MANAGER_INTERFACE: &str = "zxdg_output_manager_v1";
const OUTPUT_VERSION: u32 = 4;
const MANAGER_VERSION: u32 = 2;

fn advertised(state: &WaylandState, interface: &str) -> Vec<(u32, u32)> {
    state
        .globals
        .iter()
        .filter(|global| global.interface == interface)
        .map(|global| (global.name, global.version))
        .collect()
}

/// The logical monitors read from the core `wl_output` and `xdg_output`
/// protocols, or `None` when the session has no compositor socket or the
/// compositor offers no `xdg_output`. Only the deadline itself is an error.
pub(crate) fn logical_monitors(
    deadline: Deadline,
) -> Result<Option<Vec<LogicalMonitor>>, AdapterError> {
    let Some(stream) = connect(&DisplayEnv::from_process()) else {
        return Ok(None);
    };
    read_from(stream, deadline)
}

/// Runs the registry and `xdg_output` round trips over an already connected
/// compositor stream.
fn read_from(
    stream: UnixStream,
    deadline: Deadline,
) -> Result<Option<Vec<LogicalMonitor>>, AdapterError> {
    let Ok(connection) = Connection::from_socket(stream) else {
        return Ok(None);
    };
    let mut queue = connection.new_event_queue::<WaylandState>();
    let qh = queue.handle();
    let mut state = WaylandState::default();
    let registry = connection.display().get_registry(&qh, ());
    if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
        return Ok(None);
    }
    let outputs = advertised(&state, OUTPUT_INTERFACE);
    let Some(&(manager_name, manager_version)) = advertised(&state, MANAGER_INTERFACE).first()
    else {
        return Ok(None);
    };
    if outputs.is_empty() {
        return Ok(None);
    }
    let manager: ZxdgOutputManagerV1 =
        registry.bind(manager_name, manager_version.min(MANAGER_VERSION), &qh, ());
    for (name, version) in outputs {
        let index = state.records.len();
        state.records.push(OutputRecord {
            global: name,
            integer_scale: 1,
            ..Default::default()
        });
        let output: WlOutput = registry.bind(name, version.min(OUTPUT_VERSION), &qh, index);
        manager.get_xdg_output(&output, &qh, index);
    }
    if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
        return Ok(None);
    }
    let monitors = assemble(&state.records);
    Ok((!monitors.is_empty()).then_some(monitors))
}

#[cfg(test)]
#[path = "display_wayland_tests.rs"]
mod tests;
