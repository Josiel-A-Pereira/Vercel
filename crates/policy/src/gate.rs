//! The gate: the user's policy files over a durable Dogwood store under the
//! state root, deciding one request event per gated command and recording
//! the outcome as history. Several processes share one store; see
//! [`crate::store`] for how they hand it to each other.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use agent_desktop_core::Deadline;
use dogwood_language::Value;
use dogwood_local_engine::{DurableTemporalEngine, Outcome};

use crate::catalog;
use crate::discover::{PolicySources, engine_state_dir_for};
use crate::store::{OpenStore, StoreSlot, hand_over_on_request, store_wait_error};
use crate::{CommandFacts, PolicyError, Verdict};

const FINGERPRINT_FILE: &str = "installed.fingerprint";
const STORE_WAIT_MS: u64 = 3_000;

#[derive(Debug)]
pub struct PolicyGate {
    state_dir: PathBuf,
    text: String,
    content_stamp: String,
    /// Identity of the loaded source files (path, mtime, len), so a cached
    /// gate notices edits between the entries of a `batch` and reloads.
    source_stamp: u64,
    files: Vec<PathBuf>,
    store: StoreSlot,
}

impl PolicyGate {
    /// Load the policy sources and make sure their text is the installed
    /// set of their store. Installing only on content change keeps the log
    /// quiet and preserves the temporal history of unchanged policies.
    pub fn open(sources: &PolicySources) -> Result<Self, PolicyError> {
        Self::open_at(sources, &engine_state_dir_for(sources.origin)?)
    }

    /// [`open`](Self::open) under an explicit state root instead of the
    /// state root's; the seam tests isolate their stores through. Each
    /// layer location (a folder or a single file) keeps its own store
    /// beneath it, so processes running under different layers never
    /// reinstall over each other's history, and a file added to a folder
    /// joins that folder's history.
    pub fn open_at(sources: &PolicySources, state_root: &Path) -> Result<Self, PolicyError> {
        create_private_dir(state_root)?;
        let state_dir = state_root.join(hash_of(sources.root.to_string_lossy().as_bytes()));
        create_private_dir(&state_dir)?;

        let text = concatenated_source(&sources.files)?;
        let store: StoreSlot = Arc::new(Mutex::new(None));
        hand_over_on_request(Arc::downgrade(&store), &state_dir);
        let gate = Self {
            state_dir,
            content_stamp: installed_set_stamp(&text),
            text,
            source_stamp: source_stamp(&sources.files)?,
            files: sources.files.clone(),
            store,
        };
        gate.with_store(command_deadline()?, |_| Ok(()))?;
        Ok(gate)
    }

    /// Whether the sources this gate was built from are unchanged on disk.
    pub fn is_current(&self, sources: &PolicySources) -> bool {
        sources.files == self.files
            && source_stamp(&sources.files).map(|stamp| stamp == self.source_stamp) == Ok(true)
    }

    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Decide a command. The request event is appended to the history
    /// whatever the verdict, so temporal clauses can also see denied tries.
    pub fn check(&mut self, facts: &CommandFacts) -> Result<Verdict, PolicyError> {
        let submitted = self.with_store(command_deadline()?, |engine| {
            engine
                .submit(request_event(facts))
                .map_err(|error| PolicyError::Engine {
                    message: error.to_string(),
                })
        })?;
        match submitted.outcome {
            Outcome::Decision(decision) => {
                if decision.allowed() {
                    Ok(Verdict::Allowed)
                } else {
                    let diagnostics = decision.diagnostics();
                    let policies = diagnostics
                        .reason()
                        .map(|attribution| {
                            attribution
                                .annotation("id")
                                .unwrap_or("<unannotated policy>")
                                .to_string()
                        })
                        .collect();
                    let errors = diagnostics.errors().map(str::to_string).collect();
                    Ok(Verdict::Denied { policies, errors })
                }
            }
            Outcome::Recorded => Err(PolicyError::Engine {
                message: "request event returned no decision".to_string(),
            }),
        }
    }

    /// Record how a dispatched command ended. History only; a recording
    /// failure must not fail the command that already ran, so the caller
    /// receives the error merely to log it. The command's own deadline may
    /// have passed by now, so the wait for the store is bounded separately.
    pub fn record(&mut self, facts: &CommandFacts, ok: bool) -> Result<(), PolicyError> {
        let kind = if ok { "response" } else { "error" };
        let mut event = base_event(facts, kind);
        if ok {
            event = event.field("output", "ok", Value::Bool(true));
        }
        let deadline = Deadline::detached_after(STORE_WAIT_MS).map_err(store_wait_error)?;
        self.with_store(deadline, |engine| {
            engine
                .submit(event)
                .map(|_| ())
                .map_err(|error| PolicyError::Engine {
                    message: error.to_string(),
                })
        })
    }

    /// Run one step against the store, holding it exclusively across
    /// processes for the duration. A store this process already holds is
    /// reused; otherwise it is taken over and this gate's policy text made
    /// the installed set before the step runs.
    fn with_store<T>(
        &self,
        deadline: Deadline,
        step: impl FnOnce(&mut DurableTemporalEngine) -> Result<T, PolicyError>,
    ) -> Result<T, PolicyError> {
        let mut held = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let open = match held.take() {
            Some(open) => held.insert(open),
            None => {
                let mut open = OpenStore::acquire(&self.state_dir, deadline)?;
                self.ensure_installed(open.engine())?;
                held.insert(open)
            }
        };
        step(open.engine())
    }

    fn ensure_installed(&self, engine: &mut DurableTemporalEngine) -> Result<(), PolicyError> {
        let fingerprint_path = self.state_dir.join(FINGERPRINT_FILE);
        let installed = std::fs::read_to_string(&fingerprint_path).ok();
        if engine.has_policy() && installed.as_deref() == Some(self.content_stamp.as_str()) {
            return Ok(());
        }
        crate::install::sync_installed(engine, &self.text)?;
        std::fs::write(&fingerprint_path, &self.content_stamp).map_err(|error| PolicyError::State {
            message: format!("cannot record installed fingerprint: {error}"),
        })
    }
}

fn command_deadline() -> Result<Deadline, PolicyError> {
    Deadline::after(STORE_WAIT_MS).map_err(store_wait_error)
}

fn request_event(facts: &CommandFacts) -> dogwood_language::EventBuilder {
    let mut event = base_event(facts, "request");
    for (name, value) in facts.string_fields() {
        event = event.request_context("input", name, Value::String(value.to_string()));
    }
    event
        .request_context("input", "secure", Value::Bool(facts.target.secure))
        .request_context("input", "headed", Value::Bool(facts.headed))
        .request_context("input", "mutating", Value::Bool(facts.mutating))
}

/// The resource is `Desktop::App::"<name>"`, a member of
/// `Desktop::AppId::"<id>"` when the app's stable identifier is known, so
/// `resource in Desktop::AppId::"com.example.app"` holds whatever the app
/// is called. Bundle identifiers are case-insensitive, so the resource is
/// also a member of the identifier in lowercase.
fn base_event(facts: &CommandFacts, kind: &str) -> dogwood_language::EventBuilder {
    let mut event =
        dogwood_language::Event::builder_for(&["Desktop", "Action"], facts.command, kind)
            .principal_for("Desktop::Agent", &facts.agent)
            .resource_for("Desktop::App", facts.app_or_system());
    if !facts.target.app_id.is_empty() {
        let lowercase = facts.target.app_id.to_lowercase();
        let mut ids = vec![facts.target.app_id.as_str()];
        if lowercase != facts.target.app_id {
            ids.push(lowercase.as_str());
        }
        event = event.parents_for(
            "Desktop::App",
            facts.app_or_system(),
            ids.into_iter().map(|id| ("Desktop::AppId", id)),
        );
    }
    for (name, value) in facts.string_fields() {
        event = event.field("input", name, Value::String(value.to_string()));
    }
    event = event
        .field("input", "secure", Value::Bool(facts.target.secure))
        .field("input", "headed", Value::Bool(facts.headed))
        .field("input", "mutating", Value::Bool(facts.mutating));
    if !facts.session.is_empty() {
        event = event.logged_field("sessionId", Value::String(facts.session.clone()));
    }
    event
}

fn concatenated_source(files: &[PathBuf]) -> Result<String, PolicyError> {
    let mut text = String::new();
    for file in files {
        let body = std::fs::read_to_string(file).map_err(|error| PolicyError::Load {
            path: file.clone(),
            message: format!("cannot read policy file: {error}"),
        })?;
        text.push_str(&format!("// source: {}\n", file.display()));
        text.push_str(&body);
        text.push('\n');
    }
    Ok(text)
}

fn source_stamp(files: &[PathBuf]) -> Result<u64, PolicyError> {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    for file in files {
        let metadata = std::fs::metadata(file).map_err(|error| PolicyError::Load {
            path: file.clone(),
            message: format!("cannot stat policy file: {error}"),
        })?;
        file.hash(&mut hasher);
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            modified.hash(&mut hasher);
        }
    }
    Ok(hasher.finish())
}

/// The installed set is the schema plus the policy text: a binary whose
/// generated schema changed must reinstall even when the policy did not.
fn installed_set_stamp(policy_text: &str) -> String {
    hash_of(format!("{}\x00{policy_text}", catalog::schema_text()).as_bytes())
}

fn hash_of(bytes: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    bytes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn create_private_dir(path: &std::path::Path) -> Result<(), PolicyError> {
    std::fs::create_dir_all(path).map_err(|error| PolicyError::State {
        message: format!("cannot create {}: {error}", path.display()),
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(
            |error| PolicyError::State {
                message: format!("cannot restrict {}: {error}", path.display()),
            },
        )?;
    }
    Ok(())
}
