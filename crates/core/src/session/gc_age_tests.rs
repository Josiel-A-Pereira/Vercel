use super::*;
use crate::refs::refs_test_support::HomeGuard;
use crate::storage::private_file_ops::{PrivateFileOps, with_test_ops_override};
use std::fs::File;
use std::io;
use std::rc::Rc;
use std::sync::{Mutex, mpsc};

const COORDINATION_LIMIT: Duration = Duration::from_secs(5);

struct PausedGcLock {
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl PrivateFileOps for PausedGcLock {
    fn open_private_lock(&self, path: &Path, create: bool) -> io::Result<File> {
        if create && path.file_name().is_some_and(|name| name == "refstore.lock") {
            self.ready
                .send(())
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.release
                .lock()
                .unwrap()
                .recv_timeout(COORDINATION_LIMIT)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        crate::storage::private_file::open_private_lock_portable(path, create)
    }
}

struct GcWorker {
    release: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<Result<GcReport, AppError>>>,
}

impl Drop for GcWorker {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn gc_rechecks_age_when_an_idle_session_ends_before_the_delete_lock() {
    let home = HomeGuard::new();
    let mut session = start_session(StartSessionOptions {
        trace: SessionTraceMode::Off,
        ..Default::default()
    })
    .unwrap();
    session.created_at = 0;
    write_manifest(&session).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let gc_home = home.path().to_path_buf();
    let thread = std::thread::spawn(move || {
        crate::refs::set_home_override(Some(gc_home));
        with_test_ops_override(
            Rc::new(PausedGcLock {
                ready: ready_tx,
                release: Mutex::new(release_rx),
            }),
            || {
                gc(GcOptions {
                    ended_only: false,
                    older_than: Some(Duration::from_secs(3600)),
                })
            },
        )
    });
    let mut worker = GcWorker {
        release: release_tx,
        thread: Some(thread),
    };
    ready_rx.recv_timeout(COORDINATION_LIMIT).unwrap();
    let ended = end_session(&session.id).unwrap();
    assert!(ended.ended_at.is_some());
    worker.release.send(()).unwrap();
    let report = worker.thread.take().unwrap().join().unwrap().unwrap();
    assert!(
        !report.removed.contains(&session.id),
        "a newly ended session must retain its full age threshold"
    );
    assert_eq!(
        read_manifest(&session.id).unwrap().unwrap().ended_at,
        ended.ended_at
    );
}

#[test]
fn gc_age_and_ended_filters_preserve_existing_selection() {
    for (ended_at, ended_only, older_than, removed) in [
        (Some(0), true, Some(Duration::from_secs(3600)), true),
        (
            Some(now_millis()),
            true,
            Some(Duration::from_secs(3600)),
            false,
        ),
        (None, true, Some(Duration::ZERO), false),
        (None, false, Some(Duration::ZERO), true),
        (None, false, None, false),
        (Some(0), false, None, true),
    ] {
        let _home = HomeGuard::new();
        let mut session = start_session(StartSessionOptions {
            trace: SessionTraceMode::Off,
            ..Default::default()
        })
        .unwrap();
        session.created_at = 0;
        session.ended_at = ended_at;
        write_manifest(&session).unwrap();
        let report = gc(GcOptions {
            ended_only,
            older_than,
        })
        .unwrap();
        assert_eq!(report.removed.contains(&session.id), removed);
        assert_eq!(session_dir(&session.id).unwrap().exists(), !removed);
    }
}
