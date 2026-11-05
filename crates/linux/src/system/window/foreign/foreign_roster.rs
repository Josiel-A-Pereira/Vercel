use super::foreign_record::ToplevelRecord;
use super::foreign_tracked::Tracked;

/// The toplevel handles the compositor announced. Each gets a serial that is
/// never handed out twice on the connection, so a protocol id the compositor
/// reuses for a new toplevel can never reach a closed one.
pub(super) struct Roster<H> {
    entries: Vec<Tracked<H>>,
    next: u32,
}

impl<H> Default for Roster<H> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next: 0,
        }
    }
}

impl<H: PartialEq> Roster<H> {
    pub(super) fn announce(&mut self, handle: H) -> u32 {
        let serial = self.next;
        self.next = self.next.wrapping_add(1);
        self.entries.push(Tracked {
            serial,
            handle,
            shown: None,
            pending: Default::default(),
        });
        serial
    }

    pub(super) fn serial_of(&self, handle: &H) -> Option<u32> {
        self.entries
            .iter()
            .find(|entry| &entry.handle == handle)
            .map(|entry| entry.serial)
    }

    pub(super) fn records(&self) -> Vec<ToplevelRecord> {
        self.entries
            .iter()
            .filter_map(|entry| entry.shown.clone())
            .collect()
    }

    /// The handle behind a serial, once its first `done` arrived.
    pub(super) fn handle_of(&self, serial: u32) -> Option<&H> {
        self.entries
            .iter()
            .find(|entry| entry.serial == serial && entry.shown.is_some())
            .map(|entry| &entry.handle)
    }

    pub(super) fn shown_handles(&self) -> Vec<(u32, &H)> {
        self.entries
            .iter()
            .filter(|entry| entry.shown.is_some())
            .map(|entry| (entry.serial, &entry.handle))
            .collect()
    }

    pub(super) fn tracked_mut(&mut self, serial: u32) -> Option<&mut Tracked<H>> {
        self.entries.iter_mut().find(|entry| entry.serial == serial)
    }

    pub(super) fn settle(&mut self, serial: u32) {
        if let Some(entry) = self.tracked_mut(serial) {
            let record = entry.pending.settle(serial, entry.shown.as_ref());
            entry.shown = Some(record);
        }
    }

    /// Drops the entry and returns its handle so the caller can destroy it.
    pub(super) fn forget(&mut self, serial: u32) -> Option<H> {
        let at = self
            .entries
            .iter()
            .position(|entry| entry.serial == serial)?;
        Some(self.entries.remove(at).handle)
    }
}

#[cfg(test)]
#[path = "foreign_roster_tests.rs"]
mod tests;
