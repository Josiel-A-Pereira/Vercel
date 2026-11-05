//! Windows-only: drives `agent_facing_owner` against a real hosted Settings
//! application (A26-8) and checks that `list-apps` and `close-app` address
//! a hosted frame by the same process `list-windows` reports for it. Staging,
//! lock order and cleanup are shared with `frame_identity_settings_tests`.
#![cfg(target_os = "windows")]

use super::agent_facing_owner;
use super::settings_tests::{HOSTED_APP_IMAGE, stage_hosted_settings};
use crate::system::app_ops::list_apps_live;
use crate::system::close::{close_app_impl, top_level_windows_for_pid};
use crate::system::close_outcome::CloseOutcome;
use crate::system::fixture::test_support::SHELL_SURFACE_LOCK;
use crate::system::fixture::test_time::deadline;
use crate::system::process_identity::{process_image_name, token_for_pid};
use crate::system::window::window_enum::WindowHandle;
use crate::system::window::window_identity::live_window_owner;
use crate::system::window::window_ops::list_windows_live;
use crate::tree::fixture::bootstrap;
use agent_desktop_core::{AppInfo, ProcessId, WindowFilter};

/// A hosted frame is addressed by its hosted application, not by the frame
/// host that owns the frame window, and an ordinary owner read still names
/// the frame host.
#[test]
fn a_hosted_frame_s_agent_facing_owner_is_the_hosted_application() {
    bootstrap();
    let _shell = SHELL_SURFACE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _stage = crate::tree::fixture::fixture_window::on_screen_stage();
    let staged = stage_hosted_settings();
    let Some(frame) = staged.frame.as_ref() else {
        return;
    };
    let handle = frame.handle as WindowHandle;
    assert_eq!(
        agent_facing_owner(handle),
        Some(ProcessId::from(frame.hosted_pid)),
        "a hosted frame is addressed by the hosted application"
    );
    assert_eq!(
        live_window_owner(handle),
        Some(ProcessId::from(frame.frame_host_pid)),
        "the frame window itself is still owned by the frame host"
    );
}

/// `list-apps` lists the hosted application under the pid and name
/// `list-windows` reports for its frame, so `--app` resolves the same
/// process in both listings.
#[test]
fn list_apps_reports_the_hosted_application_list_windows_reports() {
    bootstrap();
    let _shell = SHELL_SURFACE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _stage = crate::tree::fixture::fixture_window::on_screen_stage();
    let staged = stage_hosted_settings();
    let Some(frame) = staged.frame.as_ref() else {
        return;
    };
    let windows = list_windows_live(&WindowFilter::default(), deadline(15_000))
        .expect("the window listing succeeds");
    let window = windows
        .iter()
        .find(|window| u32::from(window.pid) == frame.hosted_pid)
        .expect("list-windows reports the hosted application");

    let apps = list_apps_live(deadline(15_000)).expect("the app listing succeeds");
    let app = apps
        .iter()
        .find(|app| app.pid == window.pid)
        .expect("list-apps reports the hosted application list-windows reports");
    assert!(
        app.name.eq_ignore_ascii_case(&window.app),
        "both listings name the hosted application alike: {} vs {}",
        app.name,
        window.app
    );
    assert!(
        app.name.eq_ignore_ascii_case(HOSTED_APP_IMAGE),
        "the listed app is the hosted application, got {}",
        app.name
    );
    assert_eq!(
        app.process_instance, window.process_instance,
        "both listings carry the same process generation"
    );
}

/// The window set a graceful `close-app` posts `WM_CLOSE` to for a hosted
/// application holds its frame. Read without posting, so a Settings session
/// that was open before the test is left running.
#[test]
fn close_app_window_set_for_a_hosted_application_holds_its_frame() {
    bootstrap();
    let _shell = SHELL_SURFACE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _stage = crate::tree::fixture::fixture_window::on_screen_stage();
    let staged = stage_hosted_settings();
    let Some(frame) = staged.frame.as_ref() else {
        return;
    };
    let windows = top_level_windows_for_pid(ProcessId::from(frame.hosted_pid))
        .expect("the close window enumeration succeeds");
    assert!(
        windows.contains(&frame.handle),
        "close-app addresses the hosted application's frame"
    );
}

/// A graceful `close-app` on a hosted application reaches its frame and the
/// process exits. A Settings session that was open before the test is left
/// running, so the close leg only runs against a session the test started.
#[test]
fn close_app_gracefully_closes_a_hosted_application() {
    bootstrap();
    let _shell = SHELL_SURFACE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _stage = crate::tree::fixture::fixture_window::on_screen_stage();
    let staged = stage_hosted_settings();
    if staged.settings_preexisting {
        eprintln!("skip hosted close: Settings was already open and is left as found");
        return;
    }
    let Some(frame) = staged.frame.as_ref() else {
        return;
    };
    let pid = ProcessId::from(frame.hosted_pid);
    let app = AppInfo {
        name: process_image_name(pid).expect("the hosted image name is readable"),
        pid,
        bundle_id: None,
        process_instance: token_for_pid(pid).expect("the token read answers"),
        presentation: None,
    };
    assert!(
        app.process_instance.is_some(),
        "a live hosted process carries a generation token"
    );
    let outcome = close_app_impl(&app, false, deadline(15_000))
        .expect("a graceful close reaches the hosted application's frame");
    assert_eq!(outcome, CloseOutcome::Exited);
    assert!(
        process_image_name(pid).is_none(),
        "the hosted application has exited"
    );
}
