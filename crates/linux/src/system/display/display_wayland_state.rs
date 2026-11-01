use wayland_client::protocol::wl_callback::{self, WlCallback};
use wayland_client::protocol::wl_output::{self, Mode, WlOutput};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_manager_v1::{
    self, ZxdgOutputManagerV1,
};
use wayland_protocols::xdg::xdg_output::zv1::client::zxdg_output_v1::{self, ZxdgOutputV1};

use crate::system::display::display_wayland_record::OutputRecord;
use crate::system::display::wayland_link::SyncDone;

/// One global the compositor advertised on the registry.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Global {
    pub(crate) name: u32,
    pub(crate) interface: String,
    pub(crate) version: u32,
}

/// Everything the output protocols have reported so far, filled in by the
/// event handlers while the connection is dispatched.
#[derive(Debug, Default)]
pub(crate) struct WaylandState {
    pub(crate) records: Vec<OutputRecord>,
    pub(crate) globals: Vec<Global>,
    pub(crate) done: bool,
}

impl SyncDone for WaylandState {
    fn sync_done(&mut self) -> &mut bool {
        &mut self.done
    }
}

impl WaylandState {
    pub(crate) fn on_global(&mut self, name: u32, interface: String, version: u32) {
        self.globals.push(Global {
            name,
            interface,
            version,
        });
    }

    pub(crate) fn on_output_transform(&mut self, index: usize, transform: u32) {
        if let Some(record) = self.records.get_mut(index) {
            record.transform = transform;
        }
    }

    pub(crate) fn on_output_mode(&mut self, index: usize, current: bool, width: i32, height: i32) {
        if !current {
            return;
        }
        if let Some(record) = self.records.get_mut(index) {
            record.mode = Some((width, height));
        }
    }

    pub(crate) fn on_output_scale(&mut self, index: usize, factor: i32) {
        if let Some(record) = self.records.get_mut(index) {
            record.integer_scale = factor;
        }
    }

    pub(crate) fn on_output_name(&mut self, index: usize, name: String) {
        if let Some(record) = self.records.get_mut(index) {
            record.output_name = Some(name);
        }
    }

    pub(crate) fn on_logical_position(&mut self, index: usize, x: i32, y: i32) {
        if let Some(record) = self.records.get_mut(index) {
            record.position = Some((x, y));
        }
    }

    pub(crate) fn on_logical_size(&mut self, index: usize, width: i32, height: i32) {
        if let Some(record) = self.records.get_mut(index) {
            record.size = Some((width, height));
        }
    }

    pub(crate) fn on_logical_name(&mut self, index: usize, name: String) {
        if let Some(record) = self.records.get_mut(index) {
            record.logical_name = Some(name);
        }
    }
}

impl Dispatch<WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            state.on_global(name, interface, version);
        }
    }
}

impl Dispatch<WlCallback, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &WlCallback,
        event: wl_callback::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.done = true;
        }
    }
}

impl Dispatch<WlOutput, usize> for WaylandState {
    fn event(
        state: &mut Self,
        _: &WlOutput,
        event: wl_output::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Geometry {
                transform: WEnum::Value(transform),
                ..
            } => state.on_output_transform(*index, transform.into()),
            wl_output::Event::Mode {
                flags,
                width,
                height,
                ..
            } => {
                let current = matches!(flags, WEnum::Value(flags) if flags.contains(Mode::Current));
                state.on_output_mode(*index, current, width, height);
            }
            wl_output::Event::Scale { factor } => state.on_output_scale(*index, factor),
            wl_output::Event::Name { name } => state.on_output_name(*index, name),
            _ => {}
        }
    }
}

impl Dispatch<ZxdgOutputManagerV1, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &ZxdgOutputManagerV1,
        _: zxdg_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZxdgOutputV1, usize> for WaylandState {
    fn event(
        state: &mut Self,
        _: &ZxdgOutputV1,
        event: zxdg_output_v1::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zxdg_output_v1::Event::LogicalPosition { x, y } => {
                state.on_logical_position(*index, x, y);
            }
            zxdg_output_v1::Event::LogicalSize { width, height } => {
                state.on_logical_size(*index, width, height);
            }
            zxdg_output_v1::Event::Name { name } => state.on_logical_name(*index, name),
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "display_wayland_state_tests.rs"]
mod tests;
