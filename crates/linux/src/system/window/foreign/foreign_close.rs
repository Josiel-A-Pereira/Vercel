use super::foreign_frame::Frame;
use super::foreign_names::ProcessNames;
use super::foreign_record::ToplevelRecord;
use crate::system::x11::window::toplevel::Toplevel;
use crate::system::x11::window::window_kind::WindowKind;

/// The toplevels a close flow may act on, and how many it could not prove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::system) struct ForeignStanding {
    pub(in crate::system) windows: Vec<Toplevel>,
    pub(in crate::system) contested: usize,
}

fn listed(record: &ToplevelRecord, kind: WindowKind) -> Toplevel {
    Toplevel {
        id: record.id,
        title: record.title.clone(),
        kind,
        transient: false,
    }
}

fn count_of(frames: &[Frame], title: &str, prompt: Option<bool>) -> usize {
    frames
        .iter()
        .filter(|frame| frame.title == title && prompt.is_none_or(|wanted| frame.prompt == wanted))
        .count()
}

fn is_late(record: &ToplevelRecord, known: &[u32]) -> bool {
    !known.is_empty() && !known.contains(&record.id)
}

/// Reads which of the compositor's toplevels belong to the process.
///
/// A related toplevel that appeared after the first close request, one whose
/// id `known` does not hold, is a prompt whatever its title, because a window
/// that was always there is in `known`. Of the others, a title the process
/// shows `f` times in its own frames and the compositor lists `r` times for
/// the process's names is proven only when `r <= f`; otherwise another
/// process may own some, so none of that title is listed and `r - f` are
/// counted as contested. A proven title that any frame presents as a prompt is
/// listed as a dialog, since the compositor cannot say which of its toplevels
/// is the prompt and none of them may be closed.
pub(in crate::system) fn read_standing(
    records: &[ToplevelRecord],
    frames: &[Frame],
    names: &ProcessNames,
    known: &[u32],
) -> ForeignStanding {
    let related: Vec<&ToplevelRecord> = records
        .iter()
        .filter(|record| names.relates(&record.app_id))
        .collect();
    let mut windows = Vec::new();
    let mut contested = 0;
    let mut judged: Vec<&str> = Vec::new();
    for record in &related {
        if is_late(record, known) {
            windows.push(listed(record, WindowKind::Dialog));
            continue;
        }
        let title = record.title.as_str();
        let framed = count_of(frames, title, None);
        if framed == 0 {
            continue;
        }
        let shown = related
            .iter()
            .filter(|other| other.title == title && !is_late(other, known))
            .count();
        if shown > framed {
            if !judged.contains(&title) {
                judged.push(title);
                contested += shown - framed;
            }
            continue;
        }
        let kind = if count_of(frames, title, Some(true)) > 0 {
            WindowKind::Dialog
        } else {
            WindowKind::Normal
        };
        windows.push(listed(record, kind));
    }
    ForeignStanding { windows, contested }
}

#[cfg(test)]
#[path = "foreign_close_tests.rs"]
mod tests;
