use std::ops::Range;

use agent_desktop_core::{AdapterError, Deadline, NativeHandle};
use zbus::Connection;
use zbus::export::serde::Serialize;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedValue};

use super::action_read::read_node_view;
use super::text_units::code_point_to_utf16;
use crate::system::{PROPERTIES_INTERFACE, SharedBus, within};
use crate::tree::AtspiElement;
use crate::tree::properties::absence::gone_error;
use crate::tree::properties::liveness::Liveness;
use crate::tree::properties::node_read::call;

const TEXT_INTERFACE: &str = "org.a11y.atspi.Text";
const MAX_TEXT_CHARACTERS: usize = 64 * 1024;
const MAX_TEXT_BYTES: usize = 64 * 1024;
const TEXT_CHUNK_CHARACTERS: usize = 1024;

/// The caret and the selections of a text node, in code points.
pub(crate) type CaretAndSelection = (usize, Vec<Range<usize>>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TextState {
    pub(crate) text: String,
    pub(crate) caret: usize,
    pub(crate) selections: Vec<Range<usize>>,
}

/// What the selection read needs from a text node. `Ok(None)` is a read that
/// failed on its own field; `Err` is the node or its application gone, or the
/// deadline spent.
pub(crate) trait TextSource {
    fn caret(&mut self) -> Result<Option<usize>, AdapterError>;
    fn selections(&mut self) -> Result<Option<Vec<Range<usize>>>, AdapterError>;
    fn character_count(&mut self) -> Result<Option<usize>, AdapterError>;
    fn text(&mut self, start: usize, end: usize) -> Result<Option<String>, AdapterError>;
}

fn ordered(range: Range<usize>) -> Range<usize> {
    range.start.min(range.end)..range.start.max(range.end)
}

/// The selection as UTF-16 offsets over the text: the first selection, or the
/// caret as an empty range when nothing is selected (GTK4 reports no
/// selection for a bare caret). Unavailable when the text is longer than the
/// read cap, since an offset cannot be converted without all the text before
/// it, or when any part cannot be read or converted.
pub(crate) fn selection_utf16<S: TextSource>(
    source: &mut S,
) -> Result<Option<Range<usize>>, AdapterError> {
    let Some((caret, selections)) = caret_and_selection(source)? else {
        return Ok(None);
    };
    let range = selections.into_iter().next().map_or(caret..caret, ordered);
    let Some(count) = source.character_count()? else {
        return Ok(None);
    };
    if count > MAX_TEXT_CHARACTERS {
        return Ok(None);
    }
    let Some(text) = source.text(0, count)? else {
        return Ok(None);
    };
    let start = code_point_to_utf16(&text, range.start);
    let end = code_point_to_utf16(&text, range.end);
    Ok(start.zip(end).map(|(start, end)| start..end))
}

fn caret_and_selection<S: TextSource>(
    source: &mut S,
) -> Result<Option<CaretAndSelection>, AdapterError> {
    let Some(caret) = source.caret()? else {
        return Ok(None);
    };
    Ok(source.selections()?.map(|selections| (caret, selections)))
}

struct BusText<'a> {
    connection: &'a Connection,
    element: &'a AtspiElement,
    deadline: Deadline,
}

fn settle<T>(outcome: Result<T, zbus::Error>) -> Result<Option<T>, AdapterError> {
    match outcome {
        Ok(value) => Ok(Some(value)),
        Err(error) => match gone_error(&error, Liveness::Alive) {
            Some(gone) => Err(gone),
            None => Ok(None),
        },
    }
}

impl BusText<'_> {
    fn property(&self, name: &'static str) -> Result<Option<OwnedValue>, AdapterError> {
        let read = async {
            Ok(call(
                self.connection,
                self.element,
                PROPERTIES_INTERFACE,
                "Get",
                &(TEXT_INTERFACE, name),
            )
            .await
            .and_then(|reply| reply.body().deserialize::<OwnedValue>()))
        };
        within(self.deadline, read).and_then(settle)
    }

    fn integer(&self, name: &'static str) -> Result<Option<i32>, AdapterError> {
        Ok(self
            .property(name)?
            .and_then(|value| value.downcast_ref::<i32>().ok()))
    }

    fn call_text<T>(
        &self,
        method: &'static str,
        body: impl Serialize + DynamicType,
    ) -> Result<Option<T>, AdapterError>
    where
        T: for<'de> DynamicDeserialize<'de>,
    {
        let reply = async {
            Ok(
                call(self.connection, self.element, TEXT_INTERFACE, method, &body)
                    .await
                    .and_then(|reply| reply.body().deserialize::<T>()),
            )
        };
        within(self.deadline, reply).and_then(settle)
    }
}

impl TextSource for BusText<'_> {
    fn caret(&mut self) -> Result<Option<usize>, AdapterError> {
        Ok(self
            .integer("CaretOffset")?
            .and_then(|offset| usize::try_from(offset).ok()))
    }

    fn selections(&mut self) -> Result<Option<Vec<Range<usize>>>, AdapterError> {
        let Some(count) = self.call_text::<i32>("GetNSelections", ())? else {
            return Ok(None);
        };
        if count < 0 {
            return Ok(None);
        }
        let mut found = Vec::new();
        for index in 0..count {
            let bounds = self.call_text::<(i32, i32)>("GetSelection", (index,))?;
            let Some((start, end)) = bounds else {
                return Ok(None);
            };
            let (Ok(start), Ok(end)) = (usize::try_from(start), usize::try_from(end)) else {
                return Ok(None);
            };
            found.push(start..end);
        }
        Ok(Some(found))
    }

    fn character_count(&mut self) -> Result<Option<usize>, AdapterError> {
        Ok(self
            .integer("CharacterCount")?
            .and_then(|count| usize::try_from(count).ok()))
    }

    fn text(&mut self, start: usize, end: usize) -> Result<Option<String>, AdapterError> {
        let (Ok(start), Ok(end)) = (i32::try_from(start), i32::try_from(end)) else {
            return Ok(None);
        };
        self.call_text::<String>("GetText", (start, end))
    }
}

fn complete_text<S: TextSource>(
    source: &mut S,
    characters: usize,
) -> Result<Option<String>, AdapterError> {
    if characters > MAX_TEXT_BYTES {
        return Err(text_too_large());
    }
    let mut text = String::new();
    let mut offset = 0;
    while offset < characters {
        let end = (offset + TEXT_CHUNK_CHARACTERS).min(characters);
        let Some(part) = source.text(offset, end)? else {
            return Ok(None);
        };
        if part.chars().count() != end - offset {
            return Ok(None);
        }
        if text.len() + part.len() > MAX_TEXT_BYTES {
            return Err(text_too_large());
        }
        text.push_str(&part);
        offset = end;
    }
    Ok(Some(text))
}

fn text_too_large() -> AdapterError {
    AdapterError::new(
        agent_desktop_core::ErrorCode::ActionNotSupported,
        "Text exceeds the 65,536-byte selection limit",
    )
    .with_disposition(agent_desktop_core::DeliverySemantics::not_delivered())
}

fn checked_state<S: TextSource>(source: &mut S) -> Result<Option<TextState>, AdapterError> {
    let Some(characters) = source.character_count()? else {
        return Ok(None);
    };
    let Some((caret, mut selections)) = caret_and_selection(source)? else {
        return Ok(None);
    };
    if caret > characters {
        return Ok(None);
    }
    for range in &mut selections {
        let ordered = ordered(range.clone());
        if ordered.end > characters {
            return Ok(None);
        }
        *range = ordered;
    }
    let Some(text) = complete_text(source, characters)? else {
        return Ok(None);
    };
    if text.chars().count() != characters {
        return Ok(None);
    }
    Ok(Some(TextState {
        text,
        caret,
        selections,
    }))
}

/// Reads complete text and the full selection list after the caller has
/// checked the node's secure state and Text interface.
pub(crate) fn read_text_state(
    bus: &SharedBus,
    element: &AtspiElement,
    deadline: Deadline,
) -> Result<Option<TextState>, AdapterError> {
    let connection = bus.get(deadline)?.inner().clone();
    checked_state(&mut BusText {
        connection: &connection,
        element,
        deadline,
    })
}

/// Whether a node may have its text read at all: it names the `Text`
/// interface and is not a secure field.
fn readable(secure: bool, interfaces: Option<&[String]>) -> bool {
    !secure && interfaces.is_some_and(|names| names.iter().any(|name| name == TEXT_INTERFACE))
}

/// The selection of a text node as UTF-16 offsets, or `None` when it is
/// unavailable. A node that is not a text node, and a secure field, are
/// unavailable without any text read.
pub(crate) fn read_selection(
    bus: &SharedBus,
    handle: &NativeHandle,
    deadline: Deadline,
) -> Result<Option<Range<usize>>, AdapterError> {
    let view = read_node_view(bus, handle, deadline)?;
    let element = crate::tree::atspi_element(handle)?;
    let connection = bus.get(deadline)?.inner().clone();
    let mut source = BusText {
        connection: &connection,
        element,
        deadline,
    };
    selection_when_readable(view.secure(), view.interfaces(), &mut source)
}

fn selection_when_readable<S: TextSource>(
    secure: bool,
    interfaces: Option<&[String]>,
    source: &mut S,
) -> Result<Option<Range<usize>>, AdapterError> {
    if !readable(secure, interfaces) {
        return Ok(None);
    }
    selection_utf16(source)
}

/// The caret and every selection of a text node in code points, the unit
/// `InsertText` takes, or `None` when either could not be read. It reads no
/// text, so the caller decides whether a secure field may be read at all.
pub(crate) fn read_caret_and_selection(
    bus: &SharedBus,
    element: &AtspiElement,
    deadline: Deadline,
) -> Result<Option<CaretAndSelection>, AdapterError> {
    let connection = bus.get(deadline)?.inner().clone();
    let supported = within(deadline, async {
        Ok(crate::tree::properties::interface_support::supports(
            &connection,
            element,
            TEXT_INTERFACE,
        )
        .await)
    })
    .and_then(settle)?;
    if supported != Some(true) {
        return Ok(None);
    }
    caret_and_selection(&mut BusText {
        connection: &connection,
        element,
        deadline,
    })
}

#[cfg(test)]
#[path = "text_selection_tests.rs"]
mod tests;
