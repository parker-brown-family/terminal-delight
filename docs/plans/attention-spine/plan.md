# Plan: Attention Spine

**Difficulty 4/10:** one optional surface over an existing pane wall; false confidence in the attention model could survive unnoticed, while the UI remains cheap to unwind.

## What it is for

Terminal Delight already makes several terminals in one tab feel good. The right edge should preserve that strength while removing one repetitive job: sweeping panes to discover which agent needs judgment. Closed, it is a narrow status spine. Open, it is an ordered human-handoff queue. Selecting finished work reveals a review tray. Every row carries compact `project:group` provenance.

## What the remarks settled

- The human-handoff queue is the default face.
- The review tray is the second layer.
- `project:group` is enough provenance on a row. There is no separate provenance tree.
- The risk radar is removed. Concrete failures remain queue items; there is no risk score or permanent risk panel.
- The terminal grid keeps priority. Opening the rail overlays it; pinning is an explicit wide-screen choice.
- Version one remains read-only: focus a pane or open existing evidence. It does not send prompts, retry agents, merge work, or create worktrees.

## What the 15 September remarks settled

This plan was drawn as eleven pictures (`reports/2026-09-15-attention-spine-groundwork.html`,
under `~/Work/reports`) and came back annotated. Four decisions, in Parker's words:

- **The drawings match the intent.** *"All very very good!"* — the surface, the machinery and the
  fence are agreed as drawn, so the shape below is settled rather than proposed.
- **Held panes are out of version one.** *"agree - held panes are out."* See the last section.
- **Slice 1 is approved**, with the amendments below written in first. *"concur."*
- **The plan and the prerequisite ticket leave the parked clone.** *"affirmative. I concur."*
  This file is that move; it was written in a private repository whose last push predated the
  work by a month.

## The shape

### Product contract

1. **Closed:** a narrow right-edge spine shows a known handoff count plus an unknown marker when present.
2. **Open:** an overlay lists decision, failure, finished-unseen, then known-agent/unknown-state items. Opening it does not change pane bounds.
3. **Selected:** a row exposes reason, source, observation time, and `project:group`. Finished work may expose the review tray.
4. **Pinned:** an opt-in wide-screen mode reserves width. It is never the default.
5. **Unavailable evidence:** the UI says `unavailable` or `unknown`; it never prints an invented zero or a green empty state.
6. **Reading is not visiting.** Opening the rail, selecting a row and reading a review never moves
   keyboard focus into a pane. Focus is a verb the reader presses.

### Interfaces that matter

These are prospective contracts, not implementation.

```rust
enum PaneKind {
    Agent(AgentKind),
    Shell,
    Unknown,
}

enum AgentState {
    Working,
    Blocked,
    Error,
    Finished,
    Idle,
    Unknown,
}

struct Origin {
    project: Option<String>,
    group: Option<String>,
}

enum AttentionKind {
    Decision,
    Failure,
    ReviewReady,
    Unknown,
}

struct AttentionItem {
    pane_id: PaneId,
    origin: Origin,
    kind: AttentionKind,
    reason: String,
    observed_at: Option<Instant>,
    evidence: Vec<EvidenceRef>,
    seen_at: Option<Instant>,
    /// What this turn produced, declared by the agent. Never inferred from a filename.
    deliverable: Option<Deliverable>,
}

struct Deliverable {
    label: String,
    /// `file://` or `http(s)://`. Opened by a plain click in the rail.
    href: String,
    declared_at: Instant,
}
```

The main flow is:

```text
pane screen + bell + existing labels
    -> evidence-preserving observation
    -> fixed attention projection
    -> collapsed count / expanded queue
    -> focus the existing pane or open sourced review evidence
```

The current parser ends every unmatched screen at `Idle`. That state must become explicit before the spine can claim an accurate count. A plain shell, an agent at rest, and an unknown agent are separate facts.

## Amendments from the code

Five things the repository already decides, read in `~/Work/td-cutover` on 15 September. Each is a
sentence to honour in a slice rather than a change of direction.

1. **Origin resolves from the tree, in three states.** `Project { id, name: Option<String>, color,
   collapsed }` is persisted in the layout, and tabs and groups each carry a project id; the only
   path-derived naming is the adopt gesture a person triggers. So nothing needs inferring from a
   path — and an unnamed project is not an unknown one. Render named, unnamed-but-identified, and
   absent as three distinct things.
2. **`observed_at` needs a producer.** No `Instant`, `since` or `changed_at` exists in the
   classifier or the wall, so age is not merely unexposed, it is unrecorded. Slice 2 stamps an
   instant when the projected state changes, and the field stays `Option` because "not seen to
   change yet" is a real answer.
3. **One precedence function, not two.** `agent_badge(needs_input, working, bell, blocked)` already
   collapses these signals into a single badge, and its test table encodes the awkward cases: a pane
   back at work with last turn's bell still latched reads as working, and blocked without a bell is
   stale classification and says nothing. `AttentionKind` derives from that function; the tab badge
   is the ranking a person's eye checks first, and two rankings of one pane is a defect.
4. **Seen is the bell acknowledgement that already exists.** The finish edge acknowledges on the
   spot when that pane holds focus, and the focus-in edge acknowledges otherwise; an unwatched
   finish already raises a desktop notification titled `tab → pane` carrying a recap of the agent's
   last words, whose click parks a jump. Slice 1 adds no second notifier and Slice 3 reuses that
   latch.
5. **Blocked goes sticky when a pane goes quiet.** The predicate runs over the visible tail and a
   finished prompt never scrolls away, which is how an earlier needs-you blinker pinned on
   permanently. Slice 2 gives blocked a clearing edge — a keystroke or focus in that pane — or
   narrows the match so a resting frame cannot satisfy it.

## This turn's deliverable

The strongest remark on the blueprint was not about the rail's shape. Reviewing the page's own links
table, Parker wrote:

> These links are KEY for the review… Agents must be able to provide THE DELIVERABLES as a single
> separate section if applicable… ie. agents will know they are producing a deliverable for THAT
> TURN (currently links are showing historical docs which is a pain point)… AND THAT DELIVERABLE
> will be highlighted as a CLICKABLE link in the review (not control click like in the actual pane!)

and, on the same picture:

> When a review is being viewed, we should not automatically focus the agent, but we should be ABLE
> to navigate to focus the specific pane FROM that review card.

Four consequences, all of which belong to the review tray in Slice 4:

- **A review row's primary object is what the turn produced.** The links an agent prints today mix
  this turn's output with everything it referenced, so a reader cannot tell them apart. The rail
  shows the declared deliverable first, with its label, and opens it on a plain click — a rail is a
  UI surface, not a terminal, so no modifier is needed to follow a link.
- **A review row therefore carries two actions**, which is a deliberate departure from one verb per
  row: *open the deliverable*, and *focus the pane*. Reading never does the second on its own.
- **Declared, never inferred.** A deliverable arrives because the agent said so. Parsing a links
  table out of prose would guess, and guessing is the thing this plan refuses everywhere else. The
  narrow channel already exists in shape: the MCP note verb writes a pane's sticky note today, and a
  sibling verb that records a label and a target is the same contract with a typed payload. An
  agent that declares nothing gets a row that says *no deliverable declared* — which is a fact, not
  a blank.
- **This half can start before the rail exists.** The declaring convention is useful on its own: an
  agent that names its deliverable for the turn is already easier to come back to, whether or not
  anything renders it. It is the cheapest piece of this plan to prove and the only one whose value
  does not depend on the surface shipping.

### Opening one

A deliverable opens on a **plain click**, with no modifier. A terminal pane needs one because a
click there belongs to the program underneath; the rail is a UI surface and owes the click to
nobody.

**The desktop opens it, not the terminal.** The row hands the target to the system handler — the
same `open_with_system` path a clicked link in a pane already uses, which routes through
`uwsm-app` on a uwsm session so what opens is scoped to the desktop and outlives the pane that
produced it. A rail that hard-wired its own viewer would override a choice the person has already
made in their MIME database, and on this machine that database is set deliberately: `.md` resolves
to `text/markdown` and opens in `markdown-delight`, `.html` opens in the browser.

**The two shapes that matter are HTML and Markdown**, so the row names which it is before you
click — a rendered report and a plan document behave differently and a row that mislabels one is a
row lying about what the click will do. Anything else says `open` and nothing more. The
classification is by extension, query and fragment stripped, and it is a pure function with tests
rather than a guess at paint time.

**Nothing is inferred.** A turn that declared no deliverable gets a row with no link, not a link to
the newest file near it. The tracer in Slice 1 proves the path early, with two real documents from
this repository — the plan below and one of the HTML design notes — so that the click, the handler
and the two viewers are exercised before any of the declaring machinery exists.

## Attention levels: promoted, neutral, demoted

Right-click any row in the left bar — a project, an initiative or a task — and set what it is
worth today. Three values and no more: **promoted**, **neutral**, **demoted**. Neutral is the
default and draws nothing, because a tree where every row carries a glyph is a tree where none of
them mean anything.

The three are not severities, they are where the work sits in your day — **main focus, side task,
background** — and that is why they outrank everything the agents are doing. Parker, settling it:
*"human provenance: 'THIS IS IMPORTANTER' is central."* No parser can observe that, and no
heuristic should be allowed to overrule it.

The context menu already exists on all three rows (`BarMenu::{Project, Initiative, Task}`), so
this is two rows added to a menu rather than a new gesture to learn.

### It inherits downward, and nothing travels up

Setting a project promotes everything under it. Setting an initiative promotes its tasks. A row's
**effective level is the nearest explicit setting at or above it**, which is what lets one noisy
task sit demoted inside a promoted project — the case that makes the whole feature usable rather
than a blunt instrument.

This is deliberately the opposite direction from everything else in the bar. `tree::Roll` gathers
agent state from panes into tabs into projects, so a branch row can say what is happening beneath
it; a level goes the other way and **does not roll at all**. A branch's arrow says what that branch
is set to. It never means "something inside here is promoted", and a collapsed branch summarises
nothing — the marks appear on the children when you open it.

### The mark

A green up arrow, a blue down arrow, or nothing, in the row's badge line beside the pins and the
state badges. **An inherited mark draws dimmer than one set on that row**, because otherwise there
is no way to see which row to right-click to clear it — and a setting you cannot find is a setting
you cannot undo.

**No numbers in that space, ever.** Parker, on the mark: *"we do not include NUMBERS in the right
glyph space — that is noisy."* The badge line is glyphs; a quantity belongs on the spine's count or
the strip's `⋯n` chip, where it is read deliberately rather than scanned past twenty times a
minute. This is the grain the bar already has — `roll_glyphs` renders a branch's roll-up as a line
of glyphs rather than a tally — and the level must not be the thing that breaks it.

### What it does to the queue

Level is the **outermost sort key**, above the lane:

```text
priority  →  lane  →  age  →  pane
```

The consequence, stated plainly because it is the part to argue with: **a promoted review sits
above a neutral decision.** Promoting a project is a person saying "this is what I am doing today",
and a surface that then buries it under someone else's rate limit has overruled him with a
heuristic. Inside one level the lane order is unchanged, so the rest of the plan stands.

### What it is not

- It never hides. A demoted row still appears, last.
- It never mutes. The bell, the desktop notification and the tab badge are untouched; this is an
  ordering, not a filter.
- It never changes the count. A demoted agent blocked on a prompt still wants you, and the number
  says so.

## Evaluation contract

The baseline is currently unknown and remains unknown until recorded. Before implementation is called successful, replay the same two-pane and eight-or-more-pane event sequences with the current manual sweep and with the spine.

The spine passes when:

- every seeded decision, failure, and finished-unseen event appears exactly once;
- no known-agent unknown state is reported as idle;
- opening the overlay leaves every pane bound unchanged;
- selecting a row focuses the correct existing pane in one action;
- reading a row or a review moves focus nowhere on its own;
- every visible fact names its source and observation time, or explicitly says they are unavailable;
- the spine requires fewer unrelated pane openings than the manual sweep for the same event sequence;
- a promoted branch's work appears above every neutral row, and a demoted row still appears.

Record time-to-correct-pane, unrelated panes opened, missed known events, false positives, and unknown observations as separate measurements. Missing measurements stay blank or unknown. If unrelated pane openings do not fall, the attention-surface hypothesis fails even if the UI works.

## Least confident decisions

1. **Project label source.** *Settled by the code, 15 September.* TD carries a first-class project
   record in its layout and both tabs and groups reference it, so the label is assigned rather than
   inferred. Rule: resolve `Origin` the way the tree resolves it — the group's project when a tab is
   grouped, the tab's own field when it is not — and keep named, unnamed and absent distinct. The
   earlier worry was right in spirit: no path is ever consulted at render time.
2. **How unknown enters the count.** Preferred rule: known-agent/unknown-state gets a neutral unknown row after handoff work; unknown pane kind gets a separate marker and does not inflate the red attention count.
3. **Finished versus review-ready.** The bell can establish finished-unseen today. Changed files, checks, and artifacts require an authoritative source. The review tray must show unavailable fields until that contract exists. A declared deliverable is the one piece of review evidence that needs no new contract, because the agent supplies it.
4. **Seen lifetime.** *Narrowed, 15 September.* Reuse the existing bell acknowledgement for the
   tracer bullet — it is a real function with a focus-in edge, not a convention to invent.
   Persistence across restarts stays deferred until observation shows it matters.
5. **Whether level outranks lane.** *Settled 15 September: yes.* Both orderings were drawn side by
   side and the answer was the human-provenance one — main focus beats side work beats background,
   whatever the agents are doing. A promoted review therefore sits above a neutral decision, and
   that is intended rather than tolerated.
6. **Whether a demoted row still counts.** *Settled 15 September: yes.* Demoting says "not first";
   a count that quietly drops work is the idle fallback wearing different clothes.
7. **Nearest explicit setting wins.** *Settled 15 September: yes*, so one noisy task can sit
   demoted inside a promoted project.
8. **The mark.** *Settled 15 September:* green up, blue down, nothing for neutral, dimmer when
   inherited — and no numbers in that space.

## Slices

1. **Tracer bullet:** feed synthetic decision, failure, finished, and unknown observations through the projector into a collapsed spine and overlay; activate a row through TD's existing pane-focus path; and carry a declared deliverable on the rows that would have one, opened by a plain click through the desktop's own handler.
2. **Explicit live state:** add pane-kind and unknown-state distinctions, then connect the existing HUD parser and bell without changing queue behaviour. Carries amendments 2, 3 and 5: stamp the transition instant, derive the kind from the badge predicate, and give blocked a clearing edge.
3. **Queue lifecycle:** add fixed ordering, visible reason/source/time, `project:group` resolved per amendment 1, seen/unseen completion on the existing bell latch, and keyboard movement.
4. **Review tray:** attach sourced change/check/artifact evidence to finished items; unavailable evidence remains visibly unavailable. Carries the declared deliverable, its plain-click open, and the separate focus action.
5. **Attention levels in the tree:** the two context-menu rows on project, initiative and task;
   the level persisted in the layout beside the project record; downward resolution with the
   nearest explicit setting winning; the badge-line mark, with an inherited mark drawn dimmer. The
   projector's ordering key landed early, alongside Slice 1's core, because it is one comparison
   and deferring it would have meant rewriting the sort and its tests twice.
6. **Responsive behaviour and evaluation:** add opt-in pinning, verify overlay geometry and focus handling, then run the recorded comparison against manual pane sweeping.

## Deliberately outside this plan

- Risk scoring, risk gauges, and a risk-radar destination.
- A separate provenance tree.
- Prompt sending, agent retry, worktree creation, merge, ship, or task orchestration.
- A full diff viewer inside the narrow rail.
- Invented project names, check counts, file counts, or healthy states.
- **Held panes.** The approved close semantics keep a closed pane's process for an hour, or four
  when two or more went at once, reopenable with `ctrl+shift+z`. Such an agent has no screen to read
  and no pane to focus, so it reaches none of this plan's observation sources. Version one therefore
  scopes its count to visible panes **and says so on the surface**; a count that is quietly short is
  the same defect as an unknown pane reported as idle. Revisit in the slice that builds the
  held set, where the data will exist.

Approval of this page permits Slice 1. Until then, the work remains planning only.
