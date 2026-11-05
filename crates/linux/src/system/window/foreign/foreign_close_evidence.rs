use std::collections::HashMap;

use super::foreign_frame::Frame;
use super::foreign_names::ProcessNames;
use super::foreign_record::ToplevelRecord;

#[derive(Default)]
pub(super) struct CloseEvidence {
    pub(super) names: Option<ProcessNames>,
    pub(super) frames: Vec<Frame>,
    pub(super) known: Vec<u32>,
    pub(super) prompts: Vec<u32>,
    pub(super) unproven: Vec<String>,
    pub(super) dialogs: Vec<String>,
    pub(super) selected: HashMap<u32, (ToplevelRecord, Option<u64>)>,
}
