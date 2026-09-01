# Handoff — the bell comes back without its config (2026-09-01)

## Status

**#227 merged**; deployed `~/.local/lib/terminal-delight/td-e82dd33`. Zero open
PRs. 403 tests green. Also this session: #222 (reader zoom cap 1.6×), #223
(paging keys + URL copy chips), brown-family-sports#7 (git push identity race —
filed, not fixed, one-off workaround below). Issue #225 (reader heals
pre-resize narrow history) was **closed by Parker's decision**: the fix moved
upstream to `~/.config/agents/AGENTS.md` (agents stop producing hard-broken
narrow content); the reader-side heuristic stays unbuilt; reopen only if
post-2026-09-01 sessions still produce unhealable crunch.

## What the bell is now

Unplugging the bell (old `bell::ENABLED = false`) killed notifications when the
target was only the CONFIG overhead. The resurrection inverts the shape — the
whole config surface is deleted and the feature is always on for agent panes:

- **Ping**: the 0.3s `alert.mp3` is `include_bytes!`-embedded (2KB), written
  once to `$XDG_RUNTIME_DIR/terminal-delight-ping.mp3`, played via `ffplay`
  (fallback `pw-play`), fixed volume, never loops. A bare deployed binary
  rings with zero install.
- **Badge**: tab 🔔 + header `● done` until acknowledged. **Ack = the pane's
  focus-in edge** (click, alt+arrows, the notification jump). No SNOOZE bar,
  no card, no toggle, no tray. Deleted: `BellConfig`, sound picker, zenity
  import, trim scrubber, loop/volume, seeding, ffprobe. Net −681 lines.
- **System toast** (`notify.rs`): fires only when the finish lands where
  Parker is NOT looking (window inactive, or another pane/tab focused —
  watching auto-acks silently). Title `tab → pane`, body = recap parsed from
  the agent's own transcript (Claude JSONL last assistant text; Codex path
  wired; grid tail fallback). Click → notify-send prints `default` → TD
  raises its window over the Hyprland socket, parks `pending_jump`, and the
  next render frame activates the tab + focuses the pane (which acks).

## Environment facts (probed live, do not re-derive)

- Notification daemon is **Quickshell** (`/usr/share/omarchy/shell`), owns
  `org.freedesktop.Notifications`, capabilities include **actions**.
- Omarchy's Hyprland fork speaks **Lua** on `.socket.sock`:
  `dispatch hl.dsp.focus({window="pid:N"})` works (`ctl::focus_this_window`,
  classic `dispatch focuswindow pid:N` kept as fallback). `j/clients` etc.
  unchanged.
- **Push identity race** (bfs#7): `credential.helper` in `~/.config/git/config`
  uses the RAW mise gh → shared config → whichever account a concurrent
  session last `gh auth switch`ed (it was `parker-nothing-nord`; push → 403).
  Workaround used per push:
  `git -c credential.helper= -c 'credential.helper=!GH_CONFIG_DIR=/home/parker/.config/gh-accounts/brown /home/parker/.local/share/mise/installs/gh/2.98.0/gh_2.98.0_linux_amd64/bin/gh auth git-credential' push …`
  Durable fix belongs in the includeIf profiles — coordinate with the
  identity-rework owner (see the issue).

## First hands-on checks after restart

1. Start an agent turn in a pane, switch to ANOTHER tab → on finish: ping +
   🔔 on the source tab + a toast `tab → pane` with a recap.
2. Click the toast → TD window raises, right tab activates, pane focused,
   badge gone.
3. Finish a turn while watching that pane → ping only; no toast, no badge.
4. Focus a badged pane by click/alt+arrows → badge clears without the toast.

## Watch out

- The notify-send child blocks a background-pool thread for its 12s toast
  lifetime (same trade the old zenity import made). Many simultaneous
  finishes = many parked threads; fine at Parker-scale, revisit if ever
  pooled out.
- `TermEvent::Bell` from a NON-agent pane is still deliberately ignored
  (readline's failed-tab-complete BEL must not badge).
- The recap reads the transcript tail (256KB) OFF the UI thread; a torn first
  JSONL line parses as garbage and is skipped by design (tested).
- Concurrent-session note: another agent landed #224/#226 (theme) and pulled
  main under this session mid-surgery; WIP was committed to a branch early.
  Check `git status` before building — still the rule.

## Where it's recorded

- PR: #227 (this feature) · #222/#223 (earlier today, same session)
- Issues: TD#225 (reader narrow-history heal), brown-family-sports#7 (push race)
- Central agent file: `~/.config/agents/AGENTS.md` (new today — TD-aware
  output formatting; CLAUDE.md imports it, codex/gemini symlink it)
