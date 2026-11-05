use super::*;
use crate::adapter::{ActionOps, InputOps, ObservationOps, SystemOps};
use crate::refs::refs_test_support::HomeGuard;
use crate::storage::private_file_ops::{PrivateFileOps, with_test_ops_override};
use crate::{PermissionState, RefMap};
use std::io;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, mpsc};
use std::time::Duration;

const COORDINATION_LIMIT: Duration = Duration::from_secs(30);

struct StatusAdapter;
impl ObservationOps for StatusAdapter {}
impl ActionOps for StatusAdapter {}
impl InputOps for StatusAdapter {}
impl SystemOps for StatusAdapter {}

struct PausedLatestRead {
    reads: AtomicUsize,
    pauses: usize,
    ready: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}

impl PrivateFileOps for PausedLatestRead {
    fn read_private_bounded(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        let bytes = crate::storage::private_file::read_private_bounded_portable(path, max_bytes)?;
        if path
            .file_name()
            .is_some_and(|name| name == "latest_snapshot_id")
            && self.reads.fetch_add(1, Ordering::Relaxed) < self.pauses
        {
            self.ready
                .send(())
                .map_err(|error| io::Error::other(error.to_string()))?;
            self.release
                .lock()
                .unwrap()
                .recv_timeout(COORDINATION_LIMIT)
                .map_err(|error| io::Error::other(error.to_string()))?;
        }
        Ok(bytes)
    }
}

struct StatusWorker {
    release: mpsc::Sender<()>,
    thread: Option<std::thread::JoinHandle<Result<Value, AppError>>>,
}

impl Drop for StatusWorker {
    fn drop(&mut self) {
        let _ = self.release.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn new_map() -> RefMap {
    let node = serde_json::from_str(r#"{"role":"button","name":"New"}"#).unwrap();
    let source = crate::refs::ref_alloc_source::RefAllocSource {
        pid: crate::ProcessId::new(42),
        app: Some("App"),
        window_id: Some("w-42"),
        window_title: None,
        window_bounds_hash: None,
        process_instance: Some("instance"),
        surface: crate::SnapshotSurface::Window,
    };
    let mut map = RefMap::new();
    map.allocate(crate::refs::ref_alloc::ref_entry_from_node(
        &node,
        &source,
        None,
        &[0],
    ));
    map
}

fn run_rollover(rounds: usize) {
    let home = HomeGuard::new();
    let store = RefStore::new().unwrap();
    let first = "aaa-selected-first".to_string();
    store.save_snapshot(&first, &RefMap::new()).unwrap();
    store.set_latest(&first).unwrap();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let reader_home = home.path().to_path_buf();
    let thread = std::thread::spawn(move || {
        let _clock = crate::wait::deadline::VirtualClock::freeze();
        crate::refs::set_home_override(Some(reader_home));
        let report = PermissionReport {
            accessibility: PermissionState::Granted,
            screen_recording: PermissionState::Granted,
            automation: PermissionState::NotRequired,
        };
        with_test_ops_override(
            Rc::new(PausedLatestRead {
                reads: AtomicUsize::new(0),
                pauses: rounds,
                ready: ready_tx,
                release: Mutex::new(release_rx),
            }),
            || {
                execute_with_report_with_context(
                    &StatusAdapter,
                    &report,
                    &CommandContext::default(),
                )
            },
        )
    });
    let mut worker = StatusWorker {
        release: release_tx,
        thread: Some(thread),
    };
    let mut latest = first.clone();
    for round in 0..rounds {
        match ready_rx.recv_timeout(COORDINATION_LIMIT) {
            Ok(()) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                worker.thread.take().unwrap().join().unwrap().unwrap();
                panic!("status ended before the next selected snapshot read");
            }
            Err(error) => panic!("status did not reach selected snapshot read: {error}"),
        }
        let selected_directory = store.base_dir().join("snapshots").join(&latest);
        let lock = crate::storage::private_file::open_private_lock_portable(
            &store.base_dir().join("refstore.lock"),
            false,
        )
        .unwrap();
        let contended = match lock.try_lock() {
            Ok(()) => {
                lock.unlock().unwrap();
                false
            }
            Err(std::fs::TryLockError::WouldBlock) => true,
            Err(error) => panic!("cannot probe store lock: {error}"),
        };
        drop(lock);
        if contended {
            let _ = worker.release.send(());
        }
        for _ in 0..129 {
            store.save_new_snapshot(&new_map()).unwrap();
        }
        assert!(!selected_directory.exists());
        latest = format!("aaa-selected-{round}");
        store.save_snapshot(&latest, &new_map()).unwrap();
        store.set_latest(&latest).unwrap();
        if !contended {
            let _ = worker.release.send(());
        }
    }
    let status = worker.thread.take().unwrap().join().unwrap().unwrap();
    if status["snapshot_id"] == first {
        assert_eq!(status["ref_count"], 0);
    } else {
        assert_eq!(status["snapshot_id"], latest);
        assert_eq!(status["ref_count"], 1);
    }
}

#[test]
fn status_survives_latest_snapshot_retention_rollover() {
    run_rollover(1);
}

#[test]
fn status_survives_two_successive_retention_rollovers() {
    run_rollover(2);
}

struct ReadOnlyOps {
    pointer_reads: AtomicUsize,
    deny_refmap: bool,
}

impl PrivateFileOps for ReadOnlyOps {
    fn open_private_lock(&self, _path: &Path, _create: bool) -> io::Result<std::fs::File> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "read-only store",
        ))
    }
    fn write_atomic(&self, _path: &Path, _bytes: &[u8]) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "read-only store",
        ))
    }
    fn read_private_bounded(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        if path
            .file_name()
            .is_some_and(|name| name == "latest_snapshot_id")
        {
            self.pointer_reads.fetch_add(1, Ordering::Relaxed);
        }
        if self.deny_refmap && path.file_name().is_some_and(|name| name == "refmap.json") {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "cannot read map",
            ));
        }
        crate::storage::private_file::read_private_bounded_portable(path, max_bytes)
    }
}

fn read_status(context: &CommandContext) -> Result<Value, AppError> {
    execute_with_report_with_context(
        &StatusAdapter,
        &PermissionReport {
            accessibility: PermissionState::Granted,
            screen_recording: PermissionState::Granted,
            automation: PermissionState::NotRequired,
        },
        context,
    )
}

#[test]
fn status_reads_existing_store_without_lock_file_or_write_access() {
    let _home = HomeGuard::new();
    let store = RefStore::new().unwrap();
    let id = store.save_new_snapshot(&new_map()).unwrap();
    let lock = store.base_dir().join("refstore.lock");
    std::fs::remove_file(&lock).unwrap();
    let ops = Rc::new(ReadOnlyOps {
        pointer_reads: AtomicUsize::new(0),
        deny_refmap: false,
    });
    let result =
        with_test_ops_override(ops.clone(), || read_status(&CommandContext::default())).unwrap();
    assert_eq!(result["snapshot_id"], id);
    assert_eq!(result["ref_count"], 1);
    assert_eq!(ops.pointer_reads.load(Ordering::Relaxed), 1);
    assert!(!lock.exists());
}

#[test]
fn status_does_not_create_a_missing_namespace() {
    let _home = HomeGuard::new();
    let store = RefStore::for_session(Some("missing-status")).unwrap();
    let context = CommandContext::new(Some("missing-status".into()), None, false).unwrap();
    let ops = Rc::new(ReadOnlyOps {
        pointer_reads: AtomicUsize::new(0),
        deny_refmap: false,
    });
    assert!(!store.base_dir().exists());
    let value = with_test_ops_override(ops, || read_status(&context)).unwrap();
    assert!(value["snapshot_id"].is_null());
    assert!(value["ref_count"].is_null());
    assert!(!store.base_dir().exists());
}

#[test]
fn status_does_not_hide_unchanged_missing_or_corrupt_maps() {
    for corrupt in [false, true] {
        let _home = HomeGuard::new();
        let store = RefStore::new().unwrap();
        let id = store.save_new_snapshot(&new_map()).unwrap();
        let directory = store.base_dir().join("snapshots").join(&id);
        if corrupt {
            crate::refs::write_private_file(&directory.join("refmap.json"), b"not json").unwrap();
        } else {
            std::fs::remove_dir_all(&directory).unwrap();
        }
        let ops = Rc::new(ReadOnlyOps {
            pointer_reads: AtomicUsize::new(0),
            deny_refmap: false,
        });
        let error = with_test_ops_override(ops.clone(), || read_status(&CommandContext::default()))
            .unwrap_err();
        if corrupt {
            assert!(matches!(error, AppError::Json(_)));
        } else {
            assert_eq!(error.code(), "SNAPSHOT_NOT_FOUND");
        }
        if corrupt {
            assert_eq!(ops.pointer_reads.load(Ordering::Relaxed), 1);
        } else {
            assert!(ops.pointer_reads.load(Ordering::Relaxed) <= 2);
        }
    }
}

#[test]
fn status_preserves_refmap_permission_errors_without_refreshing() {
    let _home = HomeGuard::new();
    RefStore::new()
        .unwrap()
        .save_new_snapshot(&new_map())
        .unwrap();
    let ops = Rc::new(ReadOnlyOps {
        pointer_reads: AtomicUsize::new(0),
        deny_refmap: true,
    });
    let error = with_test_ops_override(ops.clone(), || read_status(&CommandContext::default()))
        .unwrap_err();
    assert_eq!(error.code(), "PERM_DENIED");
    assert_eq!(ops.pointer_reads.load(Ordering::Relaxed), 1);
}

struct AdvancingSnapshot {
    store: RefStore,
    conflicts: AtomicUsize,
}

impl PrivateFileOps for AdvancingSnapshot {
    fn read_private_bounded(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        if path.file_name().is_some_and(|name| name == "refmap.json") {
            self.store
                .save_new_snapshot(&new_map())
                .map_err(|error| io::Error::other(error.to_string()))?;
            std::fs::remove_dir_all(path.parent().unwrap())?;
            self.conflicts.fetch_add(1, Ordering::Relaxed);
            crate::wait::deadline::sleep(Duration::from_millis(2));
        }
        crate::storage::private_file::read_private_bounded_portable(path, max_bytes)
    }
}

#[test]
fn continuous_snapshot_version_changes_obey_the_inherited_deadline() {
    let _home = HomeGuard::new();
    let store = RefStore::new().unwrap();
    store.save_new_snapshot(&new_map()).unwrap();
    let clock = crate::wait::deadline::VirtualClock::freeze();
    let inherited = crate::Deadline::after(5).unwrap();
    let _scope = crate::wait::deadline::enter_scope(Some(inherited));
    let ops = Rc::new(AdvancingSnapshot {
        store,
        conflicts: AtomicUsize::new(0),
    });
    let error = with_test_ops_override(ops.clone(), || read_status(&CommandContext::default()))
        .unwrap_err();
    assert_eq!(error.code(), "TIMEOUT");
    assert_eq!(ops.conflicts.load(Ordering::Relaxed), 3);
    assert_eq!(clock.elapsed(), Duration::from_millis(6));
}

struct RemovedLatestPointer;
impl PrivateFileOps for RemovedLatestPointer {
    fn read_private_bounded(&self, path: &Path, max_bytes: u64) -> io::Result<Vec<u8>> {
        if path.file_name().is_some_and(|name| name == "refmap.json") {
            let snapshot = path.parent().unwrap();
            std::fs::remove_file(
                snapshot
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("latest_snapshot_id"),
            )?;
            std::fs::remove_dir_all(snapshot)?;
        }
        crate::storage::private_file::read_private_bounded_portable(path, max_bytes)
    }
}

#[test]
fn status_preserves_missing_map_error_when_latest_pointer_disappears() {
    let _home = HomeGuard::new();
    RefStore::new()
        .unwrap()
        .save_new_snapshot(&new_map())
        .unwrap();
    let error = with_test_ops_override(Rc::new(RemovedLatestPointer), || {
        read_status(&CommandContext::default())
    })
    .unwrap_err();
    assert_eq!(error.code(), "SNAPSHOT_NOT_FOUND");
}

#[test]
fn diagnostic_status_fast_path_does_not_add_a_deadline_precondition() {
    let _home = HomeGuard::new();
    RefStore::new()
        .unwrap()
        .save_new_snapshot(&new_map())
        .unwrap();
    let _clock = crate::wait::deadline::VirtualClock::freeze();
    let inherited = crate::Deadline::after(0).unwrap();
    let _scope = crate::wait::deadline::enter_scope(Some(inherited));
    assert_eq!(
        read_status(&CommandContext::default()).unwrap()["ref_count"],
        1
    );
}
