//! Where each gated command lands, decided from its arguments alone. A plan
//! names the target without looking anything up; resolving it may read the
//! ref store or ask the platform adapter, so the gate resolves plans only
//! once policy discovery has found policies.

use agent_desktop_core::SnapshotSurface;

use crate::cli::Commands;
use crate::cli_args::WindowScope;
use crate::dispatch::parse::parse_xy;

pub(super) enum TargetPlan<'a> {
    System,
    App(Option<&'a str>),
    Ref(&'a str),
    /// No named target: whatever window has focus.
    Focused,
    /// The application that owns this window.
    WindowId(&'a str),
    /// The owners of the windows whose title holds `title`.
    Titled {
        app: Option<&'a str>,
        title: &'a str,
    },
    /// Pointer input at screen coordinates: whatever window is on top there.
    Point(f64, f64),
    /// A window of the named app, any of which the command may act on, so
    /// each is judged with its title.
    AppWindows(&'a str),
    /// Whatever any running app shows, such as its notifications.
    Running,
    /// The command names nothing the gate can judge it by.
    Unnamed(&'static str),
    /// OS chrome such as the Dock or the taskbar, observed through the shell
    /// process that hosts it; where the platform resolves no shell surface,
    /// the command falls back to the app's windows, or the focused window.
    Shell {
        surface: SnapshotSurface,
        app: Option<&'a str>,
    },
}

const NOTIFICATION_APP_UNNAMED: &str = "the notification's app is not named; pass --expected-app with the app list-notifications reports";

/// The targets a command must be allowed on: none for an exempt command,
/// two for a drag (it acts where it picks up and where it drops).
/// Administrative surfaces (version, skills, status, permissions, session,
/// trace, cursor-overlay) and `batch`, whose items are gated one by one as
/// they dispatch, are exempt.
pub(super) fn plan_for(cmd: &Commands) -> Option<Vec<TargetPlan<'_>>> {
    let plan = match cmd {
        Commands::Version
        | Commands::Skills(_)
        | Commands::Status
        | Commands::Permissions(_)
        | Commands::Session(_)
        | Commands::Trace(_)
        | Commands::CursorOverlay(_)
        | Commands::Batch(_) => return None,

        Commands::Snapshot(args) => match &args.root {
            Some(root) => TargetPlan::Ref(root),
            None => surface_scope(&args.scope, args.surface.to_core()),
        },
        Commands::Find(args) => match &args.root {
            Some(root) => TargetPlan::Ref(root),
            None => surface_scope(&args.scope, args.surface.to_core()),
        },
        Commands::Screenshot(args) => {
            let names_a_window = args.scope.app.is_some() || args.scope.window_id.is_some();
            if args.screen.is_none() && names_a_window {
                window_scope(&args.scope)
            } else {
                TargetPlan::System
            }
        }
        Commands::Get(args) => TargetPlan::Ref(&args.ref_id),
        Commands::Is(args) => TargetPlan::Ref(&args.ref_id),

        Commands::Click(args)
        | Commands::DoubleClick(args)
        | Commands::TripleClick(args)
        | Commands::RightClick(args)
        | Commands::Clear(args)
        | Commands::Focus(args)
        | Commands::Toggle(args)
        | Commands::Check(args)
        | Commands::Uncheck(args)
        | Commands::Expand(args)
        | Commands::Collapse(args)
        | Commands::ScrollTo(args) => TargetPlan::Ref(&args.ref_id),
        Commands::Type(args) => TargetPlan::Ref(&args.ref_id),
        Commands::SetValue(args) => TargetPlan::Ref(&args.ref_id),
        Commands::Select(args) => TargetPlan::Ref(&args.ref_id),
        Commands::Scroll(args) => TargetPlan::Ref(&args.ref_id),
        Commands::Hover(args) => match (&args.ref_id, &args.xy) {
            (Some(ref_id), _) => TargetPlan::Ref(ref_id),
            (None, Some(xy)) => point(xy),
            (None, None) => TargetPlan::System,
        },
        Commands::Drag(args) => {
            let from = drag_end(args.target.from.as_deref(), args.target.from_xy.as_deref());
            let to = drag_end(args.target.to.as_deref(), args.target.to_xy.as_deref());
            let ends: Vec<_> = from.into_iter().chain(to).collect();
            return Some(if ends.is_empty() {
                vec![TargetPlan::System]
            } else {
                ends
            });
        }

        Commands::Press(args) => match (&args.app, &args.window_id) {
            (Some(app), _) => TargetPlan::AppWindows(app),
            (None, Some(window_id)) => TargetPlan::WindowId(window_id),
            (None, None) => TargetPlan::Focused,
        },
        Commands::KeyDown(_) | Commands::KeyUp(_) => TargetPlan::Focused,
        Commands::MouseMove(args) => point(&args.xy),
        Commands::MouseClick(args) => point(&args.xy),
        Commands::MouseDown(args) | Commands::MouseUp(args) => point(&args.xy),
        Commands::MouseWheel(args) => TargetPlan::Point(args.x, args.y),

        Commands::Launch(args) => TargetPlan::App(Some(&args.app)),
        Commands::CloseApp(args) => TargetPlan::App(Some(&args.app)),
        Commands::ListWindows(args) => TargetPlan::App(args.app.as_deref()),
        Commands::ListDisplays => TargetPlan::System,
        Commands::ListApps(args) => TargetPlan::App(args.app.as_deref()),
        Commands::FocusWindow(args) => match (&args.window_id, &args.title) {
            (Some(window_id), _) => TargetPlan::WindowId(window_id),
            (None, Some(title)) => TargetPlan::Titled {
                app: args.app.as_deref(),
                title,
            },
            (None, None) => match &args.app {
                Some(app) => TargetPlan::AppWindows(app),
                None => TargetPlan::App(None),
            },
        },
        Commands::ResizeWindow(args) => window_scope(&args.scope),
        Commands::MoveWindow(args) => window_scope(&args.scope),
        Commands::Minimize(args) | Commands::Maximize(args) | Commands::Restore(args) => {
            window_scope(&args.scope)
        }
        Commands::ListSurfaces(args) => match &args.app {
            Some(app) => TargetPlan::AppWindows(app),
            None => TargetPlan::Focused,
        },
        Commands::OpenSystemSurface(_) => TargetPlan::System,

        Commands::ListNotifications(args) => notifications_of(args.app.as_deref()),
        Commands::DismissNotification(args) => {
            notification_at(args.expected_app.as_deref().or(args.app.as_deref()))
        }
        Commands::DismissAllNotifications(args) => notifications_of(args.app.as_deref()),
        Commands::NotificationAction(args) => notification_at(args.expected_app.as_deref()),

        Commands::ClipboardGet(_) | Commands::ClipboardSet(_) | Commands::ClipboardClear => {
            TargetPlan::System
        }
        Commands::Wait(args) => match (&args.mode.element, &args.app) {
            (Some(ref_id), _) => TargetPlan::Ref(ref_id),
            (None, app) => TargetPlan::App(app.as_deref()),
        },
    };
    Some(vec![plan])
}

/// A window-scoped command acts on the named window, else a window of the
/// named app, else the focused window.
fn window_scope(scope: &WindowScope) -> TargetPlan<'_> {
    match (&scope.window_id, &scope.app) {
        (Some(window_id), _) => TargetPlan::WindowId(window_id),
        (None, Some(app)) => TargetPlan::AppWindows(app),
        (None, None) => TargetPlan::Focused,
    }
}

/// A shell surface is read from the shell process, whatever app is named,
/// unless a window id names the window or `--app` names the app whose own
/// status item `menu-bar-extras` reads.
fn surface_scope(scope: &WindowScope, surface: SnapshotSurface) -> TargetPlan<'_> {
    let app_status_item = scope.app.is_some() && surface == SnapshotSurface::MenuBarExtras;
    if surface.is_shell() && !app_status_item && scope.window_id.is_none() {
        return TargetPlan::Shell {
            surface,
            app: scope.app.as_deref(),
        };
    }
    window_scope(scope)
}

/// Without an app filter, the notifications of every running app are in
/// reach.
fn notifications_of(app: Option<&str>) -> TargetPlan<'_> {
    match app {
        Some(app) => TargetPlan::App(Some(app)),
        None => TargetPlan::Running,
    }
}

/// The notification at an index belongs to the app its expected identity
/// names; dispatch refuses it when that app does not match. A title alone
/// says nothing about the app, so the gate cannot judge it.
fn notification_at(app: Option<&str>) -> TargetPlan<'_> {
    match app {
        Some(app) => TargetPlan::App(Some(app)),
        None => TargetPlan::Unnamed(NOTIFICATION_APP_UNNAMED),
    }
}

/// Coordinates the command itself will reject (`INVALID_ARGS`) deliver
/// nothing, so they plan no app.
fn point(xy: &str) -> TargetPlan<'_> {
    match parse_xy(xy) {
        Ok((x, y)) => TargetPlan::Point(x, y),
        Err(_) => TargetPlan::System,
    }
}

fn drag_end<'a>(ref_id: Option<&'a str>, xy: Option<&'a str>) -> Option<TargetPlan<'a>> {
    match (ref_id, xy) {
        (Some(ref_id), _) => Some(TargetPlan::Ref(ref_id)),
        (None, Some(xy)) => Some(point(xy)),
        (None, None) => None,
    }
}
