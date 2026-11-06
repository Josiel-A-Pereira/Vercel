use std::collections::HashMap;

use super::*;
use crate::system::window::foreign::foreign_fake::{FakePort, deadline, ignores, record, thunar};
use crate::system::window::foreign::foreign_record::ToplevelRecord;

const PID: u32 = 7;

struct Teardown {
    port: FakePort,
    owned_after_first: Vec<u32>,
}

impl ToplevelPort for Teardown {
    fn refresh(&self, deadline: Deadline) -> Result<(), ControlError> {
        self.port.refresh(deadline)
    }

    fn records(&self) -> Vec<ToplevelRecord> {
        self.port.records()
    }

    fn owners(&self, _: Deadline) -> Result<Option<HashMap<u32, u32>>, ControlError> {
        let owned: Vec<u32> = if self.port.sent().is_empty() {
            vec![1, 2, 3]
        } else {
            self.owned_after_first.clone()
        };
        Ok(Some(owned.into_iter().map(|id| (id, PID)).collect()))
    }

    fn address(&self, id: u32) -> Option<u64> {
        Some(u64::from(id))
    }

    fn send(&self, id: u32, request: ToplevelRequest) -> Result<(), ControlError> {
        self.port.send(id, request)
    }
}

fn frames() -> Result<Vec<Frame>, ControlError> {
    Ok(vec![Frame {
        title: "Home".to_owned(),
        prompt: false,
    }])
}

fn route(records: Vec<ToplevelRecord>, owned_after_first: Vec<u32>) -> ForeignCloseRoute<Teardown> {
    let route = ForeignCloseRoute::over(Teardown {
        port: FakePort::showing(records).reacting(ignores),
        owned_after_first,
    });
    route
        .standing(&thunar(), frames, deadline())
        .expect("a read");
    route
}

fn sent_ids(route: &ForeignCloseRoute<Teardown>) -> Vec<u32> {
    let port = route.port().expect("a port");
    port.port.sent().iter().map(|(id, _)| *id).collect()
}

#[test]
fn a_window_being_closed_does_not_unprove_its_same_titled_sibling() {
    let route = route(
        vec![record(1, "Home", "thunar"), record(2, "Home", "thunar")],
        vec![2],
    );
    route.close(1, deadline()).expect("the first request");
    route
        .close(2, deadline())
        .expect("the sibling is still proven");
    assert_eq!(sent_ids(&route), vec![1, 2]);
}

#[test]
fn an_unsent_same_titled_toplevel_with_no_owner_still_unproves_the_title() {
    let route = route(
        vec![
            record(1, "Home", "thunar"),
            record(2, "Home", "thunar"),
            record(3, "Home", "thunar"),
        ],
        vec![2],
    );
    route.close(1, deadline()).expect("the first request");
    assert_eq!(route.close(2, deadline()), Err(ControlError::StaleWindow));
    assert_eq!(sent_ids(&route), vec![1]);
}
