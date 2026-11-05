use agent_desktop_core::{
    ActionOps, AdapterError, AppInfo, Deadline, InputOps, ObservationOps, SystemOps,
    context::CommandContext,
};
use clap::{CommandFactory, Parser};

use crate::cli::{Cli, Commands};

use super::super::dogwood_plan::plan_for;
use super::super::dogwood_target::Resolver;
use super::EXEMPT_COMMANDS;

/// A desktop with no running application that answers no question about
/// windows.
struct NoDesktop;
impl ObservationOps for NoDesktop {
    fn list_apps(&self, _deadline: Deadline) -> Result<Vec<AppInfo>, AdapterError> {
        Ok(Vec::new())
    }
}
impl ActionOps for NoDesktop {}
impl InputOps for NoDesktop {}
impl SystemOps for NoDesktop {}

fn command(args: &[&str]) -> Commands {
    let mut argv = vec!["agent-desktop"];
    argv.extend_from_slice(args);
    Cli::try_parse_from(argv)
        .expect("test command parses")
        .command
        .expect("test command present")
}

fn targets(args: &[&str]) -> Option<Result<Vec<agent_desktop_policy::CommandFacts>, String>> {
    let cmd = command(args);
    let context = CommandContext::default();
    let plans = plan_for(&cmd)?;
    let mut resolver = Resolver::new(&context, &NoDesktop);
    let mut facts = Vec::new();
    for plan in &plans {
        match resolver.resolve(plan) {
            Ok(targets) => facts.extend(
                targets
                    .into_iter()
                    .map(|target| super::facts_from(&cmd, &context, target)),
            ),
            Err(reason) => return Some(Err(reason)),
        }
    }
    Some(Ok(facts))
}

fn facts(args: &[&str]) -> Option<agent_desktop_policy::CommandFacts> {
    targets(args).map(|resolved| {
        resolved
            .expect("target resolves without the desktop")
            .remove(0)
    })
}

#[test]
fn every_cli_command_is_exempt_xor_gated() {
    for subcommand in Cli::command().get_subcommands() {
        let name = subcommand.get_name();
        let exempt = EXEMPT_COMMANDS.contains(&name);
        let gated = agent_desktop_policy::group_for(name).is_some();
        assert!(
            exempt ^ gated,
            "command '{name}' must be either exempt or in the policy catalog, not {}",
            if exempt { "both" } else { "neither" }
        );
    }
}

#[test]
fn every_catalog_entry_is_a_real_command() {
    let names: Vec<_> = Cli::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect();
    for (command, _) in agent_desktop_policy::COMMAND_GROUPS {
        assert!(
            names.iter().any(|name| name == command),
            "policy catalog names unknown command '{command}'"
        );
    }
}

#[test]
fn exempt_commands_build_no_facts() {
    assert!(facts(&["version"]).is_none());
    assert!(facts(&["status"]).is_none());
    assert!(facts(&["batch", "[]"]).is_none());
    assert!(facts(&["skills"]).is_none());
}

#[test]
fn app_commands_carry_their_target() {
    let launch = facts(&["launch", "Safari"]).expect("launch is gated");
    assert_eq!(launch.command, "launch");
    assert_eq!(launch.target.app, "Safari");
    assert!(launch.mutating);

    let close = facts(&["close-app", "Notes"]).expect("close-app is gated");
    assert_eq!(close.app_or_system(), "Notes");

    let snapshot = facts(&["snapshot", "--app", "Finder"]).expect("snapshot is gated");
    assert_eq!(snapshot.target.app, "Finder");
    assert!(!snapshot.mutating);

    let press = facts(&["press", "cmd+q", "--app", "Mail"]).expect("press is gated");
    assert_eq!(press.target.app, "Mail");
}

#[test]
fn global_input_is_never_judged_as_system() {
    for args in [
        &["mouse-wheel", "--x", "10", "--y", "10", "--dy", "3"][..],
        &["mouse-click", "--xy", "10,10"][..],
        &["press", "cmd+v"][..],
        &["press", "cmd+v", "--window-id", "w-1"][..],
        &["snapshot"][..],
        &["minimize", "--window-id", "w-1"][..],
        &["focus-window", "--title", "Inbox"][..],
    ] {
        assert!(
            targets(args).expect("gated").is_err(),
            "{args:?} lands in some app; a desktop that cannot say which must not yield system"
        );
    }

    let clipboard = facts(&["clipboard-set", "hello"]).expect("gated");
    assert_eq!(clipboard.app_or_system(), "system");
}

#[test]
fn a_drag_is_planned_at_both_ends() {
    let drag = command(&["drag", "--from", "@s0000000:e1", "--to", "@s0000000:e2"]);
    assert_eq!(plan_for(&drag).expect("gated").len(), 2);
    let pickup_only = command(&["drag", "--from", "@s0000000:e1"]);
    assert_eq!(plan_for(&pickup_only).expect("gated").len(), 1);
}

#[test]
fn notification_commands_use_the_expected_app() {
    let action = facts(&[
        "notification-action",
        "1",
        "Reply",
        "--expected-app",
        "Messages",
    ])
    .expect("gated");
    assert_eq!(action.target.app, "Messages");

    let dismiss = facts(&["dismiss-all-notifications", "--app", "Slack"]).expect("gated");
    assert_eq!(dismiss.target.app, "Slack");
}

#[test]
fn unresolvable_refs_leave_the_target_empty() {
    let click = facts(&["click", "@s0000000:e1"]).expect("gated");
    assert_eq!(click.command, "click");
    assert_eq!(click.target.app, "");
    assert!(click.mutating);
}
