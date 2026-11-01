use std::collections::HashMap;

use wayland_client::{Connection, Dispatch, QueueHandle};

use super::foreign_hypr_protocol::hyprland_toplevel_mapping_manager_v1::{
    self, HyprlandToplevelMappingManagerV1,
};
use super::foreign_hypr_protocol::hyprland_toplevel_window_mapping_handle_v1::{
    self, HyprlandToplevelWindowMappingHandleV1,
};
use super::foreign_state::ForeignState;

impl Dispatch<HyprlandToplevelMappingManagerV1, ()> for ForeignState {
    fn event(
        _: &mut Self,
        _: &HyprlandToplevelMappingManagerV1,
        _: hyprland_toplevel_mapping_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<HyprlandToplevelWindowMappingHandleV1, u32> for ForeignState {
    fn event(
        state: &mut Self,
        handle: &HyprlandToplevelWindowMappingHandleV1,
        event: hyprland_toplevel_window_mapping_handle_v1::Event,
        id: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let hyprland_toplevel_window_mapping_handle_v1::Event::WindowAddress {
            address_hi,
            address,
        } = event
        {
            state
                .addresses
                .insert(*id, (u64::from(address_hi) << 32) | u64::from(address));
        }
        handle.destroy();
    }
}

pub(super) fn stable_clients(
    before: &HashMap<u64, u32>,
    after: &HashMap<u64, u32>,
) -> HashMap<u64, u32> {
    before
        .iter()
        .filter(|(address, pid)| after.get(address) == Some(pid))
        .map(|(address, pid)| (*address, *pid))
        .collect()
}

pub(super) fn join(
    addresses: &HashMap<u32, u64>,
    clients: &HashMap<u64, u32>,
) -> HashMap<u32, u32> {
    let mut counts = HashMap::new();
    for address in addresses.values() {
        *counts.entry(*address).or_insert(0_usize) += 1;
    }
    addresses
        .iter()
        .filter_map(|(id, address)| {
            (counts.get(address) == Some(&1))
                .then(|| clients.get(address).map(|pid| (*id, *pid)))
                .flatten()
        })
        .collect()
}

#[cfg(test)]
#[path = "foreign_hypr_mapping_tests.rs"]
mod tests;
