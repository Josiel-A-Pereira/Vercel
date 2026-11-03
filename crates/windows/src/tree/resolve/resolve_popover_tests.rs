use super::resolve_anchor::resolve_locator_anchor;
use crate::system::window::window_identity::live_window_owner;
use crate::tree::fixture::{ensure_test_apartment, fixture_popover::PopoverFixture};
use crate::tree::walk::walker::WalkBudget;
use crate::tree::walk::walker_source::UiaTreeSource;
use agent_desktop_core::{Deadline, SnapshotSurface};
use std::ops::ControlFlow;

#[test]
fn a_ref_from_a_live_popover_resolves_back_into_that_popover() {
    ensure_test_apartment();
    let fixture = PopoverFixture::create().expect("the owned popup fixture starts");
    let owner_pid = live_window_owner(fixture.owner_handle() as _)
        .expect("the owner window belongs to a live process");
    let token = crate::system::process_identity::token_for_pid(owner_pid)
        .expect("the owner process token is readable")
        .expect("the owner process is live");
    let owner = agent_desktop_core::WindowInfo {
        id: format!("w-{}", fixture.owner_handle()),
        title: String::new(),
        app: String::new(),
        pid: owner_pid,
        process_instance: Some(token.clone()),
        bounds: None,
        state: Default::default(),
    };
    let deadline = Deadline::after(10_000).expect("the resolution deadline is valid");
    let popup = crate::tree::automation::root_from_hwnd(fixture.popup_handle(), deadline)
        .expect("the popup HWND roots a UI Automation element");
    let source = UiaTreeSource::for_root(&popup).expect("the popup provider resolves");
    let prepared = source
        .prepare_root(&popup)
        .expect("the popup root is prepared");
    let target = crate::tree::walk::walker_fake::scan_subtree(
        &source,
        &prepared,
        &WalkBudget::new(10, deadline),
        10,
        &mut |path, _, evidence, _| {
            if evidence
                .name
                .known()
                .is_some_and(|name| name == "fixture-button")
            {
                ControlFlow::Break((path.to_vec(), evidence))
            } else {
                ControlFlow::Continue(())
            }
        },
    )
    .expect("the popup tree can be scanned")
    .expect("the popup exposes its button");
    let (path, evidence) = target;
    let mut entry = crate::tree::walk::walker_fake::ref_entry(
        evidence.role.known().expect("the button role is known"),
    );
    entry.identity.name = Some("fixture-button".into());
    entry.identity.native_id = evidence.identifiers.preferred_identifier().cloned();
    entry.scope.path = path.into();
    entry.process = agent_desktop_core::RefProcess {
        pid: owner_pid,
        process_instance: Some(token),
    };
    entry.identity.unique_at_capture = true;
    entry.source.source_app = Some(String::new());
    entry.source.source_window_id = Some(owner.id.clone());
    entry.source.source_window_title = Some(String::new());
    entry.source.source_surface = SnapshotSurface::Popover;
    entry.scope.path_is_absolute = true;

    let resolved = resolve_locator_anchor(&entry, deadline)
        .expect("a ref captured on a popover must resolve against the same popover");

    assert!(
        resolved
            .downcast_ref::<crate::tree::element::UIAElement>()
            .is_some()
    );
}
