use super::{SessionLivenessLease, acquire_liveness_lease};
use crate::{AppError, refs::refs_lock::RefStoreLock, refs::refs_store::RefStore};

pub(super) struct ManifestMutationGuard {
    lock: Option<RefStoreLock>,
    _lease: Option<SessionLivenessLease>,
}

impl ManifestMutationGuard {
    pub(super) fn acquire(session_id: &str) -> Result<Self, AppError> {
        let lease = acquire_liveness_lease(session_id)?;
        let lock = if lease.is_some() {
            let store = RefStore::for_session(Some(session_id))?;
            Some(RefStoreLock::acquire(
                &store.base_dir().join("refstore.lock"),
            )?)
        } else {
            None
        };
        Ok(Self {
            lock,
            _lease: lease,
        })
    }

    pub(super) fn release_lock(&mut self) {
        drop(self.lock.take());
    }
}
