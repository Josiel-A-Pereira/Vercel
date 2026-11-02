use super::*;
use crate::refs::refs_test_support::HomeGuard;
use crate::storage::private_file_ops::{PrivateFileOps, with_test_ops_override};
use std::fs::File;
use std::io;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, mpsc};

const COORDINATION_LIMIT: Duration = Duration::from_secs(5);

fn coordination_error(error: impl std::fmt::Display) -> io::Error {
    io::Error::other(error.to_string())
}

struct PausedOverlayWrite {
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl PrivateFileOps for PausedOverlayWrite {
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        if path
            .file_name()
            .is_some_and(|name| name == SESSION_MANIFEST_FILE)
        {
            self.ready.send(()).map_err(coordination_error)?;
            self.release
                .lock()
                .unwrap()
                .recv_timeout(COORDINATION_LIMIT)
                .map_err(coordination_error)?;
        }
        crate::storage::private_file::write_atomic_portable(path, bytes)
    }
}

struct ObservedEndWrite {
    probed: AtomicBool,
    contended: mpsc::Sender<bool>,
    committed: mpsc::Sender<()>,
}

impl PrivateFileOps for ObservedEndWrite {
    fn open_private_lock(&self, path: &Path, create: bool) -> io::Result<File> {
        let file = crate::storage::private_file::open_private_lock_portable(path, create)?;
        if path.file_name().is_some_and(|name| name == "refstore.lock")
            && !self.probed.swap(true, Ordering::Relaxed)
        {
            let contended = match file.try_lock() {
                Ok(()) => {
                    file.unlock()?;
                    false
                }
                Err(std::fs::TryLockError::WouldBlock) => true,
                Err(std::fs::TryLockError::Error(error)) => return Err(error),
            };
            self.contended.send(contended).map_err(coordination_error)?;
        }
        Ok(file)
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        crate::storage::private_file::write_atomic_portable(path, bytes)?;
        if path
            .file_name()
            .is_some_and(|name| name == SESSION_MANIFEST_FILE)
        {
            self.committed.send(()).map_err(coordination_error)?;
        }
        Ok(())
    }
}

struct Workers {
    release: mpsc::Sender<()>,
    overlay: Option<std::thread::JoinHandle<Result<SessionManifest, AppError>>>,
    end: Option<std::thread::JoinHandle<Result<SessionManifest, AppError>>>,
}

impl Drop for Workers {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(worker) = self.overlay.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.end.take() {
            let _ = worker.join();
        }
    }
}

#[test]
fn overlay_update_cannot_restore_an_ended_session_manifest() {
    let home = HomeGuard::new();
    let session = start_session(StartSessionOptions::default()).unwrap();
    let overlay = CursorOverlayConfig::enabled(Some("Working".into()), 6).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let overlay_home = home.path().to_path_buf();
    let overlay_session = session.id.clone();
    let expected_overlay = overlay.clone();
    let overlay_thread = std::thread::spawn(move || {
        crate::refs::set_home_override(Some(overlay_home));
        with_test_ops_override(
            Rc::new(PausedOverlayWrite {
                ready: ready_tx,
                release: Mutex::new(release_rx),
            }),
            || set_cursor_overlay(&overlay_session, overlay),
        )
    });
    let mut workers = Workers {
        release: release_tx,
        overlay: Some(overlay_thread),
        end: None,
    };
    ready_rx.recv_timeout(COORDINATION_LIMIT).unwrap();

    let (contended_tx, contended_rx) = mpsc::channel();
    let (committed_tx, committed_rx) = mpsc::channel();
    let end_home = home.path().to_path_buf();
    let end_session_id = session.id.clone();
    let end_thread = std::thread::spawn(move || {
        crate::refs::set_home_override(Some(end_home));
        with_test_ops_override(
            Rc::new(ObservedEndWrite {
                probed: AtomicBool::new(false),
                contended: contended_tx,
                committed: committed_tx,
            }),
            || end_session(&end_session_id),
        )
    });

    workers.end = Some(end_thread);

    let contended = contended_rx.recv_timeout(COORDINATION_LIMIT).unwrap();
    if !contended {
        committed_rx.recv_timeout(COORDINATION_LIMIT).unwrap();
    }
    workers.release.send(()).unwrap();
    let overlay_result = workers.overlay.take().unwrap().join();
    let end_result = workers.end.take().unwrap().join();
    overlay_result.unwrap().unwrap();
    let ended = end_result.unwrap().unwrap();
    assert!(ended.ended_at.is_some());

    let stored = read_manifest(&session.id).unwrap().unwrap();
    assert!(
        stored.ended_at.is_some(),
        "an overlay write must not reopen the session"
    );
    assert_eq!(stored.cursor_overlay, expected_overlay);
    assert!(!trace_enabled_for_session(&session.id).unwrap());
}

#[test]
fn manifest_mutations_do_not_create_unknown_session_directories() {
    let _home = HomeGuard::new();
    let id = "unknown-session";
    let directory = session_dir(id).unwrap();
    let overlay = CursorOverlayConfig::enabled(Some("Working".into()), 6).unwrap();
    assert!(set_cursor_overlay(id, overlay).is_err());
    assert!(!directory.exists());
    assert!(end_session(id).is_err());
    assert!(!directory.exists());
}

#[test]
fn ended_manifest_rejects_overlay_changes_and_end_is_idempotent() {
    let _home = HomeGuard::new();
    let session = start_session(StartSessionOptions::default()).unwrap();
    let ended = end_session(&session.id).unwrap();
    let bytes = std::fs::read(
        session_dir(&session.id)
            .unwrap()
            .join(SESSION_MANIFEST_FILE),
    )
    .unwrap();
    let overlay = CursorOverlayConfig::enabled(Some("Working".into()), 6).unwrap();
    assert!(set_cursor_overlay(&session.id, overlay).is_err());
    assert_eq!(end_session(&session.id).unwrap().ended_at, ended.ended_at);
    assert_eq!(
        std::fs::read(
            session_dir(&session.id)
                .unwrap()
                .join(SESSION_MANIFEST_FILE)
        )
        .unwrap(),
        bytes
    );
}
