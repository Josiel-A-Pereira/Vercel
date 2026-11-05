use super::foreign_flags::ToplevelFlags;
use super::foreign_record::ToplevelRecord;

/// Handle events received since the last `done`; none of them is visible
/// until `done` arrives.
#[derive(Debug, Default)]
pub(super) struct Pending {
    pub(super) title: Option<String>,
    pub(super) app_id: Option<String>,
    pub(super) flags: Option<ToplevelFlags>,
}

impl Pending {
    /// Folds the held events over the previous record into the record the
    /// batch describes.
    pub(super) fn settle(&mut self, id: u32, previous: Option<&ToplevelRecord>) -> ToplevelRecord {
        let title = self
            .title
            .take()
            .or_else(|| previous.map(|record| record.title.clone()))
            .unwrap_or_default();
        let app_id = self
            .app_id
            .take()
            .or_else(|| previous.map(|record| record.app_id.clone()))
            .unwrap_or_default();
        let flags = self
            .flags
            .take()
            .or_else(|| previous.map(|record| record.flags))
            .unwrap_or_default();
        ToplevelRecord {
            id,
            title,
            app_id,
            flags,
        }
    }
}
