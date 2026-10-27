# What Terminal Delight does to an idle agent's processes

Rest and sleep (`app/src/agentrest.rs`, `agentrest_io.rs`, `panegroup.rs`; plan in `docs/plans/idle-agents/`) give the window three powers over the processes in a pane that it did not have before. This page lists each one and what stands in front of it.

## 1. Moving a pane's processes into a group of its own

Every hosted pane's process tree is moved into a systemd transient scope, `td-pane-<session>-<pane>-<shell pid>.scope`, by `StartTransientUnit` / `AttachProcessesToUnit` on the **user** manager. No privilege is involved: the user manager owns `app.slice`.

- The pids come from the pane's own shell pid (the host's report) and the kernel's child lists under it. Nothing is found by name or by pattern.
- Consequence worth knowing: this machine runs systemd-oomd with swap and pressure kills on `app.slice`. Before, every pane, the window and the host shared one scope, so an oomd kill took all of them. Now a kill takes at most one pane.
- `TD_AGENT_REST=0` turns the whole feature off, groups included.

## 2. Freezing a pane (rest)

`cgroup.freeze = 1` on the pane's group, then `memory.reclaim` in 64 MiB steps.

In front of it, all must hold (`agentrest::may_rest`); any fact that cannot be read refuses:

- the pane is an agent (Claude or Codex), not focused, not pinned awake;
- untouched and silent for the rest threshold (10 min by default);
- Claude says `idle` in its own `~/.claude/sessions/<pid>.json`, and the screen does not contradict it; Codex shows its prompt;
- no process in the group other than the shell, the agent and the agent's MCP helpers — no command the agent ran, no shell job, nothing listening on a TCP port. A command is a Claude Bash tool shell, or anything under the agent that leads a session of its own, and their children: the agent's helpers share its session and its commands do not (measured on every agent pane on 2026-10-27, including the process behind a Codex "background terminal" left running after its turn). A process whose session cannot be read counts as a command;
- the group used under 3% of a core over the last look.

And after it: **every write to the pane goes through `panegroup::Gate::on_input` first, which thaws before the bytes go on**, so no keystroke, paste, mouse report or bench line can reach a frozen agent. Freeze and thaw share one lock, and a freeze is refused if anything was written since the policy looked. Focusing the pane thaws it too. A frozen group whose pane this window does not show is thawed on every tick, so a process can never be left frozen holding a hangup it cannot receive.

## 3. Ending an agent (sleep)

`kill(agent_pid, SIGTERM)`, once, after the agent has rested for the sleep threshold (2 h by default), or when a person chooses **Sleep now** from the pane's menu.

In front of it, everything rest requires, plus (`agentrest::may_sleep`, `agentrest_io::sleep`):

- a Codex only when it runs its own server, meaning it has MCP helpers of its own (`agentrest::helpers`). A Codex window attached to a shared Codex service has none, because the service starts them, so ending that window would save almost nothing;
- the conversation id comes from the agent itself, not from a guess: Claude's own registry, or the rollout a Codex holds open (`session::codex_held_session`; the id a pane was resumed with is not enough, since a `/new` may have moved it);
- for Claude, that conversation is saved where the wake will look, `~/.claude/projects/<slug of the pane's cwd>/<id>.jsonl`, and is not empty (`agentrest_io::conversation_saved`). Claude writes a conversation down only once something is said in it, so a Claude that was started and never spoken to, or was `/clear`ed into a fresh id, still rests but never sleeps. Before this check, on 2026-10-27, an idle Claude nobody had typed to was ended and its wake answered "No conversation found with session ID". A Codex's held rollout is its saved conversation;
- the launch line can be rebuilt exactly. For Claude (`agentrest::wake_argv`): no initial prompt that would be sent again, no option this build does not know, not a print or background run. For Codex (`agentrest::codex_wake_argv`): every option kept word for word and `resume <id>` added; the opening prompt is dropped, because it is already the conversation's first message; refused for an option this build does not know, a subcommand that is not an interactive pane, initial images, or `--worktree`;
- no write to the pane since the look; the pid is still in the pane's own group.

The signal goes to the agent pid alone, never to the shell, never to a group. If the agent has not exited five seconds later, nothing more is done — **never SIGKILL**, which could cut a transcript write — and the pane stays awake.

## Waking a sleeper

A key or a click in an asleep pane opens a fresh pane in the same place through the host's ordinary `SpawnPane` with recipe `bash '<wake file>'` — the same spawn-time path a session restore uses (site #8 in `terminal-input.md`). The window never types into the shell that held the sleeper; that shell is hung up. A wake is refused while any live Claude process reports the same conversation open, or any live process holds that Codex conversation's rollout open, so a conversation never has two writers.

The wake file, `~/.local/state/terminal-delight/wake/<session>.sh` (mode 0600), is the agent's own argv, shell-quoted, after `exec`.

## When a window quits, or dies

A window that quits thaws every frozen pane of its session before it goes (`panegroup::thaw_session`), so the next window — this build, or an older one that knows nothing of rest — finds every pane awake; a window with the feature rests them again after ten minutes. A window that crashes cannot do that. A window with the feature adopts what it finds frozen and thaws it on the first key; if an older build is started instead, thaw by hand:

```bash
for f in /sys/fs/cgroup/user.slice/user-$(id -u).slice/user@$(id -u).service/app.slice/td-pane-*/cgroup.freeze; do echo 0 > "$f"; done
```
