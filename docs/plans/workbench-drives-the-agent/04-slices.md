# Slices: The workbench drives the agent

Vertical, in build order. Parker was AFK, so the approvals that would sit
between these did not happen; each slice was instead gated on the CI checks
(`cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test`)
and on the adapter's own suite, and what each one proves is written beside it.

| # | Slice | State | Proof |
|---|---|---|---|
| 1 | **Tracer bullet — the composer is a document and SEND is one paste.** `Line` gains breaks, rows and undo; `line_edit` takes shift; the talking branch stops calling `keystroke_bytes`; `bench_send` journals a `say` and delivers a bracketed paste. `ctrl+c` copies. | **built** | `shift_enter_is_a_line_and_enter_alone_is_the_send`, `a_draft_takes_its_changes_back_in_order`, `a_say_is_one_bracketed_paste_…`; the `if talking {` branch has no byte-writing call |
| 2 | **The inbound journal and the hook adapter.** `channel::Inbound`, `Feed::tail_inbound`, `td-agent-hooks` for five events, the installer. A question arrives whole, `Origin::Hook`, one card per question with the round on each; `needs_input` lights with no picker. | **built** | the adapter's ten node tests; `every_inbound_type_parses_…`; `a_round_becomes_one_card_per_question_…`; `the_inbound_journal_is_read_by_offset_…` |
| 3 | **The answer goes back through the hook.** The marker, the wait, the answer file, the three roads with the keys road journaled, the screen reader merging its cursor onto the hook's card instead of presenting a twin. | **built** | `an_answer_takes_the_first_open_road`, `a_press_records_first_…`, `once_the_picker_has_painted_…`, the adapter's *bench open → pre-answer* and *stale marker* cases; the pseudoterminal run against Claude Code 2.1.274 (`ANSWER=Coffee`, picker never painted) |
| 4 | **The person's words and the agent's reply, from the harness.** `prompt` captions the overview exactly and retires the screen latch; `reply` becomes a `response` when the agent presented none. | **built** | `a_reply_is_presented_only_when_the_agent_presented_nothing_itself`; the latch guard in `latch_asked` |
| 5 | **Install and prove on this machine.** Run `scripts/install-agent-hooks.sh`; build the release binary; a named install (`td-<sha>-decouple`) beside the current one, **symlink not taken** — Parker decides what his next window runs. | see `00-status.md` | the install log, and the binary's path |
| 6 | **Move `Bench` out of the pane.** A window-level map keyed by the tenancy pane's `ConvKey`, `TerminalView` holding a handle; the 124 `self.bench` sites become one accessor. | **held** — next pane | starts after `workbench-follows-the-agent` lands its key; measured seam in `00-status.md` |
| 7 | **The strip's stop control and the kill rung.** `Hit::EndAgent` already journals `end`; an `interrupt` control on the strip, and `Request::KillForeground` (#535) when the host is next upgraded. | **held** | #535 |
| 8 | **Codex and Gemini adapters.** Drive each harness's hook system against the same journal; a harness that cannot pre-answer still delivers its questions whole. | **held** | — |
| 9 | **Provenance on the overview caption and the reply card** (*as the screen showed them* vs the exact words), and a *via* word on question cards. `Origin::label` already draws it on every card; the caption does not yet say which record it came from. | **held** — small | — |

## What a fresh session must know

- The tree is `~/Work/td-decouple`, its own `target/`, cut from `origin/main`
  at `4b1fb2b` with the Gate 1 pane's seven pushed commits merged in. Build
  with `CARGO_TARGET_DIR=/home/parker/Work/td-decouple/target` from `app/`.
- `scripts/td-agent-hooks.test.mjs` runs with `node --test` and takes about
  eight seconds, because two cases wait on real clocks.
- The hooks are wired into `~/.claude/settings.json` by the installer, which
  keeps a backup and has `--uninstall`. The pre-existing `askhook.log` logger
  on the same matcher is Parker's and was left alone.
- `docs/spec/td-agent-channel.md` is the contract; a field added for a bug is
  a version bump there too.
