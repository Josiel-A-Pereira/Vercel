use std::collections::HashMap;

use super::foreign_frame::Frame;
use super::foreign_names::ProcessNames;
use super::foreign_record::ToplevelRecord;
use crate::system::x11::window::toplevel::Toplevel;
use crate::system::x11::window::window_kind::WindowKind;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) struct ForeignStanding {
    pub(in crate::system) windows: Vec<Toplevel>,
    pub(in crate::system) contested: usize,
    pub(in crate::system) unproven: Vec<String>,
    pub(in crate::system) dialogs: Vec<String>,
}

fn listed(record: &ToplevelRecord, kind: WindowKind) -> Toplevel {
    Toplevel {
        id: record.id,
        title: record.title.clone(),
        kind,
        transient: false,
    }
}

pub(in crate::system) fn read_standing(
    records: &[ToplevelRecord],
    frames: &[Frame],
    names: &ProcessNames,
    known: &[u32],
    owners: Option<&HashMap<u32, u32>>,
) -> ForeignStanding {
    let mut windows = Vec::new();
    let mut contested = 0;
    let mut unproven = Vec::new();
    let mut judged = Vec::new();
    for frame in frames {
        if judged.contains(&frame.title) {
            continue;
        }
        judged.push(frame.title.clone());
        let same: Vec<_> = records
            .iter()
            .filter(|record| record.title == frame.title)
            .collect();
        let candidates: Vec<_> = match owners {
            Some(owners) => {
                if same.iter().any(|record| !owners.contains_key(&record.id)) {
                    unproven.extend(same.iter().map(|record| record.app_id.clone()));
                }
                same.into_iter()
                    .filter(|record| owners.get(&record.id) == Some(&names.pid()))
                    .collect()
            }
            None => {
                let related: Vec<_> = same
                    .iter()
                    .copied()
                    .filter(|record| names.relates(&record.app_id))
                    .collect();
                if related.len() > 1 {
                    contested += related.len();
                    Vec::new()
                } else if same.len() > 1 || (!same.is_empty() && related.is_empty()) {
                    unproven.extend(same.iter().map(|record| record.app_id.clone()));
                    Vec::new()
                } else {
                    related
                }
            }
        };
        let prompt = frames
            .iter()
            .any(|other| other.title == frame.title && other.prompt);
        for record in candidates {
            let late = !known.is_empty() && !known.contains(&record.id);
            windows.push(listed(
                record,
                if prompt || late {
                    WindowKind::Dialog
                } else {
                    WindowKind::Normal
                },
            ));
        }
    }
    if let Some(owners) = owners {
        if !known.is_empty() {
            for record in records.iter().filter(|record| {
                !known.contains(&record.id)
                    && !owners.contains_key(&record.id)
                    && names.relates(&record.app_id)
            }) {
                unproven.push(record.app_id.clone());
            }
        }
        for record in records
            .iter()
            .filter(|record| owners.get(&record.id) == Some(&names.pid()))
        {
            if !windows.iter().any(|window| window.id == record.id) {
                let late = !known.is_empty() && !known.contains(&record.id);
                windows.push(listed(
                    record,
                    if late {
                        WindowKind::Dialog
                    } else {
                        WindowKind::Normal
                    },
                ));
            }
        }
    } else if !known.is_empty() {
        for record in records.iter().filter(|record| !known.contains(&record.id)) {
            if names.relates(&record.app_id) && !windows.iter().any(|window| window.id == record.id)
            {
                unproven.push(record.app_id.clone());
            }
        }
    }
    let dialogs = frames
        .iter()
        .filter(|frame| {
            frame.prompt
                && !windows
                    .iter()
                    .any(|window| window.title == frame.title && window.kind == WindowKind::Dialog)
        })
        .map(|frame| frame.title.clone())
        .collect();
    ForeignStanding {
        windows,
        contested,
        unproven,
        dialogs,
    }
}

#[cfg(test)]
#[path = "foreign_close_tests.rs"]
mod tests;
