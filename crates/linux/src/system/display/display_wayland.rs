use std::os::fd::BorrowedFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use agent_desktop_core::{AdapterError, Deadline};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use wayland_client::protocol::wl_output::WlOutput;
use wayland_client::{Connection, EventQueue};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::ZxdgOutputManagerV1;

use crate::system::display::display_env::DisplayEnv;
use crate::system::display::display_mutter::LogicalMonitor;
use crate::system::display::display_server::wayland_socket;
use crate::system::display::display_wayland_assemble::assemble;
use crate::system::display::display_wayland_record::OutputRecord;
use crate::system::display::display_wayland_state::WaylandState;

const OUTPUT_INTERFACE: &str = "wl_output";
const MANAGER_INTERFACE: &str = "zxdg_output_manager_v1";
const OUTPUT_VERSION: u32 = 4;
const MANAGER_VERSION: u32 = 2;

fn poll_timeout(deadline: Deadline) -> Result<Timespec, AdapterError> {
    let remaining = deadline.remaining();
    if remaining.is_zero() {
        return Err(deadline.timeout_error());
    }
    let padded = remaining.saturating_add(Duration::from_millis(1));
    Ok(Timespec {
        tv_sec: i64::try_from(padded.as_secs()).unwrap_or(i64::MAX),
        tv_nsec: i64::from(padded.subsec_nanos()),
    })
}

fn wait_readable(fd: BorrowedFd<'_>, deadline: Deadline) -> Result<bool, AdapterError> {
    loop {
        let timeout = poll_timeout(deadline)?;
        let mut entries = [PollFd::new(&fd, PollFlags::IN)];
        match poll(&mut entries, Some(&timeout)) {
            Ok(0) => return Err(deadline.timeout_error()),
            Ok(_) => return Ok(true),
            Err(Errno::INTR) => {}
            Err(_) => return Ok(false),
        }
    }
}

fn would_block(error: &wayland_client::backend::WaylandError) -> bool {
    matches!(
        error,
        wayland_client::backend::WaylandError::Io(io) if io.kind() == std::io::ErrorKind::WouldBlock
    )
}

/// One sync round trip bounded by the deadline. `Ok(false)` means the
/// connection broke; only the deadline running out is an error.
fn roundtrip(
    connection: &Connection,
    queue: &mut EventQueue<WaylandState>,
    state: &mut WaylandState,
    deadline: Deadline,
) -> Result<bool, AdapterError> {
    state.done = false;
    let _callback = connection.display().sync(&queue.handle(), ());
    loop {
        if queue.dispatch_pending(state).is_err() {
            return Ok(false);
        }
        if state.done {
            return Ok(true);
        }
        if let Err(error) = connection.flush()
            && !would_block(&error)
        {
            return Ok(false);
        }
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        if !wait_readable(guard.connection_fd(), deadline)? {
            return Ok(false);
        }
        if let Err(error) = guard.read()
            && !would_block(&error)
        {
            return Ok(false);
        }
    }
}

fn connect(env: &DisplayEnv) -> Option<UnixStream> {
    let name = env.wayland.as_deref()?;
    let path = wayland_socket(env, name)?;
    UnixStream::connect(path).ok()
}

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
