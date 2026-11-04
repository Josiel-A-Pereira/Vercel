use super::{FrameFacts, X11Client, X11Evidence, X11State};
use crate::tree::Extents;

/// Which X11 clients a listed window could be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClientMatch {
    One(u32),
    Several(Vec<u32>),
    NoClient,
}

fn frame_around(client: &X11Client) -> Option<Extents> {
    let bounds = client.bounds?;
    let top = client.extents.map_or(0, |extents| extents.top);
    let top = i32::try_from(top).unwrap_or(0);
    Some(Extents {
        x: bounds.x,
        y: bounds.y - top,
        width: bounds.width,
        height: bounds.height + top,
    })
}

fn without_shadow(client: &X11Client, frame: Extents) -> Extents {
    let (left, top, right, bottom) = client.shadow;
    Extents {
        x: frame.x + left,
        y: frame.y + top,
        width: frame.width - left - right,
        height: frame.height - top - bottom,
    }
}

fn sits_at(client: &X11Client, extents: Extents) -> bool {
    frame_around(client)
        .is_some_and(|frame| frame == extents || without_shadow(client, frame) == extents)
}

fn same_size(left: Extents, right: Extents) -> bool {
    left.width == right.width && left.height == right.height
}

fn unnamed_client(frame: &FrameFacts, clients: &[X11Client]) -> ClientMatch {
    let Some(size) = frame.extents.filter(|size| {
        size.width > 0 && size.height > 0 && size.x != i32::MIN && size.y != i32::MIN
    }) else {
        return ClientMatch::NoClient;
    };
    let matching: Vec<u32> = clients
        .iter()
        .filter(|client| client.pid == Some(frame.pid))
        .filter(|client| {
            frame_around(client).is_some_and(|outer| {
                same_size(size, outer) || same_size(size, without_shadow(client, outer))
            })
        })
        .map(|client| client.id)
        .collect();
    match matching.as_slice() {
        [] => ClientMatch::NoClient,
        [only] => ClientMatch::One(*only),
        _ => ClientMatch::Several(matching),
    }
}

fn titles_match(client_title: &str, frame_title: &str) -> bool {
    if client_title == frame_title {
        return true;
    }
    let ct = client_title.trim();
    let ft = frame_title.trim();
    if ct == ft {
        return true;
    }
    if ft.is_empty() {
        return false;
    }
    let client_doc = ct.split_once(" — ").map(|(doc, _)| doc.trim()).unwrap_or(ct);
    let frame_doc = ft.split_once(" — ").map(|(doc, _)| doc.trim()).unwrap_or(ft);
    if client_doc == frame_doc {
        return true;
    }
    let client_doc_dash = ct.split_once(" - ").map(|(doc, _)| doc.trim()).unwrap_or(ct);
    let frame_doc_dash = ft.split_once(" - ").map(|(doc, _)| doc.trim()).unwrap_or(ft);
    if client_doc_dash == frame_doc_dash {
        return true;
    }
    if ct.starts_with(ft) {
        let rest = ct[ft.len()..].trim_start();
        if rest.starts_with('—') || rest.starts_with('-') {
            return true;
        }
    }
    false
}

/// The client of a window, by process id and title first. A tie is broken by
/// the window's extents only when the toolkit reports them in screen
/// coordinates: the accessibility rectangle then equals the client rectangle
/// grown upward by the top frame extent. A tie that extents cannot break
/// stays a tie, and child order is never used. An unnamed frame with local
/// extents needs an exact, unique same-process size match, including the
/// toolkit's client-side shadow convention; a missing size identifies nothing.
pub(crate) fn match_client(frame: &FrameFacts, clients: &[X11Client]) -> ClientMatch {
    if frame.title.is_empty() && !frame.global_position {
        return if frame.size_identity {
            unnamed_client(frame, clients)
        } else {
            ClientMatch::NoClient
        };
    }
    let mut named: Vec<&X11Client> = clients
        .iter()
        .filter(|client| client.pid == Some(frame.pid) && client.title == frame.title)
        .collect();
    if named.is_empty() {
        named = clients
            .iter()
            .filter(|client| client.pid == Some(frame.pid) && titles_match(&client.title, &frame.title))
            .collect();
    }
    let [_, _, ..] = named.as_slice() else {
        return named
            .first()
            .map_or(ClientMatch::NoClient, |client| ClientMatch::One(client.id));
    };
    if let (true, Some(extents)) = (frame.global_position, frame.extents) {
        let placed: Vec<&&X11Client> = named
            .iter()
            .filter(|client| sits_at(client, extents))
            .collect();
        if let [only] = placed.as_slice() {
            return ClientMatch::One(only.id);
        }
    }
    ClientMatch::Several(named.iter().map(|client| client.id).collect())
}

/// What the window manager's answer says about the listed windows.
pub(crate) fn evidence(state: &X11State, frames: &[FrameFacts]) -> X11Evidence {
    let X11State::Ready { active, clients } = state else {
        return X11Evidence::Unavailable;
    };
    let Some(active) = active else {
        return X11Evidence::NoActive;
    };
    let mut exact = Vec::new();
    let mut possible = false;
    for (index, frame) in frames.iter().enumerate() {
        match match_client(frame, clients) {
            ClientMatch::One(id) if id == *active => exact.push(index),
            ClientMatch::Several(ids) if ids.contains(active) => possible = true,
            _ => {}
        }
    }
    match (exact.as_slice(), possible) {
        ([only], false) => X11Evidence::Frame(*only),
        ([], false) => X11Evidence::Unmapped,
        _ => X11Evidence::Ambiguous,
    }
}

#[cfg(test)]
#[path = "map_tests.rs"]
mod tests;
