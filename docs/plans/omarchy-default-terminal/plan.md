# Plan: Terminal Delight can be Omarchy's default terminal

**Difficulty 6/10.** The flags themselves are a shape this codebase has built before
(`dispatch`, the quick window, a pane running a program instead of a shell). The risk is
where they land: a new launch path into the process that twenty live agent panes depend
on. A wrong guess there persists phantom panes or claims a session nobody asked for, and
nobody notices until the next restart. One gate: this page, one approval, then build.

## What it is for

0xSero's Local AI plugin for Omarchy runs a model on your own GPU or CPU and opens a
coding agent on it. It does not choose a terminal. It calls Omarchy's
`omarchy-launch-tui`, which hands the agent to whatever terminal `xdg-terminal-exec`
names as the default, tagged with the Wayland app-id `org.omarchy.local-ai`
(`bin/omarchy-local-ai:1279` at `fef7eb8`). Terminal Delight cannot be that default. It
has no `-e`, no `--app-id`, and its desktop entry carries none of the keys
`xdg-terminal-exec` reads. The issue saying so (#178) has been open since 2026-08-29.

Parker, on why this matters more than the $50 recipe bounty that surfaced it: *"The
connection is more interesting than $50."* The aim is a working demo of their agents
landing in Terminal Delight panes, sent to 0xSero, rather than a pitch.

## The shape

**What Omarchy sends.** `xdg-terminal-exec` reads the terminal's desktop entry and
translates its own flags into the terminal's, using four keys. We adopt foot's
spellings, which Omarchy already ships:

```ini
X-TerminalArgExec=-e
X-TerminalArgAppId=--app-id=
X-TerminalArgTitle=--title=
X-TerminalArgDir=--working-directory=
```

So Terminal Delight must accept, in any order before `-e`:
`--app-id=<id>`, `--title=<t>`, `--working-directory=<dir>`, and `-e <program> [args…]`,
where `-e` consumes everything after it. Nothing else changes: `terminal-delight`,
`terminal-delight <dir>`, every verb, `--help`, `--version` behave as today, and any
other flag is still refused with exit 2 and no window. That refusal exists because
unknown words once opened windows that each claimed a session.

**What it opens.** A contract launch is a **quick window** (`Workspace::new_scratch`).
That is the existing mode for Ctrl+Alt+T and torn-off panes. It never claims or writes a
session, never starts a session host, hides the left bar, and gets its own
`ctl-<pid>.sock`, so it cannot disturb the window you are working in. Its one pane runs
`<program> [args…]` directly as the pty child. That is the same lever the demo window
already pulls (`vt::pty::Options.shell`), not a line typed into a shell. When the
program exits, the window closes, as foot's does.

```rust
/// What an xdg-terminal-exec caller asked for. `None` everywhere is not a contract launch.
struct Contract {
    app_id: Option<String>,
    title: Option<String>,
    dir: Option<PathBuf>,
    exec: Option<Vec<String>>,   // argv after -e, never empty when Some
}

enum Launch {
    Verb(Verb),
    Reply { text: String, code: i32 },
    Window { open_here: Option<PathBuf> },
    Contract(Contract),           // new
}

/// argv[1..] → a contract launch, or the refusal text. Pure; unit-tested.
fn parse_contract(args: &[String]) -> Option<Result<Contract, String>>;

/// The mark a pane wears for the app-id it was launched under.
fn launch_mark(app_id: &str) -> Option<&'static str>; // "org.omarchy.local-ai" → Some("LOCAL MODEL")
```

`SpawnSpec` gains `program: Option<(String, Vec<String>)>`. The window's
`WindowOptions.app_id` becomes the contract's app-id, defaulting to
`terminal-delight`, and its title the contract's title. Neither is persisted, because a
quick window never saves.

**The mark.** A pane launched under `org.omarchy.local-ai` wears a `LOCAL MODEL` chip
in its header. That is all v1 knows: the app-id says *a local model*, not *which one*.

## Least confident decisions

1. **Quick window, not a hosted session.** A Local AI agent therefore dies if its window
   dies, unlike your main panes. The alternative, a hosted session per launch, mints a
   session file every time `btop` or a setup terminal opens. That is the pile-up the
   refusal guard was written to stop. Revisit if agents opened this way turn out to
   matter as much as the main fleet.
2. **The window closes when the program exits.** Foot does this and Omarchy expects it.
   The plugin's own setup terminal already holds itself open on failure, so nothing
   there depends on us holding.
3. **`--working-directory` with no `-e` opens a quick window with a shell in that
   directory.** That is what Super+Return would give you if Terminal Delight became the
   system default (`omarchy-launch-terminal` always passes `--dir`). This plan does
   **not** flip your system default. That is your call after you have tried it.
4. **The mark comes from the app-id only.** Naming the model would mean reading the
   agent's gateway key out of its environment and calling the gateway with it. Filed as
   a follow-up, not built tonight.
5. **No hold flag, no `--class`, no `--dir` alias.** `xdg-terminal-exec` translates
   `--dir` into `--working-directory=` for us. Anything a caller sends beyond the four
   keys is refused, which is how we will learn it exists.

## Slices

1. **Tracer bullet: the flags open a quick window running the program.**
   `terminal-delight --app-id=org.td.tracer --title=T -e sh -c 'echo ok > $MARKER; sleep 3'`
   opens a quick window whose Hyprland class is `org.td.tracer`, writes the marker, and
   is gone when `sh` exits. Unit tests on `parse_contract`: each flag, order
   independence, `-e` eating the rest including a later `--app-id`, `-e` with nothing
   after it refused, an unknown flag refused, `--help` and `<dir>` unchanged. Each test
   must fail against today's `dispatch`. Live proof on a hidden special workspace
   (`render_unfocused`, `no_initial_focus`), with the live session's file hash and
   `list_panes` captured before and after and shown unchanged.
2. **Through Omarchy's own launcher.** Desktop-entry keys in
   `packaging/terminal-delight.desktop`. Then the plugin's exact line,
   `omarchy-launch-tui --app-id=org.omarchy.local-ai bash -c 'cd -- "$1" && shift && exec "$@"' local-ai <dir> htop`,
   lands in a Terminal Delight window, using a scratch `XDG_CONFIG_DIRS` / `XDG_DATA_HOME`
   whose list and entry point at the worktree build. `~/.config` and
   `~/.local/share/applications` are never written.
3. **The mark.** `launch_mark` plus the header chip, with a test that a pane launched
   under another app-id wears nothing, and a hidden-workspace screenshot of the chip.
4. **The real plugin (only if pre-flight B is done).** Start their CPU recipe
   (LFM2.5-2.6B, 1.6 GB) with `bin/omarchy-local-ai`, run `open` with Claude Code, and
   confirm it arrives in Terminal Delight with the chip. The default-terminal lever is
   the same scratch env as slice 2, never your `~/.config`. Windows go to the hidden
   workspace unless pre-flight C says otherwise.
5. **Land and hand back.** One PR closing #178. A handoff in `handoffs/`, a sticky note,
   and the demo-clip command for **you** to run on your return, because no screen
   capture is automated. Then a learning brief for the 0xSero outreach into
   `~/Work/writers-block/inbox/`, since outreach is written by the writer.

Out of tonight's scope, filed as follow-ups: naming the model in the chip; the
four-line upstream PR adding Terminal Delight to `omarchy default terminal`; the AUR
package (#165).

## Pre-flight: what only you can do before you leave

- **A. Approve this page**, or change it.
- **B. Optional, for the real demo (about 2 minutes, one password):**
  `omarchy plugin add https://github.com/sybil-solutions/omarchy-local-ai --enable`,
  then **Set up Local AI** in the bar. That adds you to the docker group, which is
  root-equivalent, as their README and Omarchy both say, and installs
  `nvidia-container-toolkit`. Skip it and the run stops after slice 3 with the exact
  launch line proven. The plugin itself still gets exercised.
- **C. Consent for slice 4 windows:** hidden special workspace (default), or a visible
  empty workspace you name.
- **D. Merge policy:** merge on green (main is merge-on-green), or leave the PR open for
  you to read first (default).

## Run rules while you are away

Max three attempts on any one problem, then stop and write down what was tried (APES C3).
No screen capture, no `~/.config` writes, no system-default change, no installs beyond
what pre-flight B did. The installed build and the running window are not replaced.
Every slice ends committed and pushed on `feat/omarchy-default-terminal`, so a stop at
any point leaves a resumable branch.
