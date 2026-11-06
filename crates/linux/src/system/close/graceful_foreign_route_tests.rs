use std::cell::Cell;
use std::collections::HashMap;
use std::time::Duration;

use super::super::{
    Request,
    fake::Fake,
    world::{World, answered_frames},
};
use super::close;
use crate::system::process::{liveness::Liveness, process_lookup::Lookup};
use crate::system::registry::registry_frames::RootRead;
use crate::system::signal::{Signal, signal_sent::Sent};
use crate::system::window::foreign::{
    foreign_close_route::ForeignCloseRoute,
    foreign_fake::{FakePort, deadline, record, thunar},
    foreign_frame::Frame,
    foreign_port::ToplevelPort,
    foreign_record::ToplevelRecord,
    foreign_request::ToplevelRequest,
};
use crate::system::window::window_ops::require_complete_frames;
use crate::system::x11::{
    control::{ControlRead, control_error::ControlError},
    window::toplevel::Toplevel,
};
use agent_desktop_core::{AdapterError, Deadline, DeliverySemantics, ErrorCode};

struct MappedPort {
    inner: FakePort,
    owners: Option<HashMap<u32, u32>>,
}

impl ToplevelPort for MappedPort {
    fn refresh(&self, deadline: Deadline) -> Result<(), ControlError> {
        self.inner.refresh(deadline)
    }
    fn records(&self) -> Vec<ToplevelRecord> {
        self.inner.records()
    }
    fn owners(&self, _: Deadline) -> Result<Option<HashMap<u32, u32>>, ControlError> {
        Ok(self.owners.clone())
    }
    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        self.inner.send(id, request)
    }
}

struct RouteWorld {
    fake: Fake,
    route: ForeignCloseRoute<MappedPort>,
    frames: Vec<Frame>,
    read: RootRead,
    unanswered: Cell<bool>,
}

impl World for RouteWorld {
    fn is_protected(&self) -> Result<bool, AdapterError> {
        self.fake.is_protected()
    }
    fn facts(&self) -> Lookup {
        self.fake.facts()
    }
    fn liveness(&self) -> Liveness {
        self.fake.liveness()
    }
    fn elapsed(&self) -> Duration {
        self.fake.elapsed()
    }
    fn remaining(&self) -> Duration {
        self.fake.remaining()
    }
    fn pause(&self) {
        self.fake.pause()
    }
    fn toplevels(&self) -> ControlRead<Vec<Toplevel>> {
        self.unanswered.set(false);
        let frames = || answered_frames(self.frames.clone(), self.read, &self.unanswered);
        match self.route.standing(&thunar(), frames, deadline()) {
            Ok(windows) => ControlRead::Ready(windows),
            Err(_) => ControlRead::Unavailable,
        }
    }
    fn toplevels_late(&self) -> ControlRead<Vec<Toplevel>> {
        self.toplevels()
    }
    fn nested_dialogs(&self) -> Vec<String> {
        Vec::new()
    }
    fn request_close(&self, id: u32) -> Result<(), ControlError> {
        self.route.close(id, deadline())
    }
    fn signal(&self, signal: Signal) -> Result<Sent, AdapterError> {
        self.fake.signal(signal)
    }
    fn timeout_error(&self) -> AdapterError {
        self.fake.timeout_error()
    }
    fn on_wayland(&self) -> bool {
        true
    }
    fn contested_windows(&self) -> usize {
        self.route.contested()
    }
    fn has_foreign_route(&self) -> bool {
        self.route.in_use()
    }
    fn frames_unanswered(&self) -> Result<(), AdapterError> {
        if self.unanswered.get() {
            require_complete_frames(RootRead::TimedOut)?;
        }
        Ok(())
    }
    fn foreign_unproven(&self) -> Vec<String> {
        self.route.unproven()
    }
    fn foreign_dialogs(&self) -> Vec<String> {
        self.route.dialogs()
    }
}

fn world(
    records: Vec<ToplevelRecord>,
    frames: Vec<Frame>,
    owners: Option<HashMap<u32, u32>>,
) -> RouteWorld {
    RouteWorld {
        fake: Fake::new(Liveness::Alive),
        route: ForeignCloseRoute::over(MappedPort {
            inner: FakePort::showing(records),
            owners,
        }),
        frames,
        read: RootRead::Read,
        unanswered: Cell::new(false),
    }
}

fn frame(title: &str, prompt: bool) -> Frame {
    Frame {
        title: title.to_owned(),
        prompt,
    }
}
fn request() -> Request<'static> {
    Request {
        name: "Files",
        pid: 7,
        instance: Some("t"),
        force: false,
    }
}

#[test]
fn close_refuses_unproven_shared_title_without_sending() {
    let scene = world(
        vec![
            record(1, "Home", "zzqqxx"),
            record(2, "Home", "thunar-viewer"),
        ],
        vec![frame("Home", false)],
        None,
    );
    let error = close(&scene, &request()).unwrap_err();
    assert_eq!(error.code, ErrorCode::ActionFailed);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(
        error.details.unwrap()["cause"],
        "toplevel_identity_unproven"
    );
    assert!(scene.route.port().unwrap().inner.sent().is_empty());
}

#[test]
fn close_refuses_unmapped_prompt_with_its_title_without_sending() {
    for owners in [None, Some(HashMap::from([(1, 7), (2, 8)]))] {
        let scene = world(
            vec![
                record(1, "Doc", "thunar"),
                record(2, "Save As", "unrelated"),
            ],
            vec![frame("Doc", false), frame("Save As", true)],
            owners,
        );
        let error = close(&scene, &request()).unwrap_err();
        assert_eq!(error.details.as_ref().unwrap()["cause"], "dialog_pending");
        assert_eq!(error.details.unwrap()["dialog"], "Save As");
        assert_eq!(error.disposition, DeliverySemantics::not_delivered());
        assert!(scene.route.port().unwrap().inner.sent().is_empty());
    }
}

#[test]
fn close_refuses_two_related_unknown_owners_without_sending() {
    let scene = world(
        vec![record(1, "Home", "thunar"), record(2, "Home", "thunar")],
        vec![frame("Home", false)],
        None,
    );
    let error = close(&scene, &request()).unwrap_err();
    assert_eq!(error.code, ErrorCode::AmbiguousTarget);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(scene.route.port().unwrap().inner.sent().is_empty());
}

#[test]
fn mapped_close_sends_only_to_the_target_process() {
    let scene = world(
        vec![
            record(1, "Home", "zzqqxx"),
            record(2, "Home", "thunar-viewer"),
        ],
        vec![frame("Home", false)],
        Some(HashMap::from([(1, 7), (2, 8)])),
    );
    let error = close(&scene, &request()).unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(
        scene.route.port().unwrap().inner.sent(),
        vec![(1, ToplevelRequest::Close)]
    );
}

#[test]
fn changed_list_before_close_is_revalidated_before_any_send() {
    let port = FakePort::showing(vec![record(1, "Home", "thunar")])
        .then_showing(vec![record(1, "Home", "thunar")])
        .then_showing(vec![
            record(1, "Home", "thunar"),
            record(2, "Home", "unrelated"),
        ]);
    let scene = RouteWorld {
        fake: Fake::new(Liveness::Alive),
        route: ForeignCloseRoute::over(MappedPort {
            inner: port,
            owners: None,
        }),
        frames: vec![frame("Home", false)],
        read: RootRead::Read,
        unanswered: Cell::new(false),
    };
    let error = close(&scene, &request()).unwrap_err();
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert!(scene.route.port().unwrap().inner.sent().is_empty());
}

#[test]
fn focus_uses_mapped_pid_instead_of_the_decoys_related_app_id() {
    use crate::system::window::foreign::{foreign_fake::target, foreign_focus};
    let port = MappedPort {
        inner: FakePort::showing(vec![
            record(1, "Home", "unrelated"),
            record(2, "Home", "thunar-viewer"),
        ]),
        owners: Some(HashMap::from([(1, 7), (2, 8)])),
    };
    foreign_focus::activate(&port, &target("Home"), deadline()).unwrap();
    assert_eq!(port.inner.sent(), vec![(1, ToplevelRequest::Activate)]);
}

#[test]
fn mapped_close_sends_to_both_same_title_windows_of_the_target() {
    let scene = world(
        vec![record(1, "Home", "first"), record(2, "Home", "second")],
        vec![frame("Home", false), frame("Home", false)],
        Some(HashMap::from([(1, 7), (2, 7)])),
    );
    assert_eq!(
        close(&scene, &request()).unwrap_err().code,
        ErrorCode::Timeout
    );
    assert_eq!(
        scene.route.port().unwrap().inner.sent(),
        vec![(1, ToplevelRequest::Close), (2, ToplevelRequest::Close)]
    );
}

#[test]
fn close_times_out_when_a_frame_read_ran_out_and_sends_nothing() {
    let mut scene = world(
        vec![record(1, "Home", "thunar"), record(2, "Save As", "thunar")],
        vec![frame("Home", false)],
        Some(HashMap::from([(1, 7), (2, 7)])),
    );
    scene.read = RootRead::TimedOut;
    let error = close(&scene, &request()).unwrap_err();
    assert_eq!(error.code, ErrorCode::Timeout);
    assert_eq!(error.disposition, DeliverySemantics::not_delivered());
    assert_eq!(error.details.unwrap()["kind"], "frames_unanswered");
    assert!(scene.route.port().unwrap().inner.sent().is_empty());
}
