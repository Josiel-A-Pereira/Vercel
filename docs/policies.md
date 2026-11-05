# Owner policies (Dogwood)

agent-desktop can enforce owner-written [Dogwood](https://github.com/dogwood-policy/dogwood-local-engine)
policies on every CLI command. Dogwood is a superset of Cedar. Policies decide
which commands may run, against which applications, and under which
conditions, including conditions over what the agent already did, such as
"input only into an app snapshotted in the last ten minutes". The agent driving
the CLI cannot override them: they live in files the machine's owner controls,
and a refused command fails with `POLICY_DENIED` before it reaches the platform
adapter.

With no policy files, nothing changes.

```bash
mkdir -p ~/.agent-desktop/policies
chmod 700 ~/.agent-desktop ~/.agent-desktop/policies            # the state root must stay private
cat > ~/.agent-desktop/policies/guardrails.dogwood <<'EOF'
@id("allow-all")
permit (principal, action, resource);

@id("hands-off-secrets")
forbid (principal, action in Desktop::Action::"input",
        resource in Desktop::AppId::"com.1password.1password");
EOF
chmod 600 ~/.agent-desktop/policies/guardrails.dogwood
agent-desktop status                                            # data.policies.layers[0].installed is true
```

The user guide is [Owner policies](https://agent-browser.dev/desktop/guides/policies).
This page is the full reference.

## Where policies come from

Each source is a separate layer, and a command runs only if **every** layer
allows it:

1. The owner's folder, `~/.agent-desktop/policies/*.dogwood` in the
   account's home directory as the system's account records name it
   (`getpwuid_r` on Unix, the profile known folder on Windows). It always
   applies, whatever `HOME`, `USERPROFILE` or `AGENT_DESKTOP_HOME` say.
   When `AGENT_DESKTOP_HOME` relocates the state root, its `policies/`
   folder is a further layer.
2. `AGENT_DESKTOP_POLICY_DIR=<dir>`, when set. It must be an absolute path.
3. Each `--policy <file>` and each `--policy-dir <dir>` (repeatable global
   flags), every one a layer of its own.

A harness can add a layer to tighten what its agent may do. No layer can
lift another's refusal, so an agent that passes `--policy-dir` with a
permit-everything file is still bound by the owner's folder.

Files load in lexicographic order. The folders and files must be owned by the
current user and not writable by group or others, or the gate refuses them.
`agent-desktop status` lists every layer under `data.policies.layers`, with
its files, whether it installed, and the error when it did not.

A layer that fails to load or validate **fails closed**: mutating commands are
refused until it is fixed. Read-only commands still run so the agent can report
the problem, and the healthy layers still judge them, so a broken layer never
lifts the owner's rules.

Policies are judged by the CLI. `agent-desktop mcp` runs each tool call as a
CLI process, so every call is judged, and `--policy` or `--policy-dir` given
to `mcp` add their layers to every call. The C library
(`libagent_desktop_ffi`) does not run the gate.

## History

Every allowed command records its outcome, and temporal clauses read that
history. It lives in `policy-state/` under the state root (mode `0700`,
written only by the gate), apart from the policies folder, so locking the
policies folder never freezes the history. The owner's folder keeps its
history in the account's own `~/.agent-desktop/policy-state/`, found from the
account records like the folder itself, so relocating `HOME` or
`AGENT_DESKTOP_HOME` never hands the owner's rules an empty history.

- Each layer location keeps its own installed set and history, so a temporal
  clause sees only the commands its own layer judged.
- Editing a file installs only the difference. Every policy whose text did not
  change keeps its history, and so does adding a file to a folder. A policy you
  add or change starts with an empty history.
- History survives restarts. A process that closes a store checkpoints it
  once 64 events have gathered, and a one-shot CLI process closes its stores
  before it exits, so the log stays bounded even though every call is its
  own process.
- Several sessions and agents can run at once. They share one store per layer,
  and the process holding it hands it over when another asks. A single active
  session pays nothing extra; agents that interleave commands pay roughly one
  store reopen per switch, about 100 ms.

## The vocabulary

Policies are written against a built-in schema, generated from the command
table so it cannot drift from the CLI.

- **Principal**: `Desktop::Agent::"<agent-id>"`, from `--agent-id` or
  `AGENT_DESKTOP_AGENT_ID`, else `default`.
- **Action**: one per CLI command (`Desktop::Action::"click"`), grouped so a
  policy can say `action in Desktop::Action::"input"`:
  - `observe`: snapshot, find, get, is, wait, and the `list-` commands
  - `capture`: screenshot, clipboard-get
  - `input`: every ref action (click, type, set-value, scroll, ...), press,
    hover, drag, and the mouse and key commands
  - `app`: launch, close-app, the window commands, open-system-surface
  - `notifications`: dismiss-notification, dismiss-all-notifications,
    notification-action
  - `clipboard`: clipboard-set, clipboard-clear
- **Resource**: the target application, `Desktop::App::"Safari"`, named the
  way `list-apps` lists it running. Where the platform gives the app a stable
  identifier, the resource is also a member of `Desktop::AppId::"<id>"`: the
  bundle identifier on macOS (`com.apple.Safari`) or the desktop id on Linux
  (`org.gnome.Calculator`), as `list-apps --app <name>` prints it in
  `bundle_id`. Copy it exactly or write it in lowercase; another spelling
  matches nothing. Windows has none, and its app name is the executable's
  image name (`notepad.exe`). Display names follow the system language
  (System Settings is "Impostazioni di Sistema" on an Italian Mac) and an app
  can rename itself, so a rule that must hold names the identifier:

  ```cedar
  forbid (principal, action, resource in Desktop::AppId::"com.apple.systempreferences");
  ```

- **Context**: `context.input.command`, `.app`, `.app_id`, `.window`,
  `.role`, `.element`, `.secure`, `.headed` and `.mutating`. `app_id` is the
  stable identifier, or `""` where there is none. A password field keeps the
  canonical role `textfield`, and its secure-ness arrives as the boolean
  `secure`, mirroring the `secure` state on the ref.

`version`, `skills`, `status`, `permissions`, `session`, `trace` and
`cursor-overlay` are administrative and never judged. `batch` is not judged as
a wrapper; each of its entries is judged as it runs.

## What a command is judged as

Every target is resolved to the app itself before the verdict, so a rule holds
whatever the agent typed. `--app 1password`, `--app com.1password.1password`
and `--app 1Password` are all judged as the running 1Password. An app that is
not running is judged as the installed app the name would launch.

- Ref actions resolve the app, window title, role and element name from the
  ref's snapshot, and name the app by the process that owns it.
- Window-scoped commands (`snapshot`, `find`, `screenshot`, `resize-window`,
  `move-window`, `minimize`, `maximize`, `restore`, `focus-window`,
  `list-surfaces`) are judged as the owner of the window they act on: the
  `--window-id` window, the windows `focus-window --title` matches, or the
  focused window when neither is named, or the frontmost app when no window
  has focus. With `--app`, every listed window of
  that app is judged with its title, since the command may act on any of
  them, so a rule over `context.input.window` holds without `--window-id`.
- `snapshot` and `find` on a shell surface (`--surface dock`,
  `menu-bar-extras`, `taskbar`, `start-menu` and the other OS chrome kinds)
  are judged as the shell process that hosts it, such as the Dock, whatever
  `--app` names. `menu-bar-extras --app <name>` reads that app's own status
  item and is judged as that app. `open-system-surface` is judged as
  `system`: it answers with the surface's window id, not what the surface
  shows.
- Keyboard input (`press` without `--app`, `key-down`, `key-up`) is judged as
  the focused window's app, or the frontmost app when no window has focus,
  `press --window-id` as that window's owner, and
  `press --app` as every window of that app.
- Pointer input at coordinates (`mouse-*`, `hover --xy`) is judged as the app
  whose window is on top at that point. On macOS a point on the menu bar is
  judged as the frontmost app.
- A `drag` is judged at both ends, and both must be allowed.
- `notification-action` and `dismiss-notification` are judged as the app
  their `--expected-app` (or, for `dismiss-notification`, `--app`) names;
  dispatch refuses the notification when it is from another app. With only
  `--expected-title`, the app is unknown and the command is refused with
  `dogwood_policy_target_unknown`. `list-notifications` and
  `dismiss-all-notifications` without `--app` reach the notifications of
  every app, so they are judged as every running app.
- Only what lands in no application window (the desktop, the clipboard,
  `list-displays`, a full-screen `screenshot`) acts on `Desktop::App::"system"`.

When the receiving app cannot be determined, the command fails closed with
`POLICY_DENIED` and `details.kind: "dogwood_policy_target_unknown"`. That
happens when the running applications cannot be listed, or, on Linux, when
windows of two apps overlap the point and their stacking order cannot be read.
Act through a ref, or name the app with `--app`, instead.

## Example

```cedar
// ~/.agent-desktop/policies/guardrails.dogwood

@id("allow-all")
permit (principal, action, resource);

// forbid always overrides permit.
@id("hands-off-secrets")
forbid (principal, action in Desktop::Action::"input",
        resource in Desktop::AppId::"com.1password.1password");

@id("never-secure-fields")
forbid (principal, action in Desktop::Action::"input", resource)
when { context.input.secure == true };

@id("no-notification-sweeps")
forbid (principal, action == Desktop::Action::"dismiss-all-notifications", resource);
```

Dogwood is deny-by-default: a command no `permit` matches is refused. A
stricter owner deletes `allow-all` and permits what agents may do instead,
including `observe` unless reads should be refused too.

### Rules over history

A `when temporal` clause judges a command by what came before it. Every allowed
command's outcome is recorded (`response` on success, `error` on failure), so a
policy can demand observation before action. The layer must not also hold a
blanket permit for input such as `allow-all` above, or that permit alone lets
every input through:

```cedar
@id("allow-reads")
permit (principal, action in [Desktop::Action::"observe", Desktop::Action::"capture"], resource);

@id("observe-before-act")
permit (principal, action in Desktop::Action::"input", resource)
when temporal {
    formerly within 10m Desktop::Action::"snapshot"::response{ input.app: context.input.app }
};
```

An agent holding a ref into Safari can then click it only if a snapshot of
Safari succeeded in the last ten minutes. Match on `input.app` here, not
`input.app_id`: on Windows `app_id` is always empty, so two different apps
would compare equal.

## A refused command

```json
{
  "version": "2.6",
  "ok": false,
  "command": "type",
  "error": {
    "code": "POLICY_DENIED",
    "message": "'type' on '1Password' is forbidden by policy hands-off-secrets",
    "suggestion": "The machine owner's Dogwood policies (the state root's policies folder) refused this; no flag or variable lifts them. Report it to the user",
    "details": {
      "kind": "dogwood_policy",
      "layer": "the state root's policies folder",
      "policies": ["hands-off-secrets"],
      "evaluation_errors": [],
      "retryable": false
    },
    "disposition": { "delivery": "not_delivered", "retry": "safe" }
  }
}
```

A refusal carries no `recovery` strategy: repeating the command, or waiting,
gets the same answer. `details.layer` names the refusing layer, and the
suggestion changes with it. [JSON output](json-output.md#policy_denied-kinds)
lists every `details.kind`.

## Trust boundary

The gate and the agent run as the same user. This is a seatbelt against a
misbehaving or prompt-injected agent using agent-desktop, which is the
realistic failure, not a sandbox against a hostile process. A process with a
shell can rewrite the policies folder, run a patched binary, or load the C
library, which does not check policies. Relocating `HOME` or
`AGENT_DESKTOP_HOME` does not escape them. Anything else that runs
agent-desktop as the owner, test suites included, runs under the owner's
folder too.

For a stronger stance, make the policy files immutable (`chflags uchg` on
macOS) and leave `~/.agent-desktop/policy-state/` writable. A `--policy-dir`
owned by root fails the ownership check on purpose, so locking files in place
is the way to keep them out of the agent's reach. Keep your own audit log if
you need one: the engine's log is pruned and does not record verdicts.

An edit to a policy file applies on the next command, including the next
entry of a running `batch`.
