# Changelog

All notable changes to terminal-delight are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html) once it
reaches 1.0. Until then, `0.x` minor bumps may include breaking changes.

## [Unreleased]

### Added

- **A response carries a `brief` — the bare minimum, in two sentences under
  fifty words.** The tl;dr and the ELI5 came back as ONE register, which is
  exactly what was wrong with them as two: the brevity of the first and the
  plainness of the second in a single answer to *what is the bare minimum I
  need to know about this*. Serious, direct, simple; an explanation made small,
  never a metaphor. It is drawn **last** in the reading group and moves no
  default — the chip row reads `Plain brief · Technical brief · Brief`, and a
  card still opens on its plain reply, because somebody who has opened a card
  has already decided to read and the fifty-word version is the rung they drop
  to. Optional, and every reply sent without one draws
  precisely as it did before. Two migrations came with it: a legacy `tldr`
  beside a real `layman` used to be thrown away and now fills the brief, which
  is the length it always was, and `brief` — until now an undocumented alias
  for the plain reply — still parses on its own, because the resolution ladder
  falls back to it rather than refusing a payload that plainly has text in it.
  No reading is ever a second copy of another; a brief equal to the plain reply
  is dropped at the parser. (#XXX)

### Fixed

- **Standing on the comments board no longer swallows a keystroke meant for the
  agent.** For one build, typing while the board was on screen opened a note
  under the first character — the reasoning being that the bench already works
  that way and the board should too. It is the wrong extension of that idea:
  "just type" is valuable because there is ONE place a character goes and you
  never have to aim at it, and letting the shelf you happen to be READING choose
  the destination turns reading into a mode nobody entered on purpose. Typing
  goes to the agent on every shelf. A note is opened deliberately — `alt+m`, or
  the new row — and catches keys only once it is open. (#XXX)

### Added

- **A `+ write a note` row sits at the head of the comments board.** With
  type-to-open gone, `alt+m` was the only door, and a feature reachable by one
  undiscoverable chord is one most people never find. The row is a dashed
  outline rather than a filled box, because every real row on the rail is filled
  and a filled affordance at the top of a newest-first list reads as the newest
  note. It carries `ALT+M` on its right, so the mouse path teaches the keyboard
  path instead of competing with it, and it is drawn whether or not the board
  has anything on it. (#XXX)

### Added

- **`alt+1` … `alt+4` land on a workbench tab without reaching for the mouse.**
  `tab` already cycled the shelves and still does; this is for going straight to
  one. The keys are the digits rather than four hand-picked letters because the
  obvious letters were taken — `alt+v` splits the focused pane and `alt+b` is
  readline's word-back, which reaches the agent's own prompt from the bench
  composer — and because digits extend on their own the day a fifth shelf
  appears. The mapping is `Shelf::ALL`'s own order, so there is no second list to
  keep in step with the tab strip, and a test fails if any shelf's chord is ever
  one the window has already claimed. `alt+m` still opens the note box, which
  moves to the comments board on its way. (#574)

- **A pane's rail has a fourth tab, COMMENTS, and the agent has no wire to it.**
  The bench held an agent's replies, its artifacts and what it was asking, and
  nowhere to put your own thinking — the only answer was the sticky note on the
  glass, which is one slot in handwriting built to be read across a wall of
  panes rather than written into. The board is the quiet plural version: notes
  in the palette's `human` colour, newest first, persisted through the same file
  transport every surface uses so they survive a restart. `alt+m` opens the box,
  and on the comments shelf simply typing opens it, the way typing anywhere else
  on the bench already starts talking. Nothing you write there reaches the agent
  — no bytes down the pseudoterminal, no line in the action journal, and
  deliberately no verb that hands a note over. `copy` is the whole escape hatch,
  so the words move when you decide they should. A comment is a ninth surface
  kind rather than a store of its own, which is why it arrived with persistence,
  restore, the history cap and the unseen mark already working. (#566)

- **ctrl+wheel sizes exactly what you are pointing at — four dials, one chord.**
  It used to size the cabinet from anywhere in the window, which meant the
  things a person most wants bigger — the terminal they are reading, the
  workbench they are reading — were the two it could not touch. There are now
  four separate answers and the pointer picks between them: over the **outer
  chrome** (menu bar, tabs, left bar, the gaps) it scrubs the cabinet exactly as
  before; over a **pane's own header** it scrubs that one pane's header chrome;
  over the **terminal grid** it scrubs that pane's terminal text, so the font
  and cell height move together and the shell reflows; over the **workbench**
  it scrubs the bench's type ramp. Only the dial actually turned becomes that
  pane's own — every other one keeps following the outer theme — and each is
  written into the layout like any other appearance change. The keyboard help
  has advertised `A──A · Ctrl+wheel` under "Text size" all along; it is true now.

- **The workbench has its own size dial.** A new `bench_size` grade channel,
  beside the existing text-size one on the DISPLAY tray and in the MCP
  `set_pane_config` API. It starts **unset**, which is not the same as `1.0`:
  unset means the bench follows the terminal's dial, which is what the two faces
  did before they were split and what every session already on disk describes.
  So nothing shrinks on upgrade, and the two only come apart once somebody
  turns one of them.
- **Drag a file onto the bench and its path lands in the line you are
  writing.** The composer lights up in your own colour while the file is over
  it, and letting go types the path at the caret — mid-sentence, with the rest
  of the sentence intact, because placing a caret already sends the agent's
  line editor the matching arrows. Several files come in as several words, and
  a name with a space in it arrives quoted, so `Screenshot 2026-09-18.png`
  stays one filename instead of two arguments nothing downstream can rejoin.
  A drop on the terminal face pastes the path the way every other terminal
  does. Files only: the renderer discards a drag whose contents are not local
  files before the app is told, so an image dragged straight off a web page
  still does nothing. (#561)

### Fixed

- **ctrl+wheel sizes the workbench when you are standing on the workbench.** The
  chord reached the terminal grid, a pane's header and the outer cabinet, and
  did nothing at all on the one face that had just been given a dial of its own.
  The bench paints a capture-phase wheel hook — it has to, because under the
  curved tube gpui hit-tests flat and cannot tell which box a turn is over — and
  that hook runs ahead of every bubble listener and swallows the turn, so the
  pane's own handler never saw it. It looked like it worked from the outside:
  sizing the terminal and flipping back showed a resized bench, because an unset
  bench dial follows the grid's. The chord is now one method both handlers ask,
  and a test enumerates every wheel handler on a pane and fails when a new one
  appears without answering it — which is the shape of this bug rather than this
  instance of it.

- **A surface presented over MCP now reaches the disk, so the bench survives a
  restart.** `present_surface` handed the document to the live window and
  stopped there, while this module's own header drew all three transports
  converging on `surfaces/<session>/<pane>/*.json` and promised the directory is
  re-read when a window opens. The file drop kept that promise and the verb did
  not, so everything an agent sent through MCP died with the window — a pane
  that had presented four surfaces had no directory at all, and the overview
  came back reading "No responses yet". Three things had to be true at once for
  the round trip to cost nothing: the document is written under the id it was
  filed as, because `parse` invents a fresh `anon-…` for a document that names
  no id and the surface would otherwise return from disk as a stranger; a
  retire takes the file with it, or it reappears on the next restart; and a
  second arrival cannot blur who wrote the first — the watcher re-reads the
  file the verb just wrote and delivers it as a `FileDrop`, which by design
  cannot name a writer, so an origin now only ever gets more specific. (#567)

- **One pane, one conversation.** A bench, a tool glyph, a pane's tool-call feed
  and a desktop recap all asked the same question — *which conversation is this
  pane in?* — and all asked it one pane at a time, ending in "the newest
  `.jsonl` in that project directory". That is the same answer for every pane
  sharing a directory. With fourteen agents in one repository, eight of them with
  nothing on this machine naming their session, eight benches read one
  conversation and each showed that agent's deliverable as its own. The wall
  already did this properly — one pass, every transcript claimable once — so its
  resolver is now the only one: `paneident` binds the whole window at once and
  labels each binding with the rung that produced it (a claim the agent pushed, a
  process that started when the conversation opened, elimination, or a
  preference between live conversations). Readers that attribute work take the
  first three and refuse the fourth, which means a pane that cannot be placed
  shows nothing rather than its neighbour's work. The per-pane resolver is
  deleted rather than deprecated, and a source scan fails the build if one comes
  back. Two rungs were added on the way: the session id Claude Code's own
  scratchpad descriptor names, which is the only thing left that binds a
  conversation to a *pid* now that transcripts are opened and closed per write;
  and the ledger the SessionStart hook pushes, which the wall was not consulting.
  Codex panes have no fleet pass yet and get the honest half of one — a pane
  alone in its directory is bound by elimination, a crowded one only by naming
  its own session, and two panes pointing at one rollout are both demoted, which
  is not theoretical: the rollout lookup matches a cwd as a SUBSTRING, so a pane
  in `/home/parker` and a pane in `/home/parker/PROJECT` selected the same file
  while each looked alone. (#564)
- **A ledger entry with a space in it is still a ledger entry.** The JSON reader
  behind the agent-session ledger matched the literal `"session_id":"` — the
  shape a compact writer emits, and not the one any pretty-printer does — so a
  valid hand-written entry parsed as *no id at all* and the reader fell through
  to forensics as though the file were absent. Whitespace around the colon is
  allowed now, and a non-string value is still refused. (#564)
- **Two numbers that said nothing are gone.** A pane header carried its own
  grid size — `194×50` — in among the controls, and a response's group tab
  carried a doubt count as `·2`. Neither is a number anybody acts on: the grid
  size is a fact about the window you are already looking at, and the count
  sits on a tab whose own label is one click from the doubts themselves.
  Parker, on the pair: *"that -2 shouldn't be there"*, and *"the number 194x50
  for the pane resolution in the pane header - can go away also"*. The doubts
  are untouched and still one click away; only the badge in front of them is
  gone.

- **Pasting a filename with a space in it no longer breaks it in two.** The
  clipboard's file arm joined paths raw, so a copied `Screenshot
  2026-09-18.png` arrived as two words. It now goes through the same quoting a
  dropped file does. (#561)
- **The shelf strip wraps, so a fourth tab does not fall off the rail.** The rail
  is a share of the pane, clamped between 132 and 208 points, and four tabs
  measure about 161 of them. The strip sat in an `overflow_hidden` frame, so the
  overflow would not have read as a layout problem — the last tab would simply
  have stopped being drawn, and a tab nobody can see is a shelf nobody can
  reach. The three-tab strip was fine at every width. (#566)

- **Two corners on the bench stopped ignoring the skin.** `benchdraw.rs` has
  promised since it was written that a guard test caught literal corner radii.
  There was no such test anywhere, and two `rounded(px(3.))` had gone in
  underneath the promise — the paste chip and the `LIVE → AGENT` chip in the
  composer — both of which would have stayed round under a square skin while
  every other corner squared. The gate is real now, it lives beside the three
  other source scans in the file it guards, and the module's header points at it
  instead of at another module. (#566)

- **A note stamped from a broken clock says so instead of inventing a date.**
  `localtime_r` does not refuse absurd input: handed a garbage millisecond count
  it answers `3 Apr 584556019` without an error, which is an invented value with
  the right shape — the kind every later reader takes for a measurement. A year
  outside 1900–2999 now resolves to `time unavailable`. (#566)

- **The small print on a card is small, not invisible.** Every subtitle, cost
  line, consequence, section tag and provenance line on the workbench was drawn
  in the palette's `faint` role. `faint` is furniture — it is what a divider is
  mixed from — and a word painted in it lands between **1.22:1 and 1.62:1**
  against the ground under it on the six palettes we ship. 1.0 is two identical
  colours. Forty-two sites now take the skin's meta ink instead, and that ink
  moves from 0.45 to 0.60 of the foreground, which is the first rung on which
  every builtin palette clears the 4.5:1 small-text floor — the worst of them at
  4.54:1. A new test walks all six and goes red under either of the old inks.
  (#555)
- **A panel holds its contents off its own border.** The inset lived at the call
  sites, and seven of the bench's fourteen panels forgot it: an eleven-point line
  of text sat with its descenders on a lit border and a ten-pixel corner arcing
  through its first word. The inset is the skin's own `pad_x`/`pad_y` now and it
  is applied in `Skin::panel`, so a region has to opt OUT rather than remember to
  opt in. The seven that already padded are untouched — their own padding still
  overwrites it — and the double rule still hugs the edge, because taffy resolves
  an absolute child's insets against the border box and never subtracts padding.
  (#555)
- **The primary button on a card stops shouting.** APPROVE arrives as a chip,
  which already carries a lit border, a seat and a halo, and was then given a
  second border in a second hue plus `aglow` — the bloom sized for a whole
  region, 22 pixels of blur at `glow × 0.45`, against the ring's 5.25 at 0.11.
  Four times the spread at three and a half times the heat on a box the width of
  one word, and the tube's own bloom pass multiplies whatever the chrome emits.
  `verb_button` now adds only the size, which is what makes a button a button;
  the strip's launch verb takes the new control-scale `Skin::halo` rather than a
  region's. The empty workbench's launch button keeps the big bloom on purpose —
  it is alone on the surface, with nothing for it to close over. (#555)
- **A tab you can read.** Every tab in the window — the bench's shelf strip, the
  strip across the top, the rows in the left bar — drew the lit one's label in
  the selection colour, inside a border in the selection colour, on a seat mixed
  from it, under a bloom of it: four devices, one hue, and the word was the only
  one of the four anybody had to read. The unlit ones took the "not in effect"
  ink, which is Faint, and came out as grey words floating beside a glowing pill.
  Measured on the `quiet-command` palette, the lit label ran at **1.38:1**
  against its own seat and the resting one at **1.08:1** against its face; 1.0 is
  two identical colours. The hue now goes on the edge, the seat and the halo and
  the label stays the foreground, at two weights; a tab at rest keeps a quiet
  bordered face, because a control is a control whether or not you are standing
  on it. The phosphor is dialled from 0.41 to 0.11 and the crisp spread-ring is
  gone — on a box five pixels wider than its own word it closed over the glyphs
  from every side. A new test walks every palette we ship and fails under either
  of the old inks. (#555)
- **TERM ⇄ BENCH is one switch.** It was two chips with two pixels between them,
  each reserving its own ring, so one binary choice put four vertical edges on
  the pane header and the lit half was a pill you could not read the word inside.
  One bordered track now, with the half you are on filled and carrying the
  control's whole phosphor budget, so throwing the switch moves the glow. (#555)
- **The overview shows what YOU said, over the reply to it.** The bench's
  overview is the feed of what the agent said, and for a while it was only
  that: the newest reply stood in the room with nothing above it, so the one
  thing a person could not read on the surface that holds the answer was the
  question they had just asked. It was legible in the pane's mirrored
  conversation and on the agent wall's card, which are two other places. Your
  own message now sits above the reply as its own indented block, in the ink
  the terminal already paints your turns in, pinned outside the card's scroll
  so a long answer cannot take it off the surface. It is read out of the
  pane's own scrollback rather than kept as a second record, so a turn typed
  at the terminal face counts the same as one sent from the composer; a
  message longer than the block ends in an ellipsis rather than stopping
  mid-word, and one that has scrolled out of history says so instead of
  drawing an empty block. Drawn for the reply that is STANDING IN and never
  for a card opened off the rail — this window can only read the latest
  message, and captioning a four-turn-old answer with a new question would be
  a pairing nobody made.

- **A new agent starts as whatever you told it to.** The LAUNCH AGENT panel
  opened on three constants — claude, the first model in the list, the harness's
  own effort — and every launch that wanted something else paid for it in
  keystrokes, every time. Three rows at the top of the usage card (the `Σ usage`
  face of the `</>` card, which the subscription slot in the bottom-left corner
  opens) now set them, and the panel opens holding them. The store keeps
  *nobody has chosen* apart from *somebody chose exactly what would have
  happened anyway*: an unset row still shows the value in force, drawn with a
  quiet edge and tagged `unset`, against the accent and `chosen` of a decision.
  Pressing the lit chip again clears it back to the harness's own. The block is
  one folded line — `LAUNCH AGENT CONFIG`, press to open — because the card's
  own subject is what each plan has left, and three rows of somebody else's
  question above the answer is three rows of noise. It stays open for the rest
  of the window once opened.

### Fixed

- **Clicking the group you are already in no longer puts the whole session
  across the top.** Pressing a branch row in the tree a second time backed the
  strip out to ALL, which on a 31-tab window is 31 tabs over three wrapped
  rows: *"the outer is tab bombed with ALL our tabs again"*. The gesture was a
  trap because its first half is invisible — pinning the branch the strip is
  already resting on draws the same tabs under the same chip label, so the
  second press is made by somebody who reasonably believes the first did
  nothing. Backing out now lands on the resting scope, the branch you are in,
  which is where the person pressing a row twice was trying to stay. The
  UNFILED heading, which asks for ALL by name, becomes a real toggle in the
  same change rather than a one-way door, and the chip stops carrying its own
  copy of the two lines — every control that widens the strip asks one
  function which way the next press goes. (#546)

- **Return starts the agent on a bench that is offering one.** A workbench with
  no agent in it shows a single LAUNCH AGENT button, and the key that means *do
  the obvious thing* did nothing at all there: return takes the selected
  surface's first verb, and a bench nobody has run an agent on has no surfaces.
  It now opens the launcher — unless a card is open, whose first verb still
  wins the key.

- **A dial's list drops under the dial that opened it.** The model and effort
  lists were placed at a fixed offset from the rail, which put both of them
  under the END SESSION button at the far end of the strip whichever dial had
  been pressed. Each now hangs from its own button, sharing its right edge.

- **The open list lights the value the dial is showing.** The button resolved
  what a pane is running from three sources — a press on the dial, the
  `--model` on the command that started the agent, then the harness itself —
  and the list underneath it looked only at the first, so a pane launched with
  `--model opus` read `OPUS` above a list with nothing marked in it. Both now
  ask one resolver, and the row's ink carries the same claim the button's does:
  accent for a value somebody chose, half-strength for one read off the launch
  command.

- **The agent wall's header no longer counts the fleet twice.** Six unlabelled
  glyph counters sat between the name and the token totals, setting the same
  state filter the bordered WORKING / DONE / IDLE chips two rows below set — the
  same control twice, one of them unreadable, and their numbers were fleet-wide
  while the chips' are context-aware, so the two rows contradicted each other
  whenever any filter was on. The chips stay; the glyphs are gone.

- **Changing the model or the effort no longer sends your half-written prompt.**
  The bench composer mirrors the agent's own line editor, so a draft is already
  sitting on that line — and the dial announced its new value by putting
  `/model …` into the composer and sending it. One press posted the unsent
  prompt with a slash command glued to the end, answered at the strength it was
  being changed away from, and left the box looking as though the prompt had
  been thrown away. The command now goes in beside the draft: the line is
  cleared, the command is sent on its own, the draft is typed back and the caret
  returns to the character you stopped at. A pasted image does not survive that
  erase — see #557. (#556)

- **The model and effort a pane is running stay readable while it is working.**
  The bench's two dials switched to the faint ink at 0.45 alpha whenever a press
  would not be read — which is the whole of a turn in flight — so a busy pane
  carried two outlined boxes with nothing legible in them, on a header lit green
  because the agent was busy. The value is a fact about the process, and a turn
  in flight is when a person most wants it: which model is spending this, and at
  what effort. Pressability is now drawn by the caret and the cursor, and the
  ink carries one claim only — whether anybody chose the value. (#553)

- **The tab strip opens on the group you are in.** Teaching the strip to obey
  the scope chip left the chip's default on ALL, so a window opened with every
  tab in the session across the top: the branch a person was actually working in
  spread over two wrapped rows and mixed with a dozen they were not. The chip
  now starts on the branch holding the active tab, names that branch instead of
  reading ALL, and follows when the active tab moves — a sibling group under the
  same project stays off the strip. The whole session is one press on the chip
  away, and one press back. A narrowing lasts as long as the window: the scope
  is no longer saved, because every state file written since the chip shipped
  says ALL and no loader can tell that apart from somebody asking for it. (#546)

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
