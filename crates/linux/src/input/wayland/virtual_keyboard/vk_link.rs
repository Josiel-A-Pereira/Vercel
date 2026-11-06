//! The live connection to the compositor's virtual keyboard manager.

use std::os::fd::AsFd;
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

use agent_desktop_core::{AdapterError, Deadline, ErrorCode};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{Connection, EventQueue};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1;
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1;

use super::keymap_file::sealed_keymap;
use super::vk_port::VkPort;
use super::vk_state::VkState;
use crate::system::display::display_env::DisplayEnv;
use crate::system::display::wayland_link::{connect, roundtrip};

const MANAGER_VERSION: u32 = 1;
const SEAT_VERSION: u32 = 1;
const XKB_V1_FORMAT: u32 = 1;
const KEY_RELEASED: u32 = 0;
const KEY_PRESSED: u32 = 1;
/// How long a failed delivery waits so the clients consume the last key
/// groups before the keyboard is destroyed.
const DRAIN: Duration = Duration::from_millis(150);
/// How long the first keyboard of a seat gives clients to learn the seat now
/// has a keyboard and give it focus, before the first key; a key sent sooner
/// is dropped (measured with a GTK client on Hyprland).
const FIRST_KEYBOARD_SETTLE: Duration = Duration::from_millis(100);

/// One virtual keyboard on its own compositor connection. Requests queue in
/// the connection buffer and reach the socket only at `sync` and `release`.
pub(crate) struct VkLink {
    connection: Connection,
    queue: EventQueue<VkState>,
    state: VkState,
    keyboard: ZwpVirtualKeyboardV1,
    _manager: ZwpVirtualKeyboardManagerV1,
    opened: Instant,
}

impl VkLink {
    /// Connects and creates the keyboard, or `None` when the session has no
    /// compositor socket or the compositor offers no virtual keyboard
    /// manager. Nothing has been sent as a key when it returns. Only the
    /// deadline itself is an error.
    pub(crate) fn open(deadline: Deadline) -> Result<Option<Self>, AdapterError> {
        let Some(stream) = connect(&DisplayEnv::from_process()) else {
            return Ok(None);
        };
        Self::open_on(stream, deadline)
    }

    fn open_on(stream: UnixStream, deadline: Deadline) -> Result<Option<Self>, AdapterError> {
        let Ok(connection) = Connection::from_socket(stream) else {
            return Ok(None);
        };
        let mut queue = connection.new_event_queue::<VkState>();
        let qh = queue.handle();
        let mut state = VkState::default();
        let registry = connection.display().get_registry(&qh, ());
        if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
            return Ok(None);
        }
        let (Some((manager_name, manager_version)), Some((seat_name, seat_version))) =
            (state.manager, state.seat)
        else {
            return Ok(None);
        };
        let manager: ZwpVirtualKeyboardManagerV1 =
            registry.bind(manager_name, manager_version.min(MANAGER_VERSION), &qh, ());
        let seat: WlSeat = registry.bind(seat_name, seat_version.min(SEAT_VERSION), &qh, ());
        if !roundtrip(&connection, &mut queue, &mut state, deadline)? {
            return Ok(None);
        }
        state.alone = !state.seat_keyboard;
        let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());
        Ok(Some(Self {
            connection,
            queue,
            state,
            keyboard,
            _manager: manager,
            opened: Instant::now(),
        }))
    }

    fn time_ms(&self) -> u32 {
        u32::try_from(self.opened.elapsed().as_millis()).unwrap_or(u32::MAX)
    }

    fn broken(&self) -> AdapterError {
        let detail = self.connection.protocol_error().map_or_else(
            || "the connection closed".to_owned(),
            |error| {
                format!(
                    "{} (code {}) on {}",
                    error.message, error.code, error.object_interface
                )
            },
        );
        AdapterError::new(
            ErrorCode::ActionFailed,
            format!("The compositor ended the virtual keyboard connection: {detail}"),
        )
        .with_suggestion(
            "Retry the command; read the target's state first if keys may have been sent",
        )
    }
}

impl VkPort for VkLink {
    fn upload_keymap(&mut self, text: &str) -> Result<(), AdapterError> {
        let (file, size) = sealed_keymap(text)?;
        self.keyboard.keymap(XKB_V1_FORMAT, file.as_fd(), size);
        Ok(())
    }

    fn key(&mut self, evdev: u32, down: bool) -> Result<(), AdapterError> {
        let state = if down { KEY_PRESSED } else { KEY_RELEASED };
        self.keyboard.key(self.time_ms(), evdev, state);
        Ok(())
    }

    fn modifiers(&mut self, mask: u32) -> Result<(), AdapterError> {
        self.keyboard.modifiers(mask, 0, 0, 0);
        Ok(())
    }

    fn sync(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        match roundtrip(&self.connection, &mut self.queue, &mut self.state, deadline)? {
            true => Ok(()),
            false => Err(self.broken()),
        }
    }

    fn settle(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        if !std::mem::take(&mut self.state.alone) {
            return Ok(());
        }
        std::thread::sleep(deadline.remaining_slice(FIRST_KEYBOARD_SETTLE)?);
        Ok(())
    }

    fn drain(&mut self, deadline: Deadline) {
        if let Ok(slice) = deadline.remaining_slice(DRAIN) {
            std::thread::sleep(slice);
        }
    }

    fn release(&mut self, deadline: Deadline) -> Result<(), AdapterError> {
        self.keyboard.destroy();
        self.sync(deadline)
    }
}
