# Changelog

All notable changes to terminal-delight are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it
reaches 1.0. Until then, `0.x` minor bumps may include breaking changes.

## [Unreleased]

### The Threshing Floor for 0.5.0 Development

- Pane pool cards show each agent's state with the counter's glyph and color.
  A Go to pane button below the org action closes the Floor and focuses that
  pane, including sleeping agents and shells.
- The System Map uses the org view's Actor → CAN → Targets builder. Drag a
  whole org from its frame or header tab, or drag an agent card, into a slot.
  Map cards show the agent name and vitals; cross-org lines appear as one wire
  per directed org pair with the strongest rung and a count.
- Mouse Back opens the System Map from an org and mouse Forward returns. Orgs
  can be dragged into a saved arrangement, with Arrange restoring automatic
  packing. Cross-org arrows follow their endpoints as orgs move.
- The shared agent launch briefing opens with loose cues for choosing between
  an existing TD teammate, a native subagent where allowed, and doing the work
  directly. Agents can weigh useful context and independent work against the
  handoff, without a routing score, mandatory team sweep or written explanation
  for each choice. Current Floor permissions still govern contact with teammates.
- Each press of the target **+** creates a visible empty card slot and scrolls
  it into view. Choose a card or drop it onto a slot to fill it; empty slots
  can be selected or removed, and Submit waits until every slot is filled.
  The empty-slot prompt is localized in all nine packs, and the relationship
  builder's Hindi labels now read in Hindi.
- The pane pool starts beside the relationship builder and keeps its own
  full-height column, with more compact cards. The chart supports Ctrl+wheel
  zoom around the pointer, dragging its empty surface to pan, and a reset view.
  Chart cards, connectors and relationship labels move and scale together;
  the builder and pool keep their size. The add-target button stays visible
  beside the target cards when several are selected.
- Relationships are built in a persistent Actor → Can → Targets area above the
  Floor. Dropping one card onto another or Alt-dragging a line fills the builder;
  targets are chosen on the chart and in the filtered pane pool. Submit saves
  every target together and adds staged pool cards to the org. Cancel leaves
  membership unchanged. Related controls are grouped with more space.
- The War Room is now called the Threshing Floor, everywhere a person reads it:
  the panel, its help row, the docs and the site, in all nine languages. The
  name is meant to be calm: a threshing floor is where a harvest is sorted.
- Orgs saved before the rename load unchanged, and are still saved under the
  same keys, so a build on either side of the rename reads the same window
  state. The ask record is now written under `threshing-floor/<session>/`; the
  record older builds kept under `warroom/<session>/` is still read and counted,
  and never moved.
- `TD_WAR_ROOM_DEMO` is now `TD_THRESHING_FLOOR_DEMO`.
- A line can now reach `prompt`, a rung of its own above `ask`. Along a prompt
  line, `prompt_agent` gives another agent work: the words are typed into its
  own conversation as its next turn, once it is at rest at its prompt, and
  `prompt_status` reports the stages and the worker's last message. A
  coordinating agent can now treat the other agents in the window as its
  sub-agents, Claude Code and Codex alike. A worker that is working is waited
  for; a resting or asleep one is woken; a permission prompt, a dialog or a
  person typing is never typed into, and Enter is pressed once, only after the
  paste shows. Ask lines and rail bypasses never prompt, read-only panes are
  never prompted, and lines drawn before this stay ask lines. The ask rung now
  reads "ask" rather than "prompt".
- An optional prompt key stays bound to its first assignment after detailed
  status expires or the window reopens. A retry returns `expired` without
  typing again; a new key follows the existing prompt line and worker checks.
  The lasting receipt holds no prompt, reply or raw key.
- Prompting waits longer and holds nothing back. A worker must sit settled at
  its prompt for five seconds, not two, before it is typed into; a pane its
  person typed in waits a minute, not twenty seconds; and Enter comes a second
  after the paste shows. Words a failed prompt left unsent no longer stop the
  worker taking prompts: the next one is pasted after them.
- An agent Terminal Delight starts is told about the Threshing Floor in its
  briefing: what a line is, how to read where it stands, how to answer a prompt
  from another agent, and how to hand work out (a work order with how it will
  be judged done, another try when a reply falls short, and its person told
  after two or three). The new `floor_position` tool answers where an agent
  stands, including which panes may prompt it.
- An agent the sleep ends is always recorded asleep. One slower than five
  seconds to exit used to be ended with its pane left a plain shell that no
  key or prompt would resume; the window now follows it up and marks the pane
  asleep once it has gone, and leaves it awake if it is still running a minute
  after the signal. A worker woken from sleep for a prompt is told, under the
  prompt's header, that its old process's shells and session permissions are
  gone.
- A pane rebuilt from a fresh snapshot, and an asleep agent woken into a fresh
  shell, keep their Threshing Floor seat and the read-only or off-limits mark a
  person set. Before, both came back unseated, and an off-limits pane came back
  open to agents.

### Agent Memory Hover for 0.5.0 Development

- MENU GLYPHS includes a saved 🤖 Agents shortcut toggle. The toolbar button
  opens Agents settings; CONFIG keeps the same entry when the glyph is hidden.
  Standard and All show it, while Clean folds it into CONFIG.
- Hover the top-right agent counter to see RAM and swap for awake, resting and
  sleeping panes, the observed RAM decrease since rest, measurement coverage
  and sample age. The card uses cached measurements and keeps the Threshing Floor click.
- Unavailable process lists, swap readings and comparison baselines remain
  unmeasured. Partial totals say how many panes were measured. Memory figures
  use binary units (KiB, MiB and GiB); shared services outside pane scopes are
  excluded.

### Agaro Permission Follow-Through for 0.5.0 Development

- Agents can propose opening an off-limits pane without learning whether the
  supplied PID exists. Real, missing and already-open targets occupy the same
  one-request queue. Only a person's Yes can change access; a missing or
  replaced target cannot acquire a stale approval.
- A paired MCP/control-socket regression test preserves the existing refusal
  of unbound document opens. Ordinary windows require UI actions for commands
  that act as the person; scripted rigs retain their explicit launch opt-in.
- MCP documentation now describes read-only tool hints, sticky-note writer
  provenance and the ordinary-window control-socket fence.

### Upgrading from 0.4.0 (0.5.0 development)

- **Markdown review notes now keep their source evidence.** A new note records the
  quotation, neighbouring passages and SHA-256 of the document as it was read.
  Notes stores advance to format 2 on the next successful save. Existing notes
  remain readable, with their unavailable evidence labelled; no old quotation or
  revision is invented. Older builds that only understand format 1 refuse to
  overwrite an upgraded store. Keep the notes folder when backing up TD.
- **Artifact captures use local storage.** New artifact handovers can save one
  content-addressed copy per local revision under the workbench store's
  `artifact-revisions/` directory. Capture is limited to regular files of at most
  16 MiB; remote links, symlinks, special files and unavailable bytes remain links
  with an explanation. Linked assets are not copied. Existing history is indexed
  without pretending today's bytes were delivered in the past. No automatic
  retention limit deletes saved revisions in this version.
- **Existing terminal hosts must restart before using the new cleanup code.**
  Replacing a window preserves the old host and its live terminals. Artifact and
  note changes take effect in the new window; hosted process cleanup takes effect
  in hosts started from this build. Ordinary client disconnection still leaves
  terminals running.
- **First launch opens in Clean; the other designs default to Ops and Standard.**
  Standard keeps the artifact, palette and slider glyphs and hides counts, scale,
  split buttons, graveyard, notifications, bug report and plugins. `all` still
  shows every control. Existing skin pins, UI choices and per-design changes
  remain authoritative. Fresh windows use ANSI program colours, Markdown syntax
  and desktop palette inheritance, with bottom anchoring, contrast sticky notes
  and terminal-state tint off. A saved new-window template still takes priority.
- **DESIGN and SKIN move independently; Tactical and Console are retired.**
  Picking a design, resetting one, or painting DESKTOP never changes the skin.
  A window that never picked a skin keeps the one it was drawing when its
  design changes. Designs no longer remember a skin, so a saved per-design skin
  is ignored and no longer lights a design's dot. A saved Tactical design opens
  as Hacker, a saved Console skin as Ops. The DESIGN and SKIN rows no longer
  offer `custom`; `theme.toml` and `skin.toml` still load, `ctl skin custom`
  still selects the skin file, and a window already wearing either still shows
  its tile.
- **`open_document` with placement `here` can answer `held`.** When a square
  is already open over the calling pane, nothing opens, the file becomes the
  pane's handover, and the answer begins `held` (MCP: a success with
  `"held": true`). It used to replace the square, or answer `err` when the
  square held unsaved notes. A click, the rail and `ctl doc here` still
  replace the square, since a person asked.
- **`ctl mcp …` no longer changes MCP policy from a script.** `ctl mcp on|off`,
  `ctl mcp writes on|off` and `ctl mcp expose agents|all` now answer `err` and
  change nothing, unless the window was started with `TD_MCP_SCRIPTED=1`. A
  keybinding or bar widget that used them should open the Threshing Floor (the agent
  counter, top right) instead; a script that launches its own window sets the
  variable where it launches it. `ctl mcp status` still answers.
- **`list_panes` says `reach` where it said `exposed`.** Each pane's `reach` is
  how far the caller reaches it: `seen`, `read` or `ask`. Beside the panes, a
  `hidden` count says how many the caller cannot see, without naming them. A
  pane nobody has seated in the Threshing Floor is `read`, as every listed pane was.
- **The agent counter opens the Threshing Floor, not the agent wall.** Ctrl+Shift+A
  is now the wall's only door, and the wall's row on the key sheet is gone.
- **A second `request_access` from one pane is refused while its first is on
  the rail.** It used to replace the first ask, which could hide it from the
  person who was about to answer it.
- **The pushed events leave out seated panes.** The feed cannot tell who is
  listening, so `agent_appeared`, `tool_call` and `agent_vanished` now carry only
  panes nobody seated in the Threshing Floor. A watcher that counts agents from the feed
  sees only unseated ones.
- **The layout file gains `seat` and `agents` on a pane, `[[lines]]`, and
  `ask_press_tokens`.** A 0.4 window reads a read-only pane as open and does not
  know seats or lines; nothing is written for it.

### Artifact and Review Continuity for 0.5.0 Development

- **The artifact rail has an Archive action.** It opens a searchable local page in the default browser
  assembled from the existing conversation records, including conversations whose
  panes have closed. Each entry separates its current target from an available
  captured revision. Unknown conversation identity, missing captures, unreadable
  records and changed snapshot bytes remain visible. A handover before conversation
  binding survives restart in an explicitly unbound record; the exact event joins
  its later conversation without guessing from a matching pathname.
- **Explicit repeat declarations capture changed files without duplicating cards.**
  A target keeps its stable artifact identity while its revisions have separate
  content hashes. Identical bytes reuse a snapshot. Background capture has a bounded
  queue, and replaying a mailbox does not manufacture a new historic revision.
  Capture time is recorded separately from handover time: the saved bytes are
  those the worker reads, not an atomic copy made by the declaration itself.
  The archive verifies a saved copy's hash before offering it.
- **Markdown notes follow uniquely identified passages.** Unchanged quotations can
  reattach after a move. Repeated quotations need distinguishing neighbours; ties
  remain unresolved. A small wording edit needs both substantial shared wording
  and matching context. The note map and note box preserve the original quotation
  and source hash, and state how the passage was matched. Legacy notes retain their
  existing anchor with an explicit evidence-unavailable status.
- **Simultaneous note saves retain one another's edits.** Notes serialize their
  read/modify/write operation through a per-document lock. A note deleted while
  its save is running queues the deletion without discarding the next pending
  note. Deletion after reattachment addresses the original stored note.

### Explicit Terminal Cleanup for 0.5.0 Development

- **Ending a terminal no longer waits indefinitely for a program that ignores
  hangup.** The PTY owner closes its master, allows a 250 ms hangup grace period,
  escalates to kill, then allows 250 ms to observe exit. Each of two process scans
  has a 100 ms budget. A kernel-blocked child is handed to a reaper and reported as
  pending; this is a bounded wait, not a guarantee that the kernel has ended it.
- **Signals follow owned identities.** The owner retains its unreaped child through
  the final group signal. Jobs in the same Unix session are addressed through
  pidfds, including interactive-shell jobs in separate process groups. A late
  close cannot signal a reused cached shell PID. A deliberately detached session
  is outside this terminal's close operation.
- **Observers do not keep a closed PTY alive.** Foreground-process and resize
  inspection use a weak reference to the owner's master. Explicitly ending the
  last window-owned view uses the same cleanup path. Hosted cleanup runs away from
  the GUI thread. Existing hold-for-undo and client-disconnection behavior remain.
- **The host close reply reports what was observed.** Its optional `cleanup` record
  separates hangup/kill signal acceptance, direct-child exit, remaining observed
  jobs and errors. Missing evidence remains absent. Older hosts and clients retain
  compatibility with the existing close reply.

### Security for 0.5.0 development

- **No agent prompts another without a line a person drew.** `ask_agent` asks
  only along a Threshing Floor line that reaches `ask`, or a bypass a person granted on
  the rail, and a pane nobody seated is never asked. A seated pane is hidden from
  every agent except along a line from that agent's own seat. Off-limits beats
  every line, and read-only caps a line at ask. Only a person draws, changes or
  erases a line: no MCP or `ctl` verb can, and the answering code is named by no
  agent-facing surface (`only_the_threshing_floor_and_the_rail_draw_lines`,
  `no_agent_facing_surface_can_answer_an_ask`).
- **A copy of another agent can only read, and waits for your Yes.** A copy
  forks only a conversation its own agent named, and no other pane names. Claude's
  copy runs with `Read` alone and no tool server: its servers are checked before
  the question, its tool list the moment it reports one, and a tool outside the
  grant that returns a result kills it. Codex's runs read-only with no network,
  no user configuration, no apps and no web search, at version 0.162.1 only.
  Nothing of Terminal Delight's environment reaches a copy, it saves no
  transcript, and its process group is killed when it answers, fails, or passes
  five minutes. By default every copy waits on the rail for a person's Yes.
- **The record of asks keeps no words.** Every ask writes a line per stage to
  `$XDG_STATE_HOME/terminal-delight/threshing-floor/<session>/asks.jsonl`, with the
  question's length and SHA-256, never the question and never the answer.
- **Your shell can no longer change MCP policy over the control socket.** Any
  program running as you, an agent included, could turn MCP writes on in one
  command and skip the Yes or No an agent's `request_access` waits for. All six
  policy commands now refuse unless the window was started with
  `TD_MCP_SCRIPTED=1`; a person changes policy in the MCP panel. This closes the
  route through a running window; a program running as you can still relaunch a
  window with that variable or edit the saved session (decision 0003). (#1088)
- **Restricted validates the pinned directory and its mount identity.** The selected path must be below the OS account home without a dot-directory in that path. External volumes, system trees and hidden roots are refused; project contents remain granted. One lifetime access proposal appears in the run terminal and can be dismissed only from outside.
- **Own-pane authority follows the connecting process.** The control socket checks kernel peer credentials against the current pane shell. Forged pane claims lose their own-pane exception. Hidden document notes, tab edits and queued bench input respect off-limits; appearance JSON cannot bypass note or deliverable validation.
- **New: `terminal-delight restricted run` runs an offline command with less
  authority.** `terminal-delight restricted run --workdir DIR [--read-only]
  [--td-note] -- CMD` runs a line-oriented command that sees only DIR (at
  `/work`), read-only system programs, and its own temporary home and `/tmp`. It
  has no network, no other files, no other processes and no Terminal Delight
  controls; with `--td-note` it may post a short note on its own pane. Its
  output reaches you as plain text, with terminal control sequences removed.
  `restricted status RUN`, `restricted dismiss RUN` and `restricted revoke RUN` inspect or reduce it from
  outside. It needs `/usr/bin/bwrap` 0.12.0 or newer and Linux 5.11 or newer on
  x86_64 or aarch64, and refuses to start rather than run without its
  protections. Anything inside DIR is granted to it, credentials included.
  Interactive programs, agents that need a network and automatic handoffs do not
  run there yet; ordinary panes keep your account's access. (#1088)
- **A scripted click or hover no longer reaches an off-limits pane.**
  `ctl bench click`, `bench hover`, `bench probe` and `bench on|off|toggle` skip
  a pane marked off-limits to agents, as the other bench commands already did.
  (#1090)
- **Nor do the document commands, or a refused surface.** `ctl doc notes` read
  out your notes on a brief floating over an off-limits pane, and `ctl doc note`,
  `doc concur`, `doc close` and `doc here` could add notes that looked like yours
  or discard unsaved ones; all the `ctl doc` commands now skip such a pane. A
  surface an agent was refused for an off-limits pane was still saved to disk
  and shown there a few seconds later; it is no longer saved.

### Added for 0.5.0 development

- **Named Threshing Floor orgs and a relationship builder.** Create and rename orgs;
  drag fixed-size pane cards into an org with a lightweight text preview.
  Drag a card's connector to another member, choose See, Read or Ask, then save
  the directed relationship. Clicking a line edits or erases it. Membership
  alone grants no access. Moving a connected member asks before removing its
  old connections. Legacy charts retain their seats and lines in an unnamed org.
  Global search finds members of other orgs and offers Open org. The expanded
  selected-agent context panel is removed. The fixed frame holds the org chart
  on the left and the pane pool on the right. Headers and filters stay visible
  while each tile scrolls its own contents, with overflow clipped at its bounds.
- **Threshing Floor pane search and session cards.** Type fuzzy words across project,
  tab and pane title, or use `project:tab:pane` to search each field separately.
  Every pane has a card with session age and current context tokens. Age comes
  from the conversation's first recorded timestamp and survives a resume;
  unavailable measurements say unknown. Escape clears the search, then closes
  the panel. Searching clears any half-drawn line.
- **The Threshing Floor: an org chart of agents that decides who may ask whom.** The
  agent counter, top right, opens it over the panes (and the `…` menu lists it
  when the counter is hidden). Add an agent to an org, then connect members with
  the relationship builder. A line carries a rung: `ask` (may ask it a question),
  `see` (may find it in `list_panes`), or `read` (may also read it). Seats are arranged in rows
  by their lines. Every agent is named the way a person finds it: where its tab
  sits on the left bar, then its header's title (*TERMINAL DELIGHT › MCP work ·
  ✳ Per-pane MCP least privilege*), on the chart, off it and on the rail's asks.
  Each seat's compact card shows state, model, session age and context tokens.
  Agents nobody seated are listed
  off the chart and behave as before, except that no agent can ever ask them
  anything. The window's five MCP switches are here as well as on the wall, and
  the bottom line counts this session's asks: answered from what was visible, by
  a copy, and with no line, which must stay at zero. Seats are random ids saved
  with the pane, so a session host restart that renumbers panes keeps the chart.
- **Agents can ask each other questions, cheapest first.** `ask_agent` answers at
  once from what the window already shows about the asked agent: its latest
  summary, sticky note, state, model, size and last ten tool calls. Nothing
  reaches the asked agent. With `copy: true` it forks the asked agent's own
  conversation into a throwaway copy that can only read, asks it once, and stops
  it; `ask_agent` returns an `ask_id` and a stage at once, and the new verb
  `ask_answer` reads the answer back for the asking pane only, labelled
  untrusted. The copy waits as a card on the asker's rail row, saying how many
  tokens it would read and the first forty words of the question, until you
  press Yes or No. A press limit (`ask_press_tokens` in the layout, 0 unless set)
  lets copies at or under it start without asking; the window has no control for
  it yet. An agent can ask for a bypass line with `request_access`
  (`change: "ask_line"`), and the card's 1 / 3 / 10 picker says how many copies
  it allows. Refusals come by name (`no_line`, `busy`, `conversation_unnamed`,
  `tools_wider` and nine more), so an agent can tell its person why.
- **A pane can be read-only to agents.** Its right-click menu now has three
  positions, the current one marked: open, read-only and off-limits. A read-only
  pane wears `READ-ONLY` in its header. Other agents may see it, read it and, along
  a line, ask it, but nothing from another pane writes to it: no note,
  deliverable, surface, appearance change, bench input or synthetic click.
- **Access asks on the rail are coloured by how serious they are.** ✓ clear
  (green) narrows access; ◇ caution (blue) reads a little; ▲ warning (yellow) is
  wider or costlier; ! alert (red) turns writes on or reads more than a million
  tokens. The colours are the theme's own, and the glyph and its name lead the
  card. A copy is a caution up to 600,000 tokens and a warning up to 1,000,000,
  and one of unknown size is never less than a warning.
- **Ops uses two glass layers.** Overlay backings soften their own grid with
  the same smoked treatment as the left spine. Foreground groupings and cards,
  including artifacts, coordination findings, plugins and agents, reveal the
  desktop through the compositor's blur. Fullscreen, 100% opacity and an
  unavailable compositor opacity reading keep those foreground faces solid.
- **An agent never replaces a floating square you have open, and 📜 lists
  what every agent handed over.** An agent's `open_document` over a square
  that is already up, whatever it shows, is held: the square stays, the file
  is kept as that pane's handover, the agent is answered `held`, and the 📜
  in the bottom-right corner bumps once and keeps a dot. Clicking 📜 opens the
  artifact rail: one card per pane whose agent has handed something over,
  newest first, named by its project and left-bar tab, edged in the agent's
  state and showing its newest artifact. A card's artifact takes you to that
  pane and opens it there; the card stays. Arrows and Enter work while it is
  up. 📜 is a UI CONFIG glyph like the other six: Clean folds it into
  CONFIG, which then lists Artifacts and carries the dot.
- **A Clean design, and UI CONFIG to hide any chrome.** Clean turns the light
  and tube off everywhere — letters, cards, frame, borders, menus, afterglow —
  and squares the corners; it holds the tube off and overrides Quiet. New
  windows open Clean, and that default is changeable. A UI CONFIG section in the
  painter hides any piece of chrome, at the window (OUTER) layer and at one
  global PANE layer shared by every pane, with the hidden menu glyphs giving way
  to a classic CONFIG dropdown. Each design remembers the look you set while
  wearing it, and a folded option row shows only its selection. (#1098)
- **The top bar collapses when it holds nothing.** With the app mark, Git notice
  and agent counter hidden by Clean's UI set and no window buttons, that row
  folds to a hairline, so the panes sit under the frame by the same margin as
  the sides instead of below an empty band. (#1103)
- **A folded option row opens on hover or a click.** A row collapsed to its
  selection opens after about three-quarters of a second of hover, or at once on
  a click anywhere on it. (#1104)
- **The palette marks the running theme.** The worn theme gets a thicker border
  and a subtle phosphor glow, and Chalk gives the selected tile more depth; the
  marker follows the actual pane or outer-window theme, including inherited
  Omarchy colours. (#1077)

- **Terminal lighting follows focus in every skin.** Active and inactive panes
  have separate letter-glow controls, and inactive text can dim relative to the
  active setting. GAUGES groups type, picture, active terminals and inactive
  terminals. Standard starts at 20% active glow and 45% inactive
  text; Intrusive uses 37% and 52%. Both use 100% inactive glow. Explicit settings
  take priority, including zero. These effects also work with CRT glass off.
- **Erased passages fade where they were drawn.** New text appears immediately;
  old letters retain their original colour before selection/cursor decoration.
  Active and inactive terminals each have cursor-linger and off-cursor-linger
  sliders: text changes range from 0–220ms and erased passages from 0–650ms.
  The existing 220/650ms appearance is the maximum and default; zero disables
  each trail independently. Shorter settings have no hidden minimum. Old saved
  afterglow zero stays off. The retained history is bounded per pane.
- **Backspace deletes selections across wrapped input.** Shift+Backspace deletes
  the previous word, Ctrl+Shift+Backspace the entire current logical line, and
  Ctrl+Shift+Alt+Backspace all editable input, including literal newlines. The
  terminal gestures use Readline-compatible editing; full-screen editors keep
  their own keys. The workbench composer has the same edits with undo. F1 Help
  lists all three shortcuts in all nine languages.

### Upgrading from 0.3.0

Nothing you saved is renamed or moved. No config or state file was renamed or
removed, sessions saved by 0.3.0 load, and a 0.4.0 window attaches to a session
host started by 0.3.0, because the host protocol is still version 1. These are
the changes you may notice in the first hour.

- **On Omarchy, add one line so Super+C and Super+V work.** Put
  `o.window("terminal-delight", { tag = "+terminal" })` in
  `~/.config/hypr/hyprland.lua`. Without it Omarchy sends Super+C to Terminal
  Delight as Ctrl+C, which interrupts the program in the pane. (#892)
- **SUPER+ALT+T opens the key menu instead of sending a tile.** On Omarchy that
  chord used to move the focused terminal into Terminal Delight. It now opens
  the list of Terminal Delight's keys, and `scripts/install-keys-hotkey.sh`
  removes the old binding when it adds the new one. `td-send` still works when
  run by hand. (#921)
- **The Field design is now Tactical.** A window or pane saved on Field opens
  on Tactical, keeping any palette you picked and its dials. Field and
  Tactical differed only in how much glow and bezel they drew. Tactical now
  also wears the new OPS skin, with a quieter glow and bezel than before; pick
  another skin from the SKIN row to keep the old trim. (#1029)
- **Some keys do something different.** Ctrl+Alt+R now renames the highlighted
  left-bar row; Alt+V still makes a side-by-side split. Shift+click on a file
  path reveals it in the file manager, and Ctrl+click opens it. On the
  workbench, Escape no longer drops you to the terminal (Alt+K or the TERM chip
  does), and in its composer Ctrl+C copies, Ctrl+X cuts, Ctrl+G interrupts the
  agent and Ctrl+A selects the whole draft. A new tab, group or project you ask
  for opens on its name box: Enter keeps the name, Escape goes to the terminal.
  With the paint overlay up, Return closes it instead of reaching the pane.
  (#532, #543, #559, #624, #662, #667, #723, #973)
- **Terminal Delight now takes some chords that used to reach your shell or
  agent.** Alt+L (find by name), Alt+A (start an agent), Alt+G (Git
  coordination), Ctrl+= and Ctrl+- (size the pane), Ctrl+Shift+= and
  Ctrl+Shift+- (size the window's chrome), Ctrl+Shift+Alt+arrows (carry a tab
  through the left bar) and Ctrl+Shift+M (an agent's handovers). In a Claude
  Code or Codex pane only, Ctrl+Up and Ctrl+Down jump through your own messages.
  On the workbench, Alt+1 to Alt+4 pick a tab and Alt+M opens a comment.
  Anything you had bound to these in readline, your shell or an editor no longer
  arrives there. (#566, #574, #816, #857, #907, #961, #965, #976, #984, #994,
  #998)
- **Sticky notes change colour on the first launch.** A note's paper now comes
  from the theme, and a note with no saved colour reads as the new default,
  contrast, so every note looks different after upgrading. The painter's STICKY
  NOTE row picks another. (#970, #975)
- **The Gamba design is gone.** A pane saved on it opens on the custom theme,
  and `TD_GAMBA` and `TD_GAMBA_DEMO` no longer do anything. The colour set once
  labelled "gamba" is now "gold" and still loads. (#992)
- **The painter's COLOUR SETS shelf and colour wheel are gone, and AGENT THEME
  is now called TERMINAL STATE.** Colours already set with either are kept but
  can no longer be picked. TERMINAL STATE keeps your saved on or off. (#972,
  #989, #992)
- **Branch pins and the tab strip's scope chip are retired.** A click on a
  project or group row in the left bar goes to that branch, and the strip always
  shows the active tab's own group. A group can no longer be moved straight from
  one project to another; unfile it first. State files that mention a pin or a
  scope still load. (#782, #816)
- **A fresh install opens flat.** New installs start in the Terminal Delight
  palette with the CRT off; an existing install keeps its tube. Warp and the
  roll sliders moved into a collapsed CRT section at the foot of GAUGES, and
  **reset** no longer clears them. If you already ran flat, the coloured haze
  under each pane header is gone. (#821, #856)
- **A tall pane may show one row fewer.** That row was being drawn partly off
  the bottom of the pane. (#747, #769)
- **Programs in a pane see a different terminal.** The emulator core is now
  rio-vt. Device attributes answer `?62;22c`, XTVERSION answers
  `terminal-delight` and its version, the cell-size and colour queries are
  answered, and the kitty keyboard query is not, because Terminal Delight sends
  legacy key encodings. Programs that probe the terminal, such as kitten icat,
  Neovim and Codex, may behave differently, mostly by no longer waiting on an
  unanswered query. (#720, #756, #825)
- **Pasted text loses its control characters.** Everything except newline,
  carriage return and tab is removed from a paste, so a workflow that
  deliberately pasted escape sequences into a program no longer works. (#950)
- **MCP answers that quote other panes start with `UNTRUSTED TERMINAL DATA.`**
  The quoted fields are named under `structuredContent.untrusted_terminal_data`,
  and a client that reads the text content by position needs updating. Panes a
  person marks off-limits also disappear from `list_panes`, `grep` and
  `pane_events`. (#951, #956)
- **Agents that speak the surface protocol get version 0.4.** A response opens
  on `layman`; `tldr` and `eli5` still parse but are no longer separate
  readings, and a `tldr` fills the new `brief`; `next` and `asks` are shown
  under `steps`; `escalation` is new and optional. Nothing an older agent sends
  is refused. (#534, #613, #620, #659)
- **`scripts/install-agent-hooks.sh` edits Claude Code's
  `~/.claude/settings.json`.** It adds five hooks the workbench uses to talk to
  Claude Code directly and puts `td-agent-hooks` in `~/.local/bin`, keeping a
  timestamped backup of the settings; `--uninstall` takes them out. Without the
  hooks the workbench falls back to typing keys, as in 0.3.0. Agents launched
  from the workbench also get a longer briefing, which now asks for decision
  briefs; agents you start by hand are told nothing new. (#624, #851, #940)
- **If you build from source, re-run `scripts/prepare-gpui.sh`.** The pinned
  gpui checkout gains two patches, and the script is safe to run again. CI
  builds with Rust 1.98.0. The old alacritty core is still available with
  `--features core-alacritty`. (#825, #912, #958, #963)
- **Three new optional runtime dependencies.** poppler (`pdfinfo`, `pdftoppm`)
  draws PDFs, libmpv plays video and sound, and Chromium draws HTML pages. Each
  is looked for the first time it is needed; without it, the file goes to your
  desktop's own app and a chip says why. Packagers should list them as optional.
  (#735, #859, #879, #931)
- **Packagers should ship the updated `packaging/terminal-delight.desktop`.**
  `terminal-delight default-terminal on` refuses until the installed entry
  carries its new `X-TerminalArg` keys. Becoming the default terminal is opt-in.
  (#955)
- **New files are created only when needed.** `launch.toml`, `local-agent.toml`
  and `documents.toml` under `~/.config/terminal-delight/`, every field
  optional; conversation history, brief backups, Markdown notes and handover
  lists under `$XDG_STATE_HOME/terminal-delight/`; and an HTML render cache of
  up to 256 MiB under `$XDG_CACHE_HOME/terminal-delight/pages`. None replaces a
  0.3.0 file. (#560, #655, #735, #741, #813, #907, #940)

### Added

- **A skin is a file you can write.** A skin's materials, type, glass and
  notes now come from one TOML file that TD validates and reloads, so a new look
  is a file edit instead of a code change. `docs/skins/examples/field-notes.toml`
  restyles the same controls as Chalk. The window can print its catalogue of 65
  surfaces, a template and a validator for the file. (#1035)
- **Tactical has a skin of its own, OPS.** Picking the Tactical design brings
  it; it can also be picked under any design from the SKIN row. Names are set
  in Oxanium, interface text in Chakra Petch and numbers in Share Tech Mono
  (all bundled, SIL OFL). Panels have cut corners, the notice and the focused
  pane's title sit between chevrons, the focused pane carries a hatched strip,
  the selected row is a solid bar with an arrow tip, and allowances are rows
  of pips you can count, hollow where there is no reading. Tactical's own
  glow and bezel are toned down to suit it. The new skin keys (`corner =
  "cut"`, `emphasis = "bar"`, `gauge`, `title`, `focus`, `material = "grid"`,
  and `[font] heading` and `data`) work in any skin.
- **A new design, Intrusive.** The CRT turned all the way up: scan lines over
  the text and phosphor glow bleeding off every letter, in each letter's own
  colour. It asks for no skin, so it mixes with OPS, Chalk or any other.

- **Alt+click on a file path opens it inside Terminal Delight.** Pictures (PNG,
  JPEG, WebP, GIF, BMP, SVG), Markdown, HTML pages, PDFs, video and sound open
  in a floating square over the pane, next to the line that printed them, and
  Escape closes it. The square moves by its title strip, resizes from its edges
  and corners, zooms with `− fit +` or Ctrl+wheel, and hands the file to the
  desktop with `↗ desktop`. A long document shows how far through it you are,
  with a column of blocks down its right edge and a percentage. (#721, #723,
  #749, #761, #812, #884)
- **🐞 reports a bug without taking anything from your terminals.** The new
  glyph in the bottom bar (and in the narrow window's … menu) writes a report on
  your machine and opens it for you to read, then, only if you click again,
  hands that one file to claude or codex to draft an issue, or opens GitHub's
  new-issue page for you to fill in yourself. A report holds Terminal Delight's
  builds, your system's versions, counts of tabs and panes, crash records and
  its own error lines with paths, links and quoted values blanked out; never
  scrollback, typed input, titles, directories, commands or a screenshot. The
  window and the session host now keep logs in
  `~/.local/state/terminal-delight/logs/`, where before both wrote into
  nothing, and a crash on any thread leaves a record in `crashes/`, marked by
  an orange dot on the 🐞. (#1042)
- **Ctrl+Alt+click opens a document in a new pane beside the one you clicked.**
  The new pane shows the document over a shell that keeps running underneath,
  and Alt+K switches between them. On the document, 0 fits, 1 is actual size, +
  and − zoom, the arrows pan or scroll and PageUp/PageDown page; other keys stop
  there, so nothing is typed into the hidden shell. "Open beside" in the
  right-click menu does the same. A document pane comes back after a restart at
  the place you were reading. (#729, #730)
- **Markdown files open rendered.** Headings, lists, tables, code and quotes are
  drawn in the pane's own palette, local pictures are shown, links are followed,
  and an edit to the file on disk is re-rendered with your place kept. Remote
  pictures are never fetched. (#728)
- **HTML pages open as the page itself.** Terminal Delight draws an `.html` file
  with headless Chromium and scrolls it itself. A `#fragment` link scrolls the
  page, a link to a file Terminal Delight can show opens there, and anything
  else goes to the desktop. The page's network is off unless
  `~/.config/terminal-delight/documents.toml` sets `network = "allowed"`, and
  renders are cached on disk so a page opened again draws at once. (#735, #754,
  #755)
- **A PDF opens in the floating square.** Alt+click on a `.pdf` path shows it as
  one column of pages, with the page in view counted at the bottom right;
  Ctrl+Alt+click opens it in a pane beside. It scrolls, drags and zooms like the
  other documents, and stays sharp because poppler redraws each page at the size
  shown. A link to `paper.pdf#page=3` opens it on page 3, and a PDF rewritten
  while open is read again on the page you were reading. (#879)
- **Video plays over the floating square.** Alt+click on an `.mp4`, `.mov`,
  `.webm`, `.mkv`, `.m4v`, `.ogv` or `.avi` path opens it in its own box with
  sound; it plays once and closes at its end, and the square it covered comes
  back as it was. A press on a video inside a page plays it the same way.
  Ctrl+Alt+click plays it in a pane beside instead, looping, where Space, ← →
  and M pause, skip and mute. (#859, #922, #924, #926)
- **Sound plays over the floating square, drawn as its waveform.** Alt+click an
  `.mp3`, `.m4a`, `.aac`, `.flac`, `.wav`, `.ogg`, `.oga`, `.opus` or `.weba`
  path, or press an audio player on a page, and it plays once in a band in the
  pane's accent colour, then closes. (#931)
- **A name in an `ls` listing opens by Alt+click.** When a listing puts each
  file's path behind its name, as `ls --hyperlink` and `eza --hyperlink` do,
  Alt+click opens exactly the file listed, wherever the shell has gone since. A
  name with no hyperlink opens only if the pane's own folder holds it. Omarchy's
  `ls` alias writes no hyperlinks; adding `--hyperlink=auto` to it makes every
  listing exact. (#948)
- **Pictures a program draws appear in the pane.** Anything speaking the Kitty
  graphics protocol — `kitten icat`, chafa, an agent's chart — draws in the
  cells it asked for, scrolls with its text, and bends with the glass. They are
  attentional: when a pane's tab is hidden it forgets its pictures, so coming
  back shows the text alone, and no snapshot carries them. Sixel and iTerm2
  pictures are not drawn yet. A picture command cannot grow past 4 MiB. A
  temporary picture file is deleted only by kitty's own rule. A pipe named as
  shared memory is never opened. A character carries at most 32 combining
  marks. (#825)
- **Decision briefs take notes and CONCUR stamps inside Terminal Delight, and
  save them into their own file.** A brief in the floating square or a document
  pane shows its note buttons, counts and stamps, and a note box that wraps to
  its width. Edits wait as "N unsaved" until **save into file**, which changes
  only the notes in the file, keeps eight backups and reads the result back
  before showing ✓. Closing a square with unsaved notes warns once first, and
  **copy map** puts the notes map on the clipboard. (#740, #741, #744, #869)
- **Markdown files take notes too.** Each heading, paragraph, code block, table,
  quote and list item takes a note from the 💬 beside it, and holding Alt
  outlines everything that does. The notes are kept by Terminal Delight under
  `~/.local/state/terminal-delight/notes/` and never written into the `.md`, and
  the map names each note by the line its block starts on. (#813, #814, #818)
- **A note box takes a paste, pictures included.** Ctrl+V, Ctrl+Shift+V and
  Shift+Insert paste text at the caret and copied files as their paths. A pasted
  picture is attached to the element the note is on and listed under its notes.
  (#894, #895)
- **A document's notes go to the agent beside it with one press.** **↪ send to
  agent**, or Ctrl+Shift+Enter over the square, pastes the notes map into that
  agent's prompt without pressing Enter, so you read it there and send it
  yourself; with unsaved notes it saves them into the file first. On the
  workbench it goes into the composer as a draft. (#777, #811, #851)
- **Agents can read your notes and open documents for you.** Two new MCP tools:
  `document_notes` returns the notes on the document beside the agent, unsaved
  ones and pasted pictures included, and writes nothing; `open_document` floats
  a Markdown file, page, picture, PDF, video or sound over the calling agent's
  own pane, or opens it in a pane beside with `"placement": "beside"`. On the
  attention rail, **⇲ beside** (or `b`) opens a declared deliverable beside its
  agent. (#773, #780, #896, #917)
- **Decision briefs ship with Terminal Delight.** The decision-brief kit is
  built into the app and written to
  `~/.local/share/terminal-delight/skills/decision-brief/` whenever the
  workbench launches an agent, and the launch briefing asks the agent to answer
  decision-shaped work with a brief, so the brief, its notes and the reply all
  stay in the window. (#851)
- **Alt+L finds a project, group, tab, pane or sticky note by name.** One list
  holds every left-bar entry, every pane by its header title and every sticky
  note by its words, each with where it sits written underneath. Type to narrow
  it, Enter goes there, Escape or a second Alt+L closes it. (#965)
- **Chalk, a paper skin for the whole window.** Pick Skin → Chalk in the
  painter's Style area. Controls, menus, gauges and the attention rail become
  deckled paper with pigment grain and handwritten labels, while the terminal's
  own text stays monospace. It follows the chosen palette, Terminal Delight's or
  the Omarchy theme's, light or dark, and sticky notes gain paper fibres, tape
  and a small flower on their curled corner. It costs a little more to draw: at
  20 updates a second, about 20% of one core against 16–17% for the default
  skin. (#1020, #1022)
- **A GIT signal warns when panes are changing the same files.** One compact
  signal covers every project in the window. Alt+G opens an inspector with a
  card per project, the working copies that share files, and the checkouts,
  branches and panes behind each, every finding with a short sentence saying why
  it is shown. Sharing you intend can be marked as expected for a while. Agents
  can read the same findings through four read-only MCP tools. (#994, #996,
  #999)
- **A pane can be marked off-limits to agents.** Right-click a pane and choose
  **Off-limits to agents**: it wears `OFF-LIMITS` in its header, disappears from
  what agents can list, search and read over MCP, and refuses changes and notes
  from other panes; the agent inside it can still work on its own pane. Only the
  right-click menu sets or lifts the mark. An agent refused access can call
  `request_access`, and the request waits on the attention rail for a person's
  **Yes** or **No**. (#956, #957)
- **Terminal Delight can be Omarchy's default terminal, and can stop being it.**
  It now accepts the flags `xdg-terminal-exec` sends (`--app-id=`, `--title=`,
  `--working-directory=`, `-e`), so once it is the default, Super+Return and
  Omarchy's TUI launchers open in it. `terminal-delight default-terminal on`
  puts it first in `~/.config/xdg-terminals.list`, `off` restores the list
  exactly as it was, and `status` says which terminal is the default. (#955)
- **Ctrl+Up and Ctrl+Down jump through your own messages in a Claude Code or
  Codex pane.** Each press goes to the previous or next thing you typed, with
  the agent's reply in view, read from the conversation rather than by
  scrolling, and nothing is sent to the agent. Escape returns to the live chat.
  Other programs still receive Ctrl+Up/Down, and Alt+arrows still move between
  panes. (#983, #984, #998)
- **Every agent pane's header names the newest thing its agent handed over.** A
  chip after the TERMINAL/WORKBENCH switch opens the latest deliverable in the
  floating square, and ▾ or Ctrl+Shift+M lists everything that agent has handed
  over, newest first. A handover is recorded whether the agent declared it over
  MCP, showed it on its workbench or printed a `Deliverable:` line, and the list
  survives a restart. (#907, #914)
- **Alt+A, or the ⌁ AGENT button in a terminal pane's header, starts an agent.**
  It opens the same project, program, model and effort picker as the empty
  workbench and launches the agent in that pane. (#961)
- **Codex, and a local program you configure, launch from the workbench with the
  same briefing Claude Code gets.** A local agent is declared in
  `~/.config/terminal-delight/local-agent.toml` as a command line; the pane goes
  back to being a shell when it exits, and a broken config shows its error in
  the launcher. (#940)
- **A new agent starts as whatever you told it to.** LAUNCH AGENT CONFIG, a
  folded block at the top of the usage card, sets the program, model and effort
  the launcher opens on. A row nobody has set shows the value in force, tagged
  `unset`; pressing a chosen value again clears it. Settings are kept in
  `~/.config/terminal-delight/launch.toml`. (#560, #582)
- **The workbench strip carries the agent's controls.** Above the card it reads,
  for example, `CLAUDE ▾ · HIGH ▾ · END SESSION`: model and effort dials, and
  END SESSION while an agent runs or LAUNCH AGENT once it has gone. The model
  dial names the version (`OPUS 5.5`) and stays readable mid-turn, each dial's
  list hangs under it with the current value lit, and changing either confirms
  the agent's own prompt and types your half-written draft back rather than
  sending it. A card taller than the pane scrolls. (#543, #553, #556, #560,
  #610, #711, #713, #715)
- **PAUSE TURN and RESUME TURN on the workbench strip.** While an agent works,
  the strip offers PAUSE TURN; once paused, RESUME TURN, and the dials can be
  changed in between. (#654)
- **The turn in progress gets its own card.** As soon as you send a message, the
  overview shows a THIS TURN card with what the agent says it is doing, the tool
  it is in, its clock and its tokens, and the reply replaces it when it lands.
  Before, the previous answer sat under your new question for the whole turn.
  (#672, #764, #870)
- **The overview shows what you said above the reply to it.** Your last message
  sits over the agent's answer in your own colour, held outside the card's
  scroll, and stays there after a long turn has scrolled it out of the
  terminal's history. (#560, #610)
- **The workbench keeps each conversation's history, so it survives restarts.**
  What you said, every card, each round of questions with its answer, and where
  the conversation was compacted or resumed are appended to
  `$XDG_STATE_HOME/terminal-delight/conversations/<id>.ws` as they happen. A
  pane that picks the conversation back up shows what it already had, and
  `terminal-delight conversation <id>` prints it without a window. (#655, #668)
- **A COMMENTS tab on the workbench holds your own notes.** Notes are written in
  your own colour, newest first, and kept across restarts; Alt+M or the `+ write
  a note` row opens the box. Nothing written there reaches the agent; **copy**
  is how the words leave. (#566, #595)
- **Alt+1 to Alt+4 jump to a workbench tab.** Tab still cycles them. (#574)
- **Ctrl+wheel sizes exactly what you are pointing at.** Over the menus, tabs or
  left bar it sizes the window's chrome as before; over a pane's header, that
  header; over the terminal, that pane's text; over the workbench, the
  workbench's own text, which has its own `bench_size` dial on the DISPLAY tray
  and in `set_pane_config`. Until you turn it, the workbench follows the
  terminal's size, so nothing changes on upgrade. (#549, #597)
- **Ctrl+= and Ctrl+- size the focused pane, and Ctrl+Shift+= and Ctrl+Shift+-
  size the window's chrome.** One press is one notch of Ctrl+wheel on whatever
  the pane is showing. (#976)
- **Text on the workbench can be selected with the mouse and copied.** Drag
  across a card or the rail beside it; letting go fills the primary selection,
  and Ctrl+Shift+C or the right-click tray copies to the clipboard. (#621)
- **The workbench composer selects like a text box.** Ctrl+A selects the whole
  draft, Shift with any motion grows a selection, and typing, Backspace or
  Delete replace what is selected. Ctrl+C and Ctrl+X act on the selection, or on
  the whole draft when nothing is selected. (#543, #639)
- **Drag a file onto the workbench and its path lands at the caret.** The
  composer lights up while the file is over it, several files arrive as several
  words, and a name with a space in it arrives quoted. A drop on the terminal
  pastes the path, as before. (#561)
- **Claude Code's folder-trust prompt can be answered from the workbench.** It
  used to show "Waiting on you" with nothing to press; it now arrives as a
  question card. (#714)
- **Agents can say how urgently they need you.** A response can carry
  `escalation` of `blocking`, `wanted` or `none`, and a blocking one is framed
  in the theme's red above the card's title. (#534)
- **A tab can be carried through the left bar from the keyboard, or dragged onto
  it.** Ctrl+Shift+Alt+Up/Down moves the active tab to the previous or next
  place in the tree, and Ctrl+Shift+Alt+Left/Right widens or narrows the step
  from tab to group to project. Dragging a running pane onto the tree files it
  there; a hover that would put a fifth pane in a tab shows red. (#816, #822,
  #832, #857)
- **Backspace and Delete erase a selection on the line you are typing.** Select
  part of the current line with Ctrl+Shift+Left/Right or the mouse, and
  Backspace or Delete removes all of it; Ctrl+X cuts it. Selections in
  scrollback or in full-screen programs behave as before. (#979)
- **Programs that ask the terminal for its colours get an answer.** The
  foreground, background, cursor and palette queries (`OSC 10`, `11`, `12` and
  `4`) are answered with the colours the pane draws, so Codex no longer waits at
  start and Neovim no longer guesses a dark background on a light theme. (#756)
- **INHERIT keeps Terminal Delight in step with Omarchy's current theme.** The
  switch beside INVERT links the window, and every pane following it, to
  Omarchy's selected colours and repaints them when the desktop theme changes.
  Turning it off restores the colours you had set by hand. (#988)
- **GAUGES gains a phosphor gauge and a glare slider.** Phosphor scales the glow
  of every lit border from none to four times the default; glare, in the CRT
  section, sets the reflection in a pane's top-left corner and follows the theme
  until you move it. (#883, #943)
- **The agent wall has an IMAGES switch.** It sits in a new CARDS group beside
  the card-size slider and takes the pictures out of every card, so more cards
  fit on screen. (#858)
- **The usage card has a Grok tab.** An installation that is not signed in says
  so rather than showing zero usage. (#934)
- **`terminal-delight bindings` names the conversation a session belongs to.**
  Each row carries the conversation's root id, how many restarts deep the
  session is and what joined them. It reports `unrecorded` where the agent hooks
  are not installed. (#642)
- **Documentation for the whole product.** The documentation site covers first
  launch to the MCP server, config files and keys, with a key sheet read from
  the app's own F1 help, a recorded tour on its homepage, and a page on why the
  terminal core moved to rio-vt. The info kiosk is rebuilt in the terminal's own
  look and points readers to the docs. (#790, #794, #805, #815, #823, #831,
  #835, #841, #848, #854)

- **The usage card says whether the week is on pace.** Under the session and
  weekly bars, a TOKEN USAGE PACE line reads the week as banked, on pace,
  behind or way over, with a bar whose width is the room left to adjust:
  whichever of the week's remaining time and remaining tokens is smaller, drawn
  blue when banked, green on pace and red when running hot. Today's tokens, the
  seven-day history and the by-model breakdown fold shut beneath it, and the
  left bar shades each provider's own logo along the same blue-to-red ramp,
  staying neutral when there is no reading to shade it by. (#1011)

- **Release candidates, and a digest on every download.** Each stable release is
  now preceded by a candidate (`vX.Y.Z-rc.N`), marked *Pre-release* on the
  releases page, that people can try before it becomes the download the install
  page serves. Every release lists the AppImage's SHA-256 and the commit it was
  built from, and is published only after every check passed for that commit.
  The schedule and the checks are in `RELEASING.md`. (#1008)

- **SUPER+ALT+T on Omarchy shows every Terminal Delight key.** It opens
  Omarchy's own searchable key menu, the one SUPER+K opens for Omarchy's keys,
  listing the keys on the docs site's Keys page, each led by the part of the app
  it belongs to. `scripts/install-keys-hotkey.sh` binds it. The Keys page also
  gains the keys added since it was written. (#921)

### Changed

- **Ctrl+↑/↓ steps a floating document; Ctrl+Shift+↑/↓ pages it.** With its
  note box open the same chords, and plain PageUp/PageDown, turn the notes. The
  Document face takes them too. Ctrl+PageUp/PageDown still switch tabs, and
  with no document floating the chords reach the shell as before.
- **Ctrl+W closes the document on top; Ctrl+Shift+W closes the tab.** A
  picture, page or clip opened in a box over a pane now goes away with Ctrl+W,
  one box per press, as Escape and its ✕ already did. Closing the whole tab
  moved to Ctrl+Shift+W and still asks first. With no box open, Ctrl+W reaches
  the shell or agent again as delete-word.
- **Model menus name the models your agents actually offer.** The launcher,
  the saved launch defaults and the workbench's live model dial read their
  choices and reasoning levels from the installed Codex or Claude, and cards and
  dials keep the reported family and version: SOL 6.1, ASTRA 6, LUNA 6,
  TERRA 5.6, OPUS 5.5. A name TD does not know is shown whole. *Agent default*
  passes no model or effort override. If an agent cannot be asked, the menu
  says so and offers retry rather than guessing, and a saved choice the agent no
  longer lists stays visible until you change it. (#1067)
- **OPS has a name in every language.** Its caption was the three Latin
  letters "ops" in all nine languages; it is now the word for "command" that
  its own help line already used in each (mando, Kommando, commande, штаб, 指挥,
  司令, 지휘, कमांड). A new CI job, Translation suggestions, flags on-screen
  English that a pull request adds without translating; it never fails a
  build. (#1062)
- **The terminal emulator core is rio-vt, the core inside the Rio terminal.**
  Measured before choosing it: an 8 MB text stream parses in 82 ms against
  alacritty_terminal's 140 ms, and a pane holding 10,000 lines of history takes
  11.0 MB against 28.5 MB. The old core is still built with `--features
  core-alacritty` as a fallback, and a window still attaches to a session host
  built before the swap. (#825, #843)
- **Terminal Delight says what it is when a program asks.** Device attributes
  are `?62;22c`, long enough for `kitten icat`'s detector, which used to wait
  ten seconds in every pane and then give up; XTVERSION is `terminal-delight`
  and its version; and the cell size (`CSI 16 t`) is answered in every pane, in
  device pixels, at the pane's current size. The kitty keyboard query goes
  unanswered, because Terminal Delight sends legacy key encodings. `--version`
  names the core. (#720, #825)
- **The workbench talks to Claude Code through hooks instead of reading its
  screen and typing into it.** A question the agent asks reaches the workbench
  whole, every option included, and pressing an answer replies to it directly.
  The composer is a real text box with line breaks and undo, Up on an empty
  draft recalls the last message, and SEND delivers the draft as one paste.
  `scripts/install-agent-hooks.sh` installs the hooks; without them the
  workbench types keys as before. (#624, #635)
- **A fresh install opens flat, and the CRT is a switch at the foot of GAUGES.**
  New installs start in the Terminal Delight palette, which now ships inside the
  app, with no tube effects. The CRT is a collapsed section at the bottom of
  GAUGES holding warp and the roll sliders, and switching it on gives the usual
  tube. An existing install keeps its tube, and **reset** now clears only the
  main gauges. (#821)
- **The painter is three areas: Theme, Style and Rules.** Theme holds the theme
  tiles and the sticky-note row; Style holds design, skin, program colour and
  syntax; Rules holds INHERIT, INVERT, TERMINAL STATE, anchor and pass-down.
  Hovering a choice explains it. (#992)
- **The colour pickers offer exactly the themes Omarchy's own picker shows.**
  The paint overlay's shelves are FAVOURITES and OMARCHY THEMES, and the theme
  tray shows one tile per desktop theme in its own colours. Hiding, installing,
  removing or editing an Omarchy theme shows up in both within a second, and
  themes that ship only an `alacritty.toml` now load. Without Omarchy, the stock
  Omarchy themes are offered from a copy Terminal Delight carries. (#972)
- **Sticky notes take the theme's own colours: contrast, accent or mute.** The
  STICKY NOTE row in the painter picks the paper, and the ink stays readable at
  4.5:1 or better on every theme. (#970, #975)
- **TERMINAL STATE (formerly AGENT THEME) colours a pane by what it is
  running.** SSH, Claude Code and Codex keep their colours, and local agents,
  other agent programs, editors, system monitors and consoles get their own.
  Shells, unrecognised programs and panes you painted by hand keep theirs.
  (#989)
- **Return finishes painting, and closing the painter closes it in every window
  on the workspace.** With the paint overlay up, Return folds it and no longer
  reaches the pane, and Tab, Space, Backspace and Delete are ignored. Folding
  the overlay in one window folds it in every Terminal Delight window on the
  workspace. (#973)
- **An idle window uses less memory, and background panes cost less CPU.** On
  the measured laptop an idle one-pane window's GPU memory fell from 182 to 86
  MiB and it maps 13 font files instead of 263. Panes in hidden tabs redraw at
  most every five seconds, unchanged panes are not redrawn when a neighbour
  changes, tab badges animate a few times a second, and a visible pane's
  streaming output redraws at most about thirty times a second while your own
  typing still echoes at once. (#958, #963, #967, #971, #974, #980, #982, #986,
  #993, #1000, #1006)
- **The FOCUS reader (Alt+R) has a size of its own.** It lays text out at the
  pane's own letter size across the whole glass, so a narrow pane opens with as
  many columns as the glass holds instead of its own few columns blown up. The
  slider and Ctrl+wheel move the letters from 0.5× to 2.5×. (#893)
- **A shell's lines reach the reader as the program printed them.** Rows are
  joined exactly where the terminal soft-wrapped them, what a program broke
  itself (like `ls`'s columns) stays as printed, and blank rows under the last
  line no longer push a short screen's content out of view. (#893)
- **Alt+R on an agent's pane reads its conversation.** When the pane's Claude
  Code or Codex session can be identified for certain, the reader shows the
  conversation from its transcript instead of the narrow screen: what you typed
  and the replies whole, with Markdown drawn, one row per tool call that a click
  opens, and a divider where the conversation was compacted. The agent's own
  input box stays live at the reader's foot. The header says what is being read:
  TRANSCRIPT, CODEX, SCREEN, SCROLLBACK or ALTERNATE SCREEN. (#900, #902)
- **Alt+R on a document reads the document.** A document pane, or a square
  floating over a pane, lends the reader its own view, where it scrolls, zooms
  and takes notes as on its pane; the reader used to show the shell hidden
  behind it. (#898)
- **Ctrl+Alt+R renames the highlighted left-bar row instead of splitting the
  pane.** Ctrl+Alt+↑/↓ walks to a project, group or tab and Ctrl+Alt+R opens its
  name box; with the tree shut it renames the active tab. Alt+V still makes the
  split. (#662)
- **A new project, group or tab you ask for opens on its name box.** The left
  bar, Ctrl+Shift+T, the strip's `+` and "New tab here" put the keyboard in the
  new row's name; Enter keeps it and Escape drops into the terminal. A tab that
  arrives as a side effect does not take the keyboard. (#662, #667)
- **Shift+click on a file path reveals it in the file manager.** It used to open
  the file; Ctrl+click still opens it, and Shift+click on a web link still opens
  the link. The right-click menu offers Open here, Open with desktop, Reveal in
  folder and Copy link, each labelled with its click, and the Alt chip says what
  Alt+click will do on the line under the pointer. (#723)
- **A click on a project or group row in the left bar goes to that branch.** It
  opens the branch's first tab, as its chip on the shut strip does, and the tab
  strip always shows the active tab's own group. (#545, #546, #596, #782)
- **A group filed under a project stays in that project.** Moving it straight to
  another project is refused; unfile it first, then file it where it belongs.
  (#816)
- **The window's corners are rearranged.** The agent counter, which opens the
  agent wall, is at the top right. The bottom right holds what the window
  contains (`6 tabs · 6 panes`), the menu glyphs, the menu-bar scale and the
  split buttons, which are drawn as boxes filled on the half the new pane opens
  into, and the theme and display trays open over the buttons that raise them.
  With the tree shut, the tab strip reads project, then its groups, then your
  group's tabs. (#748, #767)
- **A new install's panes wear Terminal Delight's own mark.** With no
  `dir-logos.toml` at all, the first start writes one mapping `/` to the app's
  logo, where panes used to show an empty logo slot. An existing file, even an
  empty one, is never touched. (#887)
- **The workbench is a surface you stay on.** Escape no longer drops you to the
  terminal once there is nothing left to close, and choosing an option on a card
  no longer flips the pane to its terminal. Alt+K and the TERM chip go back.
  (#532, #559)
- **The workbench opens documents inside Terminal Delight.** An HTML or Markdown
  artifact on the workbench floats over it instead of going to the desktop, and
  Alt, Ctrl+Alt and Ctrl on an artifact or on any link in a card, including a
  Markdown link's words, do what they do on the terminal. A deliverable clicked
  on the attention rail opens in the floating square instead of the browser.
  (#781, #787, #795, #859)
- **A response card is a row of tabs over one body.** Instead of a stack of
  folding sections, a card shows `reading · evidence · steps` across the top,
  and a reply that is only a short answer draws no tabs. An agent still sends
  `next` and `asks`; they are shown under `steps`. (#554, #613)
- **A reply card opens on its plain answer, and carries a `brief` as its
  shortest reading.** The plain-language reply is what a card and the overview
  open on, with the technical reply one chip away. A `brief`, at most two
  sentences under fifty words, is drawn last (`Plain brief · Technical brief ·
  Brief`). `tldr` and `eli5` are no longer separate readings: a `tldr` fills the
  brief and an `eli5` is kept under Other. (#620, #659)
- **A response card has at most one loud thing on it.** Only a declared
  escalation is drawn in the attention colour, word and item counts beside each
  reading are gone, and the empty workbench is a single launch card. (#534)
- **A card's weights read as one sentence.** The strip that read `effort L ·
  complexity moderate · depth subsystem · terminal-del…` now says, for example,
  "A big, fiddly job in terminal-delight — workbench, where several components
  depend on it. Measured." It wraps instead of cutting the name off. The routing
  lines under each card are shown only with `TD_VERB_PREVIEW=1`. (#613, #657)
- **A round of questions is one card with SUBMIT ANSWERS.** A three-question
  round puts one row on the rail instead of three. Its tabs name each question
  and tick the answered ones, answering one moves to the next, and SUBMIT
  ANSWERS sends the round however much of it is answered; a question left blank
  goes back as skipped. Questions use the theme's accent instead of bright red,
  and SUBMIT turns green once every question is answered. (#634, #638, #691,
  #764)
- **When an agent ends, its workbench goes with it.** The next agent started in
  that pane opens on an empty workbench instead of inheriting the last one's
  cards and questions. (#627)
- **Return starts the agent on a workbench that is offering one.** It used to do
  nothing there; it opens the launcher unless a card is open. (#560)
- **Every pressable button on the workbench lights under the pointer**, over
  exactly the area that takes the click, and labels that cannot be pressed stay
  unlit. (#669)
- **Two numbers that said nothing are gone from the pane header.** The header no
  longer shows its grid size (`194×50`), and the bare digit beside the BENCH
  toggle is gone; the badge above it already says `2 answers waiting` in words.
  (#573, #591)
- **Small print on workbench cards is readable.** Subtitles, cost lines, section
  tags and provenance lines were drawn at 1.22:1 to 1.62:1 against their
  background; they now clear 4.5:1 on every bundled palette. (#555)
- **Tabs and the TERM ⇄ BENCH switch are easier to read.** A lit tab's label,
  which measured 1.38:1 against its own background, stays in the text colour
  with the highlight on the tab's edge; TERM ⇄ BENCH is one switch instead of
  two chips; panels keep their contents off their own border; and a card's
  primary button no longer glows over its word. (#555)

### Fixed

- **The OPS skin no longer crashes the window.** A cut-cornered panel that was
  laid out at no size for a frame, as one is while it opens or collapses, made
  the window panic while drawing it. Clicking a tab in the left bar was one way
  to hit it. (#1072)
- **The 🐞 bug panel shows its rows under the OPS skin.** Its background was
  drawn over its content, so the panel opened as an empty frame and a bug could
  not be reported from it. (#1072)
- **Omarchy's Super+C copies in a terminal pane, and Super+V pastes.** Omarchy
  sends its universal copy and paste to a window as Ctrl+Insert and Shift+Insert
  when it counts the window as a terminal, and as Ctrl+C and Ctrl+V when it does
  not. Terminal Delight was not counted, so Super+C interrupted whatever ran in
  the pane and Super+V typed `^V`. A pane now takes Ctrl+Insert as copy and
  Shift+Insert as paste, the chords every other terminal takes, and the README
  gives the one-line Hyprland rule that tags Terminal Delight a terminal. (#892)
- **A note box holds any number of notes.** Notes past the box's height were
  saved but clipped out of sight with no way to scroll to them. The box now
  opens on the newest, the wheel turns back to the oldest, and a note just
  added is always in view. A hidden note's delete no longer takes a press
  under the draft.
- **Launching Codex no longer picks a retired model.** The launcher's Codex
  menu offered `gpt-5-codex` and `gpt-5` and passed one with `--model`, which a
  current Codex account rejects with HTTP 400. Codex now starts on the model in
  its own configuration unless you pick one it lists. (#1044)
- **Pasting a screenshot into Claude or Codex works again.** The agents could
  not reach the desktop clipboard from the session host, so Codex timed out on
  X11 and Claude found no image. TD now saves the picture and pastes its path,
  which both agents attach as `[Image #1]`; Ctrl+V, Ctrl+Shift+V, Shift+Insert
  and workbench paste all do it. (#1060)
- **Turning origin mode on moves the cursor home again.** A program that sets
  origin mode (DECOM) expects the cursor at the top of its scrolling region,
  where xterm, tmux and the old core put it. The rio-vt core left it where it
  was, so the program's next text landed in the wrong place. Found by the
  Terminal Lab. (#1034)
- **A screen redraw opened the older way is no longer drawn half-finished.**
  Programs can ask the terminal to hold a redraw back until it is complete,
  either with `ESC [ ? 2026 h` or with an older string, `ESC P = 1 s ESC \`.
  Terminal Delight already held the first form back properly; with the second,
  text that arrived together with the request was drawn at once, so the frame
  could appear torn. Found by the Terminal Lab. (#1046)
- **Tabs no longer come back out of their projects after the window restarts.**
  One answer from the session host that took longer than five seconds marked
  the host as gone for the rest of the window's life, and the only thing that
  checked was saving. The window kept working but stopped writing its layout,
  so tabs opened after that were missing from the file and the next window
  brought them back bare, with no name and no project. A slow answer no longer
  counts; only a closed connection does. And whatever stops saves in future,
  the window now checks every half minute that the saved layout matches what
  it shows. It saves again quietly the first time they differ, and if they
  still differ it says "Layout not saving" across the top of the window and
  in a desktop notification. (#1017, #1024)
- **The agent wall says NO READING for a session it has not measured, instead
  of calling it empty and healthy.** A session whose newest record carried no
  context figure was drawn 0% full, 100% relevant and RUN, with thousands of
  turns of headroom, even while holding 165,815 tokens. Its WINDOW and
  RELEVANCE bars now draw as a grey track captioned with a dash, the verdict
  reads NO READING, and the placeholder records Claude Code writes for an
  interrupt or a session bridge no longer count as turns or as activity.

- **A paste while a sticky note or a pane name is being written goes into it,
  not into the shell under it.** Ctrl+Shift+V typed the clipboard into whatever
  ran behind, a newline included, and Ctrl+V and Shift+Insert vanished. All
  three now paste into the note or name, as one line. (#937) — thanks @nixfred
- **A file reference Codex wraps across rows opens whole.** Alt+click or
  Ctrl+click on a path Codex broke over indented rows opened nothing or half a
  path. A relative reference such as `(reports/brief.html)` also opens by
  Alt+click, from the pane's folder. (#917, #939)
- **A link Claude Code breaks across two rows opens whole from either half.**
  The second half starts at Claude's two-column margin, where the click did not
  look, so a wrapped `file://` path opened nothing or the wrong half. Where the
  break is ambiguous, the reading that names an existing file wins, and the
  second half is painted as a link too. (#882)
- **The Alt copy chip copies a command Claude Code wrapped as one line, and
  works when Claude Code runs fullscreen.** It copied only the first row, with
  the margin in front; it now copies the whole command from either row. In
  Claude Code's fullscreen mode the chip had been switched off altogether.
  Full-screen programs such as vim and htop still get no chip. (#882, #927)
- **A restart reopens the session that was on that workspace.** A save recorded
  whichever workspace had keyboard focus rather than the window's own, so two
  windows could claim one workspace and a restart could reopen the wrong
  session. (#981)
- **Live panes no longer wear a false DUPLICATE TAB note.** Two Codex panes in
  one folder, or any two bare `claude` panes, were taken for duplicates on every
  save, and the second was told it was a fresh shell while its agent was still
  running. The note is now written only on a pane that really comes back empty,
  and the false ones already saved are taken off when the pane reattaches.
  (#1004)
- **Window and pane shortcuts work while the workbench is showing.** Alt+S,
  Ctrl+Shift+B, Ctrl+W and most other chords did nothing on the workbench, and
  the rename box took focus but no letters. A key the workbench has no use for
  now reaches the window, the pane or the terminal. (#609)
- **Escape reaches a pane again after it closes the reader there.** After
  closing the FOCUS reader with Escape, Escape and the page keys in that pane
  went to the closed reader, so Esc could not interrupt an agent until the
  reader was reopened. (#898)
- **With two agents under one shell, the pane follows the one in the
  foreground.** Suspending an agent with Ctrl+Z and starting another left the
  workbench, the agent wall and the tool glyph reading the suspended one. (#581)
- **Each pane reads its own conversation.** Several agents in one folder could
  all be shown the newest conversation there, so each workbench showed another
  agent's work as its own. A pane that cannot be matched to its conversation for
  certain now shows nothing rather than a neighbour's. A hand-written
  session-ledger entry with spaces in its JSON is read too. (#576)
- **A surface an agent presents over MCP survives a restart.** It reached the
  window but not the disk, so everything sent through MCP vanished with the
  window. (#578)
- **Re-sending a surface can no longer lose it.** Two writes of the same surface
  at once could collide, and one vanished under an error that blamed a missing
  directory. (#547)
- **An agent keeps its Terminal Delight tools after a window it knew has
  closed.** A leftover socket file was taken as proof the window was alive, so
  an agent could lose every Terminal Delight MCP tool for the rest of its
  session. (#664)
- **Background notifications are no longer shown as messages you typed.** Claude
  Code reports a finished background task or a message from another session as
  if it were a prompt; those turns are now kept apart from what you said. (#656)
- **A round of questions ends once.** A refused round stayed open with pressable
  cards, and after a restart could come back live and send answers to a question
  already closed. A round the agent has moved past now closes, the tab badge no
  longer blinks with every keystroke, and an answer pressed on the workbench
  reaches the agent instead of stalling. (#660, #666, #700, #771)
- **A question opens at the top of the workbench and stays there.** It used to
  appear against the composer and jump up when you pressed an option. (#673)
- **The workbench strip stops saying "Reading your answer" after the agent has
  left.** An agent started in the pane soon after could have its first question
  hidden behind it. (#936) — thanks @nixfred
- **A program asking a pane its size in pixels is told the current size.** A
  pane on a session host answered with the size it was created at, a
  window-owned pane did not answer, and the cell size was rounded down on scaled
  monitors. (#720)
- **Clicks and the Alt chip land on the row you see, all the way down the
  pane.** At fractional text sizes on a scaled monitor the click target drifted
  upward the further down the pane it was, by most of a row at row 40. (#747,
  #769)
- **Panels and hover cards over a curved pane are drawn flat.** With CRT warp on
  they bent with the terminal and could be cut off down one side; hover cards
  now wear the window's menu style. (#682, #699)
- **Highlights go out when the pointer leaves a pane.** Workbench chips, the
  sticky note's peel corner and the floating square's buttons stayed lit after
  the pointer moved away. (#753)
- **A flat screen no longer shows a coloured haze under every pane header.** It
  is drawn only with the CRT on. (#856)
- **A drop that would put a fifth pane in a tab leaves the layout alone.** A
  pane dropped on a full tab through the tab strip joined it anyway, and a
  refused drop on the left bar pulled the pane out into a tab of its own. (#816,
  #819)
- **A tab can be dragged out of its group when every tab is filed.** The UNFILED
  line, the only drop target for "no group", was drawn only once some tab was
  already loose. (#558)
- **The savings tab of the `</>` card no longer opens empty** when the card is
  opened from `Σ usage`, Ctrl+Shift+A or a left-bar allowance rail. (#575)
- **A workbench artifact named by `open`, `path`, `file`, `url` or `link` is
  shown as an artifact**, not as raw JSON, and table columns are sized to their
  contents and wrap instead of being cut at forty characters. (#610)
- **Pasting a filename with a space in it no longer breaks it in two.** A copied
  `Screenshot 2026-09-18.png` arrived as two words. (#561)
- **The F1 help card scrolls to its last row.** On a short or narrow window the
  last rows, the demo button and the footer could not be reached. (#947)
- **Piping a command-line verb into `head` no longer prints a panic.**
  `terminal-delight skin --list | head -5` printed a Rust backtrace after its
  output. (#608)
- **The trash bay at the foot of the left bar has a frame and one seam**,
  instead of two lines down its middle and no frame. (#704)
- **Two chips in the workbench composer follow a square skin.** The paste chip
  and the `LIVE → AGENT` chip stayed round when every other corner squared.
  (#566)

### Removed

- **The Gamba design and its slot-machine overlay.** A pane saved on it opens on
  the custom theme; pick another design in the painter's Style area. Bells and
  attention badges are unaffected. (#992)
- **The painter's ◎ theme button under SKIN.** Pick the skin you want instead;
  a fresh window still starts on the skin its theme names. (#1051)
- **The Field design.** It is folded into Tactical, and a pane saved on it
  opens on Tactical. See Upgrading from 0.3.0. (#1029)
- **The painter's COLOUR SETS shelf and colour wheel.** The Omarchy themes
  replace the colour sets; colours already chosen with either are still applied.
  (#972, #992)
- **Branch pins and the tab strip's scope chip.** See the left-bar entry under
  Changed. (#748, #782)
- **The agent wall's 🎨 theme toggle.** The wall draws in its panes' own colours;
  `TD_WALL_THEME=1` still starts it themed. (#652)
- **The agent wall header's six unlabelled glyph counters.** They set the same
  filter as the WORKING / DONE / IDLE chips below them and could contradict
  them. (#560)

### Security

- **A paste can no longer end bracketed paste early and type keys into your
  shell.** A clipboard holding its own end-of-paste marker could make the shell
  read the rest as typed keys, so a line copied from a web page could run a
  command. Every paste, including the context menu's Paste and a dropped file's
  path, now has control characters removed except newline, carriage return and
  tab. (#950)
- **MCP answers that quote other panes say their words are untrusted.**
  `list_panes`, `grep`, `pane_events` and `engineering_state`, and the pushed
  `agent_appeared` and `tool_call` events, open with `UNTRUSTED TERMINAL DATA.`
  and name every quoted field, so an agent reading what another program printed
  is told it is data, not an instruction. (#951)
- **An MCP pid too big for 32 bits is refused instead of landing on another
  pane.** The pid was cut to its low 32 bits, so `4294967396` addressed pid 100,
  and tools that read or change a pane could reach one the caller never named.
  They now answer "pid is out of range". (#935) — thanks @nixfred
- **A program cannot make a pane hold an unfinished escape sequence without
  limit.** An ESC followed by a run of intermediate bytes was held for as long
  as it stayed open and rescanned on every read with the terminal locked; those
  bytes now pass straight through. (#938) — thanks @nixfred
- **Documents opened inside the terminal stay off the network and the rest of
  the disk.** HTML pages render with the network off unless `documents.toml`
  allows it, in a throwaway browser profile, and Markdown never fetches a remote
  picture. Saving notes into a brief refuses a symlink or a hard link, and
  pictures handed to an agent with a document's notes are read only from regular
  files inside their own folder. (#728, #735, #741, #896)
- **Agents can be kept out of a pane.** Only a person can mark a pane off-limits
  or lift the mark, and an agent's request for more access waits for a person's
  Yes or No. (#956, #957)

## [0.3.0] — 2026-09-18

### Added — the week of 15 September

- **The Overview is the feed of what the agent said.** TDSP 0.3 adds the
  `response` kind: a `tldr` that is always open, registers a person unfolds by
  name (ELI5, plain brief, technical brief, what was verified, needs from you,
  what's next — and any key the agent invents, labelled by the key), and
  articles of doubt drawn apart in their own colour with a confidence on each.
  The OVERVIEW shelf holds responses and nothing else, and shows the newest one
  by itself until a person opens another; changesets file with decisions and
  the unclassifiable with artifacts. The launch briefing asks every agent to end
  its turn with one.
- **The launcher's effort row is the harness's own dial.** Claude Code's five
  levels (`low`, `medium`, `high`, `xhigh`, `max`) and Codex's four, verbatim,
  passed as `--effort` and `model_reasoning_effort` — the `quick / standard /
  hard / ultra` scale that was turned into a sentence about thinking is gone.

### Fixed

- **LAUNCH AGENT starts the agent in the pane you pressed it on.** The button is
  drawn on one surface only — the empty bench of a pane with no agent, under the
  sentence *"A shell has no agent to present anything. Launch one into this
  pane."* — and it then opened a tab at the far end of the window, in no group at
  all, leaving the pane you were standing in exactly as empty as before. It now
  types the recipe at that pane's prompt, clearing the line first and `cd`-ing
  only when the project is somewhere else; when the pane is busy, or when nobody
  has read it yet, it opens a tab seated in the branch you are working in
  instead of loose under UNFILED. The panel's `↵` hint names which of the two it
  is about to do. An adoption from another session (`ctl adopt`) stays loose on
  purpose. (#508)

- **The bench no longer types into a pane with no agent.** The composer mirrors
  the agent's own line editor rather than holding a buffer of its own, so every
  keystroke went straight down the pseudoterminal — and on a pane whose agent had
  exited, the shell underneath collected them into a command line and the return
  key ran it. A bug report sent from the bench became `claude <the whole
  message>`: a brand new session with itself as the argument, no history, while
  the bench went on drawing the conversation it thought it was talking to. Writes
  now pass one gate that asks both questions — is the pane on screen, and is
  there an agent in it — and anything held says so in the header rather than
  counting silently. (#509)

- **The launcher's project list was squeezed to nothing** whenever the filter
  matched only a few projects: the panel's height was computed for one chip row
  and it has four, so the list — the only child that could shrink — gave up its
  entire height. Typing `ter` showed no `terminal-delight`. The chrome is now a
  named sum every fixed child is counted into, with a test that holds every
  match count gets its rows on top. The scan behind it also kept only the 60
  newest of 257 directories, so a project untouched for a week vanished from
  the picker entirely; it keeps them all now and caps only what is drawn.

- **The Workbench.** Every pane has a second face. The TERMINAL is the agent's
  typing; the WORKBENCH is its work — decisions to take, changes to review,
  documents to open, tables of things compared, and the question it stopped on —
  presented as JSON documents through one protocol (TDSP 0.2) over three
  transports: a `.json` file dropped in the pane's own directory, `terminal-delight
  surface` on the CLI, and the `present_surface` MCP verb. A person answers from
  the bench and the answer is typed back into the agent's terminal as one line,
  signed with the session's tag (`[workbench:<tag>]`), with the exact bytes every
  verb will type shown under it before it is pressed. Every card says who put it
  there. `ctl bench on|off|toggle|choose|say|type` drive it from a script and
  answer with the outcome, and `surface_catalogue` says what a build can render.
  (#493, #498)
- **The attention spine.** A rail per pane and a queue over the panes that say
  which agent wants a person, why, and how urgently — promoted, neutral or demoted
  by a thumb on the scale — with a review tray, levels in the left bar's tree, and
  docking. The rail reads live pane state rather than a synthetic rotation, and
  the pill counts each lane in its own colour. (#414, #419, #424, #435, #443, #445,
  #446, #448, #449, #454, #460, #463)
- **The left bar's tree answers to the keyboard**: `Ctrl+Alt+↑/↓` walk it on a
  ring, `→` opens a branch, `←` climbs out, `1…9` jump to a top-level branch; a
  group can be deleted and given back; every way of ending something goes through
  the bay. (#417, #423, #428, #450, #459)
- **Instance identity.** Several Terminal Delights on one box, and every MCP
  answer names the window it came from; an agent finds its own terminal by walking
  its parents. (#469)
- **A bell in the chrome footer**, with a notifications panel and two switches
  behind it. (#468)
- **The paint overlay paints the cabinet** from a card hung off the top of the
  window, and the OUTER tray passes its theme down to every pane — including panes
  not born yet. (#464, #472)

### Changed

- A window asks the session host what its panes are running rather than being told
  once, so an agent that started after the window opened is no longer invisible to
  the rail. (#465)
- Trays stop at the terminal render, one hairline past it, and scroll the rest.
  (#470)
- A tab you closed stays closed: the session records what its trash is holding.
  (#482)
- The filming pipeline left the open-source repository. (#455)

### Fixed

- A working agent in a narrow pane is visible again: detection stopped depending on
  a footer line the CLI truncates at tiled widths. (#477)
- An agent whose screen cannot be read stops reporting as idle, and the Unknown
  state no longer switches off the finish bell. (#425, #426)
- A restored duplicate no longer paints nothing: a terminal answers to one pane
  again, and a layout can no longer say that two leaves are one terminal. (#433,
  #437)
- A tab that loses a pane keeps its name, its colours and the branch it hangs
  from; the divider above the loose tasks names them. (#413, #473)
- Four defects in the attach encoder, found by a generated sweep. (#386)

### Changed

- **TD wears the desktop's font.** A theme that names no font now takes whatever
  Omarchy has pointed the `monospace` alias at — the same family alacritty,
  foot, ghostty and the shell are already using — instead of asking for
  `JetBrains Mono` by name. `omarchy font set` writes its choice into
  `~/.config/fontconfig/fonts.conf` as a strong `prepend_first` and calls
  fontconfig "the canonical source of truth"; `omarchy-font-current` is one line
  of `fc-match monospace`. TD now asks the same question, so changing the system
  font changes TD's on its next launch. A theme file that DOES name a family
  still wins — an explicit choice outranks the desktop — and a machine with no
  fontconfig to ask falls back to the shipped default exactly as before.

### Fixed

- **The font TD asked for was installed, and TD could not find it.** The lookup
  tested for an exact family name, so a box carrying `JetBrainsMono Nerd Font`
  answered "JetBrains Mono is not installed" and the whole UI silently ran on
  Liberation Mono — which has no `▾` (U+25BE) and no `▸` (U+25B8), so the left
  bar's disclosure triangles rendered as blank space and folding looked broken.
  A patched build of the requested family is now recognised as that family (the
  name with spaces removed, optionally followed by a Nerd Font suffix), tried
  before any substitute, and the launch diagnostic stays quiet when that is what
  happened. The match is tight enough that `Noto Sans` cannot capture `Noto Sans
  Devanagari`.
- **The rename pencil on every tab has been invisible.** It was drawn with
  U+270E, which exists in exactly one font installed on a typical desktop — and
  that font is in none of TD's fallback chains. It is now U+270F, which lives in
  Noto Color Emoji beside the pin, the robot and the tick.

### Added

- **A left bar, and a tab is now a task.** The mother bar had run out of
  attentional space: twelve titles competing for one glance, wrapping onto a
  second row, which is the same problem stacked. The left edge of the window now
  carries the session as a collapsible tree, two layers deep and no deeper —
  **PROJECT** over **INITIATIVE** over the tabs themselves, each holding its
  sub-terminals. The initiative layer is the tab group that already existed,
  read as what it always was: a run of tasks belonging to one push. Its colour
  band, rotated ninety degrees, is the rail each initiative's rows sit against.
  - **Scoping is the point.** Clicking a project or an initiative narrows the
    MOTHER BAR to that branch; the tree never narrows. A strip carrying one
    push's worth of tabs is a strip that stops wrapping.
  - **Nothing can be lost behind a fold.** The branches holding the active task
    refuse to collapse, activating a task from anywhere widens the scope to
    contain it, and every branch row rolls up the 🤖 / ✅ / ❌ / 📌 of everything
    beneath it — a folded project holding an agent that stopped to ask a
    question blinks in the tree. The strip carries a `⋯n` chip counting what the
    scope is hiding, lit when one of them is waiting on you.
  - **Filing.** Drag a task (or a whole initiative) onto a branch; the chip under
    the cursor says where it will land, in that branch's colour. A tab's config
    tray gained the same control for people who would rather press a button, and
    `⌁` in the bar's header files every unfiled task under the project its
    terminal is actually sitting in — its git repository, walking up to the root,
    so a worktree adopts as itself rather than as the folder its siblings share.
  - `ctrl+shift+B` shows and hides the bar; with it hidden the strip keeps a `⟩`
    handle where the bar used to be, because a feature you can only restore by
    knowing a chord is one people turn off once and never see again. The tree,
    its folds, the bar's width and the current scope all persist per session, and
    a session file written before any of this existed opens as what it is — an
    unorganised list of tasks, not an empty window.

- **A favourites shelf in the paint overlay, because nobody wears thirty-three
  looks.** The overlay's two existing shelves are each COMPLETE — every colour
  set we ship, every Omarchy palette installed on the machine — and that
  completeness is exactly what makes them slow to paint from. The new shelf is a
  hand-picked shortlist that **mixes both vocabularies**, since a person's
  shortlist does not respect where a colour came from. It sorts first and is the
  default, because it is the shelf that actually gets worn.
  - `⇧f` stars the look a pane is wearing, and unstars it; the legend says which
    of the two the key will do rather than making you find out.
  - The list lives in `~/.config/terminal-delight/favourites.toml` as one ordered
    array of **tagged** ids — `favourites = ["palette:retro-82", "set:army"]`.
    The tag is load-bearing, not decoration: a desktop is free to ship a theme
    named `army` beside our colour set of the same name, and an untagged list
    could not say which one was starred. Order in the file is order on the shelf.
  - A favourite naming something this desktop does not have — a theme from the
    old laptop — is **unresolvable, not absent**: skipped when the grid is drawn
    and *kept* in the file, so reinstalling the theme brings the tile back
    instead of it having been quietly dropped months earlier. A typo in a
    hand-edited file drops that one line rather than poisoning the list.
  - The shelf disappears when nothing is starred, and a stored shelf that has
    gone away — the last favourite unstarred while the overlay is open — falls
    back to the first visible shelf rather than to a clamped number.

- **Agents can leave notes on the fridge door.** A new `leave_note` MCP tool
  posts a sticky note onto a pane's glass — a bold headline plus at most ten
  words of body ("GET MILK!" / "home at 7pm") — so a returning human reads the
  wall the way they read a fridge, before opening any transcript. Post again to
  change the note (same paper, same lean, new words), `clear` peels it,
  `pin: true` pushes the pin through so the tab flags it from the mother bar.
  The note lands on exactly the paper a human `alt+s` writes on: peel, edit,
  pin and restart-survival all behave identically, and `list_panes` now reads
  the door back — each pane's posted note rides its listing line. Writes stay
  behind the same `TD_MCP_WRITE` opt-in as every other mutation, and the
  ten-word limit refuses verbosity by name, with the count, so an agent's retry
  is an edit rather than a guess.

- **The robots came to the web wall.** A working card on `agents.html` now shows
  the playhouse robot animated — holding the tool that agent is actually
  holding, wearing that tool's face, with the verb lettered on the glass: *at
  the console*, never `Bash`. Stop working and the card goes back to its project
  art, which is the app's own precedence: a busy pane wears what it is doing, a
  resting one wears where it is.
  - The art is **published, not copied by hand**:
    `scripts/publish-robot-faces.mjs` moves the 19 scenes, their still props and
    the tool table out of `app/assets/` into `assets/robots/`, and `--check`
    fails CI on drift in either direction, orphans included. One table, two
    walls, and no second place to edit.
  - The verb is used **both** on the glass and as the live-action word in the
    recap, so the picture and the sentence are the same string and cannot end up
    describing different activities. A check asserts it.
  - Codex panes **do** wear a face, unlike the vitals bars they still cannot
    have — `tail_tool_events` reads the Codex `function_call` shape as well as
    Claude's `tool_use`, so their cards hold `exec` and `grep` under their own
    names. Two subsystems, two honest answers, both visible on one card.
  - Reduced motion swaps the animation for the app's own still prop, because an
    animated WebP cannot be paused from CSS and six looping robots is a lot of
    movement for someone who asked for less.

- **The kiosk family is one site.** All seven pages — info · omarchy · agents ·
  tv · global · gamba · start-crawl — now carry the same head furniture and a
  shared strip that names every other kiosk, so `tv`, `global` and `gamba`
  stop being places you can reach and not leave.
  - **A real favicon.** `favicon.ico` (16/32/48) and `favicon.svg`, cut down
    from the app's own CRT mark until it survives 16px: the tube silhouette
    and the three phosphor panes, without the stand, the scanlines or the
    eight lines of text that are a grey smudge at that size. `/favicon.ico`
    stops answering 404 on every page load.
  - **The theme travels.** The palette table moved out of `omarchy.html` into
    `assets/kiosk-theme.js`, and the pick is remembered under one key for the
    whole family — choose gruvbox on the Omarchy kiosk and the info page and
    the agent wall are already wearing it. `?t=<theme>` still opens a page
    already dressed.
  - **The cabinets keep their cabinets.** A console television that turns
    tokyo-night is no longer a console television, so on `tv`, `global`,
    `gamba` and `start-crawl` the roles are written onto the strip alone. It
    also keeps GAMBA's own `--red`, which a document-level repaint would have
    silently replaced with whatever red the palette shipped.
  - Social cards and a content-security policy on every page, not just the two
    newest.
- **`scripts/verify-kiosks.mjs`** — 104 assertions over the family: a clean
  console on every page, head parity, the strip's links, the pick surviving a
  navigation, the cabinets *not* repainting, the wall's bars agreeing with its
  verdicts, and no horizontal overflow at six widths down to 390px. It asserts
  on computed style and the console rather than on screenshots, because the
  defect it was written for was invisible in a screenshot.

### Changed

- **The agent wall shows what the app shows.** Each card carries the three bars
  read from that agent's own transcript — CTX WINDOW, FATIGUE, RELEVANCE — and
  the call they add up to, replacing the MODEL and EFFORT boxes with one
  `OPUS · MAX` chip. The verdict ladder mirrors `scripts/td-agent-vitals.mjs`,
  so a bar cannot read calm while the chip beside it says to act, and
  RELEVANCE diverges: high relevance is the good case on a roomy window and the
  alarming one on a full one. Codex panes draw no bars, which is the honest
  state of `#279` rather than an invented number.
- The wall's screenshot on the info page was two generations stale — it still
  showed the pre-card row layout and an effort gauge beside a model box that
  never once displayed a model. Regenerated from the live kiosk, and at half
  the file size.

- **An Omarchy kiosk.** `omarchy.html` joins the kiosk family (info · agents ·
  tv · global · gamba · start-crawl) and tells the desktop-integration story the
  README has been carrying alone: the two-shelf paint overlay, the eleven shared
  variant names, `SUPER+ALT+T` window adoption, Quickshell-rendered agent
  notifications, and the three-repo topology with `td-tint` as the seam.
  - The page **wears the themes it describes.** All 23 Omarchy schemes installed
    on a stock box are inlined as their named roles, and picking one repaints
    the whole page through the same role→slot mapping a painted pane uses —
    including the light schemes. `?t=<theme>` opens it already wearing one, and
    the choice is remembered.
  - The **agent-badge strip** is documented with the real mascot art and the
    real timings: the HEY blinker's 700 ms square wave and the 1.4 s bounce-eased
    breath, so NEEDS INPUT / WORKING / DONE / BLOCKED read on the page the way
    they read on the mother bar.
  - The page is built on a **scroll-driven backdrop and glass chrome**. The
    repo's own `crt-wall` screenshot sits on a fixed layer that drifts at 16% of
    scroll and scales to 1.30 across the page, blurred so it reads as phosphor
    bloom rather than as legible terminal text competing with the headline.
    Buttons, chips, cards and panels are frosted glass over a translucent tint
    of the theme's own surface colour, so the moving wall is visible *through*
    the chrome. The scrim opens at the hero and closes for prose, re-weighting
    itself for the light palettes; `prefers-reduced-motion` parks the backdrop
    and never arms the loop.
  - **The backdrop is the desktop's, and it changes as you scroll.** Every stop
    on the page — hero, seven sections, footer — names an Omarchy theme, and
    crossing into one cross-fades that theme's own shipped wallpaper onto the
    same fixed layer while the palette repaints: retro-82 → miasma →
    everforest → ethereal → tokyo-night → osaka-jade → catppuccin-latte →
    gruvbox. Colour and wallpaper are one pick on Omarchy, so they are one pick
    here. Each stop also rolls into focus through a central band (opacity,
    scale, a light blur) and the hero drifts off above it. Eight wallpapers
    cost 504KB after a 1600px cap and a webp re-encode, and each is fetched a
    screen before it is needed. The rail carries seven curated themes rather
    than all 23 — gruvbox, osaka-jade, tokyo-night, ethereal, everforest,
    miasma, retro-82 — laid out as a wrapping grid that never scrolls
    sideways, with a standing "follow scroll" toggle that says whether the
    scroll or a pick owns the page. `prefers-reduced-motion` still wears each
    stop's theme while nothing moves, and inside the last half-screen of the
    document every stop returns to full focus, so the bottom of the page is
    sharp at any viewport height.
  - **The MCP control surface gets its own section, second from the top** —
    `list_panes`, `pane_events`, `grep`, `get_pane_config`, `set_pane_config`
    and the push notifications, each with what it returns, plus the master
    switch and the appearance-only line. The nav gained an MCP entry.
  - **The theme picker scatters and reassembles as you scroll.** Two rails
    exist at once and one scrubbed number runs them in opposition: the hero's
    chips disperse on golden angles, blurring in proportion to their distance
    from home, while the ride-along's arrive from the right and converge into
    place. Both take clicks the whole way through — each chip sits in a slot
    that never moves and carries the handler, so the pill wherever it has flown
    to and the place it belongs are two targets for one action. Replacing the
    old threshold-and-DOM-move with a scrubbed value also fixed a restore that
    only worked sometimes: eight of eight returns to the top now land home.
  - **The theme picker rides along.** Past the hero on a window wider than
    1250px the rail docks to the right-hand gutter, vertically centred, as a
    narrow glass column carrying the seven themes and the follow-scroll
    toggle; scrolling back up returns it to its place under the headline. It
    is sized to the gutter (158px between 1250 and 1620, 214px above that) so
    it never covers the content column, and docking moves the node out of the
    hero — a transformed or filtered ancestor would otherwise make itself the
    containing block for a fixed child — while the vacated slot holds its
    height so the page cannot shorten under the reader.
  - **Figures open in a lightbox.** A screenshot reduced to a column width is
    unreadable; click, Enter or Space opens it as large as the window allows,
    and Escape, the backdrop or the ✕ closes it with focus returned.

- **Super+Ctrl-click reveals a path in the file manager**, where Shift- or
  Ctrl-click opens it. A pane full of printed paths — an agent's Links table,
  a build log — provokes two different questions, and only one of them was
  answerable by a click. The file manager comes up with the item *selected*
  (`org.freedesktop.FileManager1.ShowItems`, which Nautilus, Dolphin, Nemo,
  Thunar and PCManFM all export); a desktop exporting no such manager gets the
  containing directory opened instead. Works on a bare path and on a `file://`
  URI, percent-escapes and all, so a wrapped Links-table row reveals from the
  same click that opens it. The right-click menu carries the same action as
  **Reveal in folder**, shown only when the link names something on this disk.

### Changed

- **Opened links are scoped to the desktop, not to the terminal**, on a
  uwsm-managed session (Omarchy's). `xdg-open` now runs through `uwsm-app --`
  when `wayland-wm-app-daemon`'s socket is present, which is how the rest of
  such a session launches apps: the PDF a click opens gets its own systemd
  scope under `app.slice` instead of living inside the terminal's cgroup.
  Sessions without that daemon — GNOME, KDE, X11, a bare compositor — spawn
  exactly as before.

- **Paint with the desktop's own palettes.** The paint overlay now carries TWO
  shelves, turned with `z` (`shift+z` back) or by clicking a pill: the existing
  COLOUR SETS, and DESKTOP PALETTES — every Omarchy theme installed on the
  machine (`/usr/share/omarchy/themes`, `~/.config/omarchy/themes`, user themes
  shadowing stock ones by name; 23 on a stock box).
  - A palette replaces the pane's whole colour table and leaves its **texture**
    — scanlines, bloom, curvature, font — untouched, so a pane can match every
    other window on the desktop without giving up the look. It rides the same
    seam `$TD_PALETTE` already used, so there is one set of rules for both.
  - The ANSI mapping is **copied from Omarchy's own terminal template**, not
    invented: normal black is `background`, bright black is `muted`, cursor is
    `bright_foreground`. A pane painted `tokyo-night` therefore renders colour
    for colour like alacritty, foot and ghostty do under the same theme.
  - Each tile previews the scheme as a **miniature screen** in its own
    background carrying three of its own hues — a name alone cannot tell gruvbox
    from everforest. Light schemes wear a ☀ and sort last.
  - The keyboard survives the second shelf: the desktop's names collide
    (`catppuccin` beside `catppuccin-latte`, three `r`s), so a **letter cycles**
    through the palettes sharing it, painting each on the way past. `d` still
    means desktop, `esc` still folds. `z` may be a verb because nothing on
    either shelf is spelled with one — guarded by a test.
  - The pick persists per pane like any other paint pick; a palette that is
    later uninstalled falls back to the theme's own colours rather than failing.

- **The paint overlay plays from the keyboard, and the colour sets are the
  desktop's.** `terminal-delight ctl paint on` (the Omarchy 🎨 bar widget, or
  any script) still raises the palette over every pane at once — but now it is
  mouse-optional and reads like Omarchy's own picker:
  - the **focused pane is spotlit** — a thin scrim and a bright frame on it, a
    heavy scrim on everything else, so which terminal you are painting is
    answered from across the room;
  - **bare arrows** walk the wall in the direction you press (`ctrl` keeps its
    word-jump; the overlay is modal, so the plain keys are free);
  - a set's **first letter paints it**, drawn the way it is pressed — bigger,
    bolder, underlined in the accent — so the chord is legible from the tile
    instead of a legend elsewhere. `d` hands the pane back to the desktop,
    `esc` folds. A miss is a no-op, never a keystroke into the agent behind it.
- **Per-directory default logos — persistent and inherited.** Picking a pane
  logo now writes a directory default to
  `~/.config/terminal-delight/dir-logos.toml`: every pane whose cwd is at or
  under that directory wears the logo, across sessions, live as you `cd`
  (2s sweep). Mapping a child dir overrides its parent for that subtree; the
  picker says which directory a pick will bind ("↵ sets the default logo for
  ~/proj + subdirs") and its ✕ row clears the rule that currently applies. An
  explicit per-pane logo (MCP `set_pane_config`, or one saved by an older
  session) still shadows the map for that pane.

- **Alt+V / Alt+H split chords.** One-hand alternatives to `ctrl+alt+r` /
  `ctrl+alt+d`: `alt+v` opens the new pane beside the focused one (vertical
  divider), `alt+h` below it — Tilix-style naming. Listed in the `?`/F1 help
  panel. Costs readline's `alt+v` (page-scroll) and `alt+h` (mark-paragraph)
  in the shell, matching how `alt+r` was already claimed for the FOCUS reader.

- **Text-crawl mode** — a per-pane toggle that renders the whole terminal as a
  Star-Wars-style opening crawl: every line in the bundled News-Gothic crawl face
  (News Cycle, SIL OFL), centred and receding into the distance. The perspective
  is a GPU pre-map baked into the same CRT post-pass that curves the glass, so it
  composes for free with the barrel warp, screen jiggle, tracking band, glare and
  phosphor at one extra `pow` per pixel — no second render pass. Two per-pane
  dials in the DISPLAY tray (rides the grade group like warp/tracking): **angle**
  (2–30°, side convergence) and **depth** (0.05–15×, bottom-to-top text-height
  ratio). The 👓 FOCUS reader inherits the crawl font + centring (flattened for
  readability). Renderer change ships via `docs/patches/0003-text-crawl.patch`.
  Web tributes: a `start-crawl.html` kiosk and an `info.html` section.
  - _Known limitation:_ click hit-testing in a crawling pane stays barrel-only,
    so text selection is approximate — crawl is a display/nostalgia mode.

### Changed

- **One palette vocabulary, shared with the desktop.** The colour-set tray and
  the paint overlay now offer the **11 variants the Omarchy theme pack ships**
  (`army badger cherry ember glacier nuclear pineapple retro tide violet wood`)
  instead of 19 TD-only names, listed alphabetically so the paint letters run in
  reading order and every one is unique. Five sets took the desktop's name for
  the same colours — `snowflake`→**glacier**, `toxic`→**nuclear**, `ocean`→
  **tide**, `bat`→**violet**, `cyberpunk`→**retro** — and the old `retro` set
  (the slot-machine palette) is now **gamba**, after the theme it has always
  coloured. Renames are display-only: saved themes serialise the variant, not
  the label, so nothing you already picked moves. Sets no longer listed
  (`greenworks bolt amber gamba cotton-clowndy midnight retro-sunset galaxy`)
  still load from saved state — they are simply not offered.

### Fixed

- **Tabs no longer scrunch against the header icons.** The tab strip shared the
  mother bar's top line with the brand and was capped at 55% of its width, so
  four ordinary tab titles were already enough to fold it into a narrow column
  jammed beside the 🎨/📊/🤖 icons — unreadable at a glance and worse with every
  tab added. Tabs now get a ROW OF THEIR OWN beneath the brand, with the whole
  bar width: the common case doesn't wrap at all, and when it eventually does it
  grows downward without moving the brand or the controls.

- **Paint tiles with a two-word name folded mid-word.** With the desktop's own
  palettes on the second shelf the captions got longer (`catppuccin-latte`,
  `last-horizon`, `matte-black`), and a tile that wrapped where the text ran out
  read as `CATPPUCCIN-L / ATTE` and left the grid rows at ragged heights. Names
  now break on the **hyphen** onto a second line, in fixed-height boxes, so every
  tile is the same size whether its name takes one line or two.

- **Logo picker missed most of the filesystem.** The candidate scan walked the
  home root only 2 levels deep, so project brand assets
  (`~/ORG/Software/<proj>/assets/logo.png`) never appeared — only the picture
  dirs did. The walk is now full-depth (bounded by a 20k cap + heavy-dir skip
  list; picture dirs still scan first), and `.webp` counts as an image.

## [0.2.0] — 2026-06-15

The "now you can actually download it" release: a single MIT-licensed AppImage,
no more source-only. Plus a per-pane agent-finished bell and richer agent panes.

### Added

- **Prebuilt, MIT-clean AppImage.** `scripts/build-appimage.sh` produces a single
  self-contained `terminal-delight-x86_64.AppImage`, bundling a `cargo about`
  third-party license notice; CI builds it on every `main` push and **attaches it
  to the GitHub Release on version tags** (one-command install). Graphics libraries
  are loaded from the host (a GPU app must use the host driver stack).
- **Per-pane agent bell.** When a program rings the terminal bell (BEL) — as
  Claude/Codex do when they finish — the pane plays a configurable sound (trimmed
  clip, optional loop), raises a SNOOZE bar, and shows an always-visible `♪` mute.
  Five PD/CC0 default sounds are bundled and seeded on first run; playback is via
  the host `ffplay` (degrades silently without `ffmpeg`). See `BELL_SOUNDS.md`.
- **Agent panes.** Your own messages get their own colour (👤 colour-wheel pip) and
  `Alt+↑/↓` / ▲▼ jump between them; an agentic help section in the `?` modal.
- **Portability hardening** (toward running on untested boxes — AMD/Intel,
  Wayland, fractional scaling): vendor-agnostic GPU check in `scripts/setup-deps.sh`;
  an explicit monospace **font fallback chain** with a startup diagnostic when the
  default isn't installed (no more silent substitution); a startup log of the
  wgpu **GPU/driver** gpui selected; and **X11 PRIMARY-selection** copy
  (select-to-copy + write-on-copy, so middle-click paste works in other apps).
- Right-click context menu (Copy / Paste / Open link); `?` help modal; a split now
  inherits the seed terminal's working directory.

### Changed

- **Binaries are now MIT-distributable** — the project is **no longer source-only**.
  `docs/patches/0002-sever-gpl-crates.patch` removes the GPL-3.0 crates (`ztracing`,
  `zlog`, `ztracing_macro`) that the Zed graph linked via `gpui -> sum_tree`; they
  were trace-only. `app/deny.toml` now passes with **no GPL exceptions**.
  `scripts/prepare-gpui.sh` applies both patches.

## [0.1.0] — 2026-06-14

First public, source-only release. A GPU-native Linux terminal (Rust + gpui +
`alacritty_terminal`) with a hot-reloadable, CRT-flavored visual identity.

### Added

- **Real terminal core.** PTY + full VT emulation (bash, vim, top, tmux
  verified); live resize → SIGWINCH; full ANSI colour (16 themed + 256 +
  truecolor), bold/underline/inverse/dim; scrollback, mouse selection, copy/paste
  with bracketed paste.
- **Tiling multi-pane.** True tiling-tree splits (`ctrl+alt+r` / `ctrl+alt+d`)
  that divide only the focused pane, tab strip, `alt+←/→` focus movement, sub-tab
  drag-to-split/move, and a pop-out scratch window with sub-tab tear-off.
- **Hot-reloadable themes.** Four built-ins (`quiet-command`, `field-command`,
  `tactical-overdrive`, `hacker`) plus a live-editable `custom` slot read from
  `~/.config/terminal-delight/theme.toml` and reloaded on save (~300 ms). Theme
  picker with per-glyph captions and 1.5 s hover tooltips; the custom slot's
  tooltip shows its resolved path and an "Open in editor" action.
- **Per-pane appearance.** A pane's look splits into two independently-inheriting
  groups — the theme group (theme/seed/colour-mode/syntax) and the monitor-OSD
  grade group — each with a live, non-destructive "follow outer" toggle.
- **Monitor-OSD grading.** A display tray (global or per-pane) with
  brightness / contrast / colour / text / background / gamma, applied in HSLA at
  paint time, **plus a text-size channel** that rides the same inherit/override
  scope.
- **Seed colour wheel** for retinting a theme from a single accent colour.
- **CRT-lite effects** — scanlines, vignette, glow, and a per-pane barrel warp
  via the vendored `td-crt-pass` gpui renderer patch — all per-theme dials.
- **Desktop integration.** `scripts/install-hotkey.sh` registers
  `Ctrl+Alt+T` on GNOME to launch the app (reversible with `--uninstall`).

### Project / packaging

- MIT-licensed own source; binaries are **not** MIT-distributable because the
  vendored Zed/gpui graph links GPL-3.0 crates — see
  [`THIRD-PARTY-LICENSES.md`](THIRD-PARTY-LICENSES.md). This is a **source-only**
  release.
- CI gate: fmt + clippy (`-D warnings`) + tests + release build + `cargo-deny`
  (licenses/bans/advisories/sources) + browser-prototype checks.
- Contributor docs: [`CONTRIBUTING.md`](CONTRIBUTING.md), issue/PR templates,
  [`SECURITY.md`](SECURITY.md), [`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md).

### Platform

- Linux only (X11 & Wayland via gpui's wgpu renderer). Not macOS/Windows.

[Unreleased]: https://github.com/parker-brown-family/terminal-delight/compare/v0.3.0...HEAD
[0.3.0]: https://github.com/parker-brown-family/terminal-delight/compare/v0.2.1...v0.3.0
[0.2.0]: https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.2.0
[0.1.0]: https://github.com/parker-brown-family/terminal-delight/releases/tag/v0.1.0
