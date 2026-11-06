//! The bounded socket plumbing every Wayland reader shares: finding the
//! compositor socket, waiting on it under a deadline and running one sync
//! round trip.

use std::os::fd::BorrowedFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use agent_desktop_core::{AdapterError, Deadline};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use rustix::io::Errno;
use wayland_client::protocol::wl_callback::WlCallback;
use wayland_client::{Connection, Dispatch, EventQueue};

use crate::system::display::display_env::DisplayEnv;
use crate::system::display::display_server::wayland_socket;

/// A dispatch state that records when a sync callback has fired.
pub(crate) trait SyncDone {
    fn sync_done(&mut self) -> &mut bool;
}

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
pub(crate) fn roundtrip<S>(
    connection: &Connection,
    queue: &mut EventQueue<S>,
    state: &mut S,
    deadline: Deadline,
) -> Result<bool, AdapterError>
where
    S: Dispatch<WlCallback, ()> + SyncDone + 'static,
{
    *state.sync_done() = false;
    let _callback = connection.display().sync(&queue.handle(), ());
    loop {
        if queue.dispatch_pending(state).is_err() {
            return Ok(false);
        }
        if *state.sync_done() {
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

pub(crate) fn connect(env: &DisplayEnv) -> Option<UnixStream> {
    let name = env.wayland.as_deref()?;
    let path = wayland_socket(env, name)?;
    UnixStream::connect(path).ok()
}
