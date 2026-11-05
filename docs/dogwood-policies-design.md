# Design: Dogwood policies for agent-desktop

Status: implemented on this branch — see [policies.md](policies.md) for usage.
Deviations from this proposal: the gate hooks `dispatch::dispatch` (not a
third preflight stage) so batch items are covered on every platform; policy
paths travel on `CommandContext`; rules live flat at
`~/.agent-desktop/policies/` with the engine's state in a separate
engine-owned `policy-state/`; the `agent-desktop policy` subcommand and
a user schema extension are not in v1.

## Goal

Let the machine's owner declare, in files they control, which desktop actions
agent-desktop may perform — independently of whatever agent is driving the CLI.
Examples of rules an owner should be able to express:

- Never act on the password manager, banking apps, or System Settings.
- Never type into a secure text field, anywhere.
- Allow input actions only into an app the agent snapshotted in the last 30 seconds
  (observe-before-act, enforced rather than suggested).
- Never `dismiss-all-notifications`; never `clipboard-get` unless the agent itself
  did a `clipboard-set` earlier in the session.
- Rate-limit mutating actions (e.g. at most 20 clicks per minute).

[Dogwood](https://github.com/dogwood-policy/dogwood-local-engine) fits this well:
it is AWS's Cedar-superset policy language for governing agent actions, Apache-2.0,
with a pure-Rust embeddable engine (`dogwood-local-engine` + `dogwood-language`,
redb-backed). Cedar semantics carry over — deny-by-default, `forbid` overrides
`permit` — and `when temporal { … }` clauses can condition a verdict on the ordered
history of past request/response events, which is exactly what observe-before-act
and rate limits need.

## Where enforcement goes

The pipeline already has a single choke point: `execute_with_adapter` in
`src/main.rs` runs `command_policy::preflight` / `preflight_context` before
`dispatch::dispatch`. Every command — including each item of a `batch` — passes
through it. The gate slots in as a third preflight stage:

```
parse → permission preflight → context preflight → [dogwood request event → verdict]
      → dispatch → [dogwood response event with outcome]
```

- **Request event, pre-dispatch.** Build an `Event::builder(action, "request")`
  from the command, submit it, and read `Outcome::Decision(verdict)`. On deny,
  return a new `ErrorCode::PolicyDenied` with `disposition.retry = never`, the
  denying policy's `@id` in `error.details`, and a suggestion pointing at the
  policy file. The command never reaches the platform adapter.
- **Response event, post-dispatch.** Submit a `"response"` event carrying the
  outcome (`ok`, error code, target app). Temporal clauses evaluate against these.

### Engine placement

Every command runs in its own process and opens the durable store directly.
redb takes a single-process lock, and concurrent commands and sessions all
share the store. Opening it is not cheap: measured on an M-series
release build, about 4 ms for redb, 12 ms for engine recovery, 35 ms to reload
the installed policies, and 16 ms for redb to close. So a process keeps the
store open while nobody else needs it, guarded by a cross-process file lock; a
process that wants it keeps a `store.wanted` marker fresh, and the holder's
watcher closes the store between decisions to hand it over. One active session
costs what a held store costs (about 9 ms over no policy); two sessions
interleaving every command cost about 100 ms per switch.

The durable store lives under the existing state root:
`~/.agent-desktop/policy-state/<source-set>/` (or under
`$AGENT_DESKTOP_HOME/policy-state/`), one store per distinct set of policy
files so agents under different sources never reinstall over each other,
created `0700` like the rest of the state tree. It sits apart from the
user-edited `policies/` folder on purpose: the engine must keep writing the
log, so an owner can lock the rules without freezing it.

## Entity model

One namespace, `Desktop`. Schema ships built in (generated from the `Commands`
enum so it can never drift from the CLI); a user schema file may extend it.

- **Principal** — `Desktop::Agent::"<id>"` from `--agent-id` /
  `AGENT_DESKTOP_AGENT_ID`, else `"default"`. Multi-agent sessions already
  require an agent id for mutating commands, so policies can distinguish agents.
- **Action** — one Dogwood action per CLI command, grouped for convenience:
  - `Desktop::Action::"observe"` ⊇ snapshot, find, get, is, screenshot, list-*
  - `Desktop::Action::"input"` ⊇ click, type, set-value, press, drag, mouse-*, …
  - `Desktop::Action::"app"` ⊇ launch, close-app, focus-window, resize-window, …
  - `Desktop::Action::"clipboard"`, `Desktop::Action::"notifications"`, …
- **Resource** — the target application, `Desktop::App::"<name>"` by the
  name the platform lists it running under, and a member of
  `Desktop::AppId::"<id>"` when the platform has a stable identifier (macOS
  bundle id, Linux desktop id; Windows has none). Display names are
  localized and app-controlled, so durable rules use `resource in
  Desktop::AppId::…`. Every target is canonicalized before the verdict:
  `--app` text goes through the platform's own app matching (any letter
  case, bundle id, verified aliases), then the new `find_installed_app`
  capability for an app that is not running; refs and windows are named by
  the owning process via the cheap `list_app_identities` capability. For ref
  actions the app is resolved from the ref's snapshot in the `RefStore`
  *before* the verdict — refs are already bound to a snapshot of a specific
  app window, so this is a lookup, not a guess. Window-scoped commands
  without `--app` are judged by the window they act on. Keyboard input with no named
  target is judged as the focused window's app, and pointer input at
  coordinates as the app whose window is on top at the point (the new
  `window_at_point` adapter capability: CoreGraphics front-to-back on macOS,
  `WindowFromPoint` on Windows, single-app overlap on Linux), both read at
  verdict time; a drag is judged at both ends. A receiver that cannot be
  determined fails closed. Machine-level commands act on
  `Desktop::App::"system"`.
- **Context** — request context carries what the schema declares per action:
  target role and element name (`secure-text-field` matters), window title,
  headed vs headless, text length for `type`/`set-value` (never the text itself
  — typed text may be a secret and the event log is durable).

## Policy discovery

Every source is its own layer and a command runs only if every layer
allows it (files within one layer are concatenated). A first-match design
let an agent replace the owner's rules by passing its own `--policy-dir`,
and a broken layer that failed open for reads lifted the owner's read
rules; layers that only ever add close both.

1. The account's folder, always applied: `~/.agent-desktop/policies/`
   under the home directory the system's account records name, never
   `HOME`, `USERPROFILE` or `AGENT_DESKTOP_HOME`, which the caller sets.
   A relocated state root's `policies/` folder is a further layer.
2. `AGENT_DESKTOP_POLICY_DIR` environment variable.
3. `--policy <file>` (repeatable) and `--policy-dir <dir>` CLI flags.

```
~/.agent-desktop/
  policies/*.dogwood      # policy files, lexicographic load order
  policy-state/<set>/     # redb event log + snapshots per policy-file set (0700, engine-owned)
```

A user schema extension (`schema.dogwood-schema` beside the policies) is
reserved for a later version.

Rules:

- No policy files found → the gate is a no-op (today's behavior, zero overhead).
- Any policy present → load it; a parse/validation failure **fails closed** for
  mutating commands (`cmd.is_mutating()`) and reports the error with the file
  and line, per the upstream guidance that a dropped policy-update error must
  never look like success. Read-only commands still run so the agent can report
  the problem.
- Same ownership checks the project already applies to `TMPDIR` and
  `AGENT_DESKTOP_HOME`: policy files and directories must be owned by the
  current user and not group/world-writable, else refuse to load.

## Example policy file

```cedar
// ~/.agent-desktop/policies/guardrails.dogwood

// Reads are always fine.
@id("allow-observe")
permit (principal, action in Desktop::Action::"observe", resource);

// Mutating actions are allowed by default…
@id("allow-mutations")
permit (principal, action, resource);

// …except where forbidden. forbid always overrides permit.
@id("hands-off-secrets")
forbid (principal, action in Desktop::Action::"input", resource)
when { resource in [Desktop::AppId::"com.1password.1password",
                    Desktop::AppId::"com.apple.Passwords",
                    Desktop::AppId::"com.apple.systempreferences"] };

@id("never-secure-fields")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.secure == true };

@id("no-notification-sweeps")
forbid (principal, action == Desktop::Action::"dismiss-all-notifications", resource);

// Temporal: input only into an app observed in the last 30 seconds.
@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    since within 30s
    Desktop::Action::"snapshot"::response{ output.app: context.input.app }
};
```

A stricter owner deletes `allow-mutations` and enumerates `permit`s instead;
deny-by-default then covers everything unlisted.

## Honest trust boundary

The engine issues verdicts; agent-desktop enforces them. Both run as the same
user as the agent, so this is a **seatbelt against a misbehaving or
prompt-injected agent that uses agent-desktop**, not a sandbox against a
malicious process: an adversary with shell access can rewrite
`~/.agent-desktop/policies/` or run a patched binary. Worth doing anyway — the
realistic failure mode is an LLM being talked into a bad action through the
tool it was given, and that path this closes. Documented hardening steps for
owners who want more:

- Point `--policy-dir` at a root-owned directory (`/etc/agent-desktop/policy.d/`
  is honored when present and passes an owned-by-root, not-world-writable check),
  or `chflags uchg` the policies folder (never `policy-state/`, which the
  engine must keep writing).
- Policy updates are appended to the engine's own log in sequence with events,
  so tampering at the engine level is at least visible; a separate audit log of
  verdicts (the engine does not log decisions) should be written under
  `policy-state/audit/`.
- The upstream README's warning stands: the reference interpreter is not a
  hardened production enforcement point. Treat this as defense in depth.

## Implementation sketch

1. New crate `crates/policy` (`agent-desktop-policy`), depending on
   `dogwood-local-engine` / `dogwood-language`; core stays free of the
   dependency. Exposes `PolicyGate::load(sources) -> Option<PolicyGate>`,
   `gate.check(&CommandFacts) -> Verdict`, `gate.record_outcome(&CommandFacts,
   &Result)`.
2. `CommandFacts` built in `src/command_policy/` from `Commands` + resolved
   target (ref → app via `RefStore`, frontmost app for global input). This is
   the only mapping that needs per-command care; generate the action table from
   the same match that `policy_for` uses so new commands can't skip the gate
   (compile error on a missing arm).
3. Wire into `execute_with_adapter` after `preflight_context`.
4. New `POLICY_DENIED` error code in `docs/json-output.md`, exit code non-zero,
   `retry: never`, `error.details.policy_id`.
5. `agent-desktop policy` subcommand: `status` (files loaded, policy ids),
   `lint` (validate against schema), `test <command> --app <X>` (dry-run
   verdict without acting).
6. Tests: unit tests per mapping arm; conformance test that a denied click
   leaves no trace in the adapter; temporal test for observe-before-act using
   the real engine against a temp store.

## Open questions

- Should `screenshot` and `clipboard-get` count as observe (privacy-sensitive
  reads an owner may want to forbid)? Proposal: group them separately
  (`Desktop::Action::"capture"`) so they are individually addressable.
- Dogwood currently lacks day-of-week/holiday accessors; time-of-day rules are
  possible, calendar rules are not — don't promise them.
- Whether to feature-gate the dependency (`--no-default-features`) for builds
  that will never use policies.
