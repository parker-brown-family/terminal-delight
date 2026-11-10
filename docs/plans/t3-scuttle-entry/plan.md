# Plan: the entry point — an INBOX spine and a chat that launches agents

**Difficulty 6/10** — two new surfaces (a view of the left spine and a composer over the main
area) in a window whose keyboard and focus rules are strict, nine languages, and state a person
sets that has to survive a restart. Each surface is a shape this window already has (the rail's
queue, the LAUNCH AGENT panel), everything lives on `explore/t3-scuttle`, and unwinding it is not
merging the branch. That buys one gate. Parker waived the approval for this run: *"Feel free to
build build build - but keep it OUT O LAUNCH branches … I am fully afk"*.

## What it is for

Parker, 2026-11-07: *"the GOAL is to make the WORKFLOW and interface feel more and more and more
like a Web3 experience … we will have all the TERMINALs running, but what it feels like is an
ENTRY point … the left spine might toggle between PROJECTS and INBOX — the inbox being very
similar to what T3 has done with their left spine agent attention surface … THe MAIN surface
being a standard CHAT that activated the AGENT LAUNCHer (chooose model and effort)"*. Then, while
the work was running: *"Like the left spine being organized into SLEEP squitet --- done - allowed
to snooze is super cool - then the inbox of LOOK AT ME at the top"*.

T3 Code's argument, in Theo's words (the 2026-10-29 video): *"the sidebar is an inbox. Every time
you open a thread, the goal should be to close it."* Working threads fold away (*"If the thing is
working, get it out of my face. Don't bother me until it's ready"*), a thread with a question
pops back to the top, and a thread can be snoozed or settled.

Terminal Delight already ranks what wants a person (the attention rail) and already launches
agents (Alt+A). What it lacks is the inbox's verbs (snooze, done) and a front door that starts
from the sentence rather than from the project list.

## The shape

```
            ┌ PROJECTS │ INBOX ┐   ← the spine's header toggles; Ctrl+Shift+I
 LOOK AT ME 3 ─────────────────
 ● agent asks   CLAUDE · TD › T3 Scuttle      2m
   "Do you want to proceed?"                    ⏾ ✓
 ● finished     CODEX · OMARCHY › themes      14m
 ...
 (space)
 WORKING 5 ▸
 SNOOZED 2 ▸
 SLEEP & QUIET 14 ▸
 DONE 7 ▸
```

- `app/src/inbox.rs` — pure, no gpui. What the inbox knows about a pane (`Fact`), what a person
  said about it (`Store`: snoozes, done marks, woke marks, which shelves are open), and the one
  function that sorts every agent pane onto exactly one shelf:

  ```rust
  pub enum Shelf { LookAtMe, Working, Snoozed, Quiet, Done }
  pub fn partition(facts: &[Fact], store: &mut Store, tracker: &mut Tracker,
                   now: SystemTime, at: Instant) -> Partition
  ```

  LOOK AT ME keeps the rail's order (promoted, then lane, then oldest first): one ranking of a
  pane, never two. `Store` round-trips through `~/.config/terminal-delight/inbox/<session>.json`.
- `app/src/inbox_view.rs` — `impl Workspace`: builds the facts from the same per-frame projection
  the rail reads (`rail_rows`), draws the shelves inside `render_left_bar`, and carries the row
  verbs (focus, snooze menu, done, wake, reopen).
- `app/src/entry.rs` — pure. The composer's draft and the recipe it becomes
  (`launcher::Recipe` with the sentence as its `opener`), the optional machines list
  (`~/.config/terminal-delight/machines.toml`), the ssh line for a far machine, and the `gliff`
  command that shows its screen.
- `app/src/entry_view.rs` — `impl Workspace`: the composer as a glass page over the main area,
  its chips, its keys, and the launch (`adopt_pane` into a new tab, foreground or background).
- `app/src/lang/entry.rs` — every new word, in the nine languages, one table.
- Touch points in `main.rs`, `pane.rs`, `keylayer.rs`: two chords (Ctrl+Shift+I, Ctrl+Shift+E),
  the spine header's toggle, the composer in the render tree, the store's load and save.

## Least confident decisions

1. **LOOK AT ME is ordered like the rail, not like T3.** T3 sorts by when a thread came back,
   newest first; TD's rail sorts promoted, then lane, then oldest first, and that order has tests
   behind it. A pane ranked two ways in one window would be the defect the rail spent a slice
   removing.
2. **A row's identity is the host pane id**, not the conversation id. Session strings are not
   unique per pane (#416); the host pane id survives a window restart and an agent's sleep.
3. **DONE undoes itself on new activity**: a new question, a new finish, or starting work again
   after the mark. A person's "done" is about the turn they saw, not the next one.
4. **SNOOZED wakes early** for a new question, failure or access ask (T3's rule), and a snooze
   that runs out puts the row in LOOK AT ME marked WOKE until the pane is visited.
5. **The composer sits over the main area and never resizes a pane**; Enter starts the agent in a
   new tab and goes there, Ctrl+Enter starts it in the background and keeps the composer open for
   the next one (T3's `mod+Enter`).
6. **Chords**: Ctrl+Shift+I (inbox) and Ctrl+Shift+E (entry). Both were unclaimed.
7. **The farbox is opt-in**: no `machines.toml`, no machine chip. A far launch is
   `ssh -t <host> 'cd <same path under ~> && <agent line>'` in a local pane, and its screen opens
   with `gliff --headless <host>` in gliff's own window. TD reads nothing about a remote agent's
   state, so its row says unknown rather than idle.

## Slices

1. Tracer bullet: `inbox.rs` partition with tests; the spine's PROJECTS │ INBOX toggle; rows
   that focus their pane.
2. The verbs: snooze menu, done, wake, reopen; the store persisted; early wake; WOKE.
3. The composer: draft, chips (project, harness, model, effort, reach), Enter and Ctrl+Enter.
4. Keyboard: the two chords, arrows and keys inside the inbox, the composer's keys.
5. The farbox: `machines.toml`, the machine chip, the gliff action.
6. Nine languages, the gates (fmt, clippy `-D warnings`, the full suite), one adversarial review
   round from Codex, photographs from a hidden window, the brief.
