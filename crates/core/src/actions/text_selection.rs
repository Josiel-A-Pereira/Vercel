use serde::{Deserialize, Serialize};

use crate::{AdapterError, DeliverySemantics, ErrorCode, ResolvedTextSelection};

const MAX_TEXT_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextSelection {
    Match { text: String, nth: Option<usize> },
    Range { start: usize, end: usize },
}

impl TextSelection {
    pub fn validate(&self) -> Result<(), AdapterError> {
        match self {
            Self::Match { text, .. } if text.is_empty() => Err(invalid("Match must not be empty")),
            Self::Match { text, .. } if text.len() > MAX_TEXT_BYTES => {
                Err(invalid("Match exceeds the text size limit"))
            }
            Self::Match { nth: Some(0), .. } => Err(invalid("Match occurrence starts at 1")),
            Self::Range { start, end } if start > end => {
                Err(invalid("Range start must not exceed its end"))
            }
            _ => Ok(()),
        }
    }

    pub fn resolve(&self, text: &str) -> Result<ResolvedTextSelection, AdapterError> {
        self.validate()?;
        if text.len() > MAX_TEXT_BYTES {
            return Err(AdapterError::new(
                ErrorCode::ActionNotSupported,
                "Text exceeds the 65,536-byte selection limit",
            )
            .with_disposition(DeliverySemantics::not_delivered()));
        }
        let (start, end) = match self {
            Self::Range { start, end } => {
                if *end > text.chars().count() {
                    return Err(invalid("Range exceeds the text length"));
                }
                (*start, *end)
            }
            Self::Match { text: needle, nth } => {
                let mut matches = text
                    .char_indices()
                    .enumerate()
                    .filter(|(_, (byte, _))| text[*byte..].starts_with(needle));
                let occurrence = nth.unwrap_or(1);
                let start = matches
                    .nth(occurrence - 1)
                    .map(|(index, _)| index)
                    .ok_or_else(|| {
                        AdapterError::new(ErrorCode::ElementNotFound, "Text match not found")
                            .with_disposition(DeliverySemantics::not_delivered())
                    })?;
                if nth.is_none() && matches.next().is_some() {
                    return Err(AdapterError::new(
                        ErrorCode::AmbiguousTarget,
                        "Text match occurs more than once",
                    )
                    .with_suggestion("Specify --nth to select an occurrence")
                    .with_disposition(DeliverySemantics::not_delivered()));
                }
                (start, start + needle.chars().count())
            }
        };
        let byte_start = byte_offset(text, start);
        let byte_end = byte_offset(text, end);
        Ok(ResolvedTextSelection {
            range: start..end,
            utf16_range: text[..byte_start].encode_utf16().count()
                ..text[..byte_end].encode_utf16().count(),
            text: text[byte_start..byte_end].to_owned(),
        })
    }
}

fn byte_offset(text: &str, scalar: usize) -> usize {
    text.char_indices()
        .nth(scalar)
        .map_or(text.len(), |(offset, _)| offset)
}

fn invalid(message: &'static str) -> AdapterError {
    AdapterError::new(ErrorCode::InvalidArgs, message)
        .with_disposition(DeliverySemantics::not_delivered())
}

#[cfg(test)]
#[path = "text_selection_tests.rs"]
mod tests;
