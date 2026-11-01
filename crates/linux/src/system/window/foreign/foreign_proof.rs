use super::foreign_names::ProcessNames;
use super::foreign_record::ToplevelRecord;

/// What the toplevel list proves about one window of one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) enum Proof {
    One(u32),
    Several(Vec<u32>),
    NoWindow,
    Unrelated(Vec<String>),
}

/// Picks the toplevel a title and a process name stand for. The compositor
/// names no process, so a toplevel counts only when its title matches and its
/// app id relates to the process; an unrelated window with the same title
/// never makes a related one ambiguous.
pub(in crate::system) fn prove(
    title: &str,
    names: &ProcessNames,
    records: &[ToplevelRecord],
) -> Proof {
    if title.is_empty() {
        return Proof::NoWindow;
    }
    let same_title: Vec<&ToplevelRecord> = records
        .iter()
        .filter(|record| record.title == title)
        .collect();
    if same_title.is_empty() {
        return Proof::NoWindow;
    }
    let related: Vec<u32> = same_title
        .iter()
        .filter(|record| names.relates(&record.app_id))
        .map(|record| record.id)
        .collect();
    match related.as_slice() {
        [] => Proof::Unrelated(
            same_title
                .iter()
                .map(|record| record.app_id.clone())
                .collect(),
        ),
        [only] => Proof::One(*only),
        _ => Proof::Several(related),
    }
}

#[cfg(test)]
#[path = "foreign_proof_tests.rs"]
mod tests;
