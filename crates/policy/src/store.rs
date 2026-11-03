//! One process's hold on a policy store, and the hand-over between
//! processes.
//!
//! The durable store takes a single-process lock, and several agent-desktop
//! processes share one store, one per command and session running at the
//! same time. Opening the store costs tens of milliseconds
//! (recovery plus reloading the installed policies), so a process keeps it
//! open across commands while nobody else needs it. A process that wants
//! the store keeps a `wanted` marker fresh while it waits for the
//! cross-process lock; the holder's watcher sees the marker and closes the
//! store between decisions, which releases the lock to the waiter.
//!
//! The engine checkpoints (and prunes the log behind the checkpoint) only
//! after enough events within one open engine, and a one-shot CLI process
//! appends two. So whoever closes the store checkpoints it once enough
//! events have gathered since the last checkpoint, counted across
//! processes; otherwise the log, and the replay every open pays, would
//! grow without bound.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use agent_desktop_core::{AdapterError, Deadline, FileLock};
use dogwood_local_engine::DurableTemporalEngine;

use crate::PolicyError;

const SNAPSHOT_INTERVAL: u64 = 64;
const STORE_FILE: &str = "history.redb";
const LOCK_FILE: &str = "store.lock";
const WANTED_FILE: &str = "store.wanted";
const CHECKPOINT_FILE: &str = "checkpoint.offset";
const LOCK_ATTEMPT: Duration = Duration::from_millis(25);
const WATCH_INTERVAL: Duration = Duration::from_millis(20);

pub(crate) type StoreSlot = Arc<Mutex<Option<OpenStore>>>;

pub(crate) struct OpenStore {
    engine: DurableTemporalEngine,
    checkpoint_file: PathBuf,
    checkpointed: u64,
    _exclusive: FileLock,
}

impl std::fmt::Debug for OpenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenStore").finish_non_exhaustive()
    }
}

impl OpenStore {
    /// Take the store exclusively, asking whichever process holds it to
    /// hand it over, then open it.
    pub(crate) fn acquire(state_dir: &Path, deadline: Deadline) -> Result<Self, PolicyError> {
        let wanted = state_dir.join(WANTED_FILE);
        let exclusive = loop {
            let _ = std::fs::write(&wanted, b"");
            match FileLock::acquire(
                &state_dir.join(LOCK_FILE),
                deadline.capped(LOCK_ATTEMPT),
                "the policy store",
            ) {
                Ok(lock) => break lock,
                Err(error) if is_contended(&error) && !deadline.is_expired() => continue,
                Err(error) => return Err(store_wait_error(error)),
            }
        };
        let _ = std::fs::remove_file(&wanted);
        let store = state_dir.join(STORE_FILE);
        let engine = DurableTemporalEngine::open(&store, SNAPSHOT_INTERVAL).map_err(|error| {
            PolicyError::Engine {
                message: format!("cannot open policy store {}: {error}", store.display()),
            }
        })?;
        let checkpoint_file = state_dir.join(CHECKPOINT_FILE);
        let checkpointed = std::fs::read_to_string(&checkpoint_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0);
        Ok(Self {
            engine,
            checkpoint_file,
            checkpointed,
            _exclusive: exclusive,
        })
    }

    pub(crate) fn engine(&mut self) -> &mut DurableTemporalEngine {
        &mut self.engine
    }
}

impl Drop for OpenStore {
    /// Best effort: a skipped checkpoint costs replay on the next open,
    /// never a verdict. The lock is released only after this runs.
    fn drop(&mut self) {
        let offset = self.engine.log_offset();
        if offset.saturating_sub(self.checkpointed) < SNAPSHOT_INTERVAL {
            return;
        }
        match self.engine.checkpoint() {
            Ok(up_to) => {
                let _ = std::fs::write(&self.checkpoint_file, up_to.to_string());
            }
            Err(error) => tracing::debug!(%error, "policy store checkpoint skipped"),
        }
    }
}

/// Close the store held in `slot` whenever another process asks for it and
/// no decision is in progress. Stops once the gate owning the slot is gone.
pub(crate) fn hand_over_on_request(slot: Weak<Mutex<Option<OpenStore>>>, state_dir: &Path) {
    let wanted: PathBuf = state_dir.join(WANTED_FILE);
    let spawned = std::thread::Builder::new()
        .name("policy-store-hand-over".to_string())
        .spawn(move || {
            loop {
                std::thread::sleep(WATCH_INTERVAL);
                let Some(slot) = slot.upgrade() else {
                    return;
                };
                if wanted.exists() {
                    if let Ok(mut held) = slot.try_lock() {
                        held.take();
                    }
                }
            }
        });
    if let Err(error) = spawned {
        tracing::warn!(%error, "policy store hand-over watcher did not start");
    }
}

pub(crate) fn store_wait_error(error: AdapterError) -> PolicyError {
    PolicyError::Engine {
        message: format!("policy store unavailable: {}", error.message),
    }
}

fn is_contended(error: &AdapterError) -> bool {
    error
        .details
        .as_ref()
        .and_then(|details| details.get("kind"))
        .and_then(serde_json::Value::as_str)
        == Some("lock_timeout")
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
