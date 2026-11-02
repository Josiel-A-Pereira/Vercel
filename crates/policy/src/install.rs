//! Bringing a store's installed policies in line with the policy text.
//!
//! The engine's `install` replaces the whole set and every policy is born
//! again with no history, so editing a comment would forget every temporal
//! clause's observations. After the first install, the gate instead applies
//! only the difference: a policy whose canonical statement is unchanged
//! keeps its history, removed ones are deleted, new ones are added.

use dogwood_language::{ParsedPolicySet, ServiceSchema};
use dogwood_local_engine::{DurableTemporalEngine, Verb};

use crate::{PolicyError, catalog};

pub(crate) fn sync_installed(
    engine: &mut DurableTemporalEngine,
    text: &str,
) -> Result<(), PolicyError> {
    if !engine.has_policy() {
        return engine
            .install(text, catalog::schema_text(), None, None)
            .map(|_| ())
            .map_err(rejected);
    }
    let wanted = canonical_statements(text)?;
    let mut installed = engine.list();
    let mut verbs = Vec::new();
    if engine.action_schema().as_deref() != Some(catalog::schema_text()) {
        verbs.push(Verb::SetActionSchema {
            action_schema: catalog::schema_text().to_string(),
        });
    }
    for statement in wanted {
        match installed
            .iter()
            .position(|entry| entry.statement == statement)
        {
            Some(index) => {
                installed.swap_remove(index);
            }
            None => verbs.push(Verb::Add { policy: statement }),
        }
    }
    verbs.extend(
        installed
            .into_iter()
            .map(|entry| Verb::Delete { id: entry.token }),
    );
    engine.batch(verbs).map(|_| ()).map_err(rejected)
}

fn canonical_statements(text: &str) -> Result<Vec<String>, PolicyError> {
    let service = ServiceSchema::builder().build().map_err(rejected)?;
    let parsed = ParsedPolicySet::parse(text, &service).map_err(rejected)?;
    Ok(parsed
        .policies()
        .map(|policy| policy.expanded_source())
        .collect())
}

fn rejected(error: impl std::fmt::Display) -> PolicyError {
    PolicyError::Install {
        message: error.to_string(),
    }
}
