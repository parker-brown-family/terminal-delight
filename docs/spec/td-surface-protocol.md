# TDSP — the Terminal Delight Surface Protocol

**Version 0.4 · text revised 2026-11-01 against the code · implemented in `app/src/surface.rs`**

Every example in this document parses under the build it describes: 0.4.0
(`main`) and the 0.5.0 development line, which share this protocol's code. The
few places where 0.5.0 behaves differently say so with a **0.5.0
development:** lead-in.

An agent hands Terminal Delight a small JSON object describing **what it made**.
Terminal Delight decides how that thing looks. This document is the contract
between those two halves.

```
   agent                    TDSP                    Terminal Delight
  ───────                  ──────                  ──────────────────
  "I made a         →   { "kind":              →   the architecture
   diagram of          "architecture",             renderer, this theme,
   the pipeline"        "model": {…} }              this skin, this pane
```

---

## 1 · The one rule

**Describe meaning. Never describe layout.**

There is no width in this protocol, no colour, no font, no component tree, no
ordering of boxes on a screen. An agent that could send those would eventually
send a layout that does not belong in this window — and every renderer here has
to work inside a bent phosphor tube at 344 pixels, under a theme the person
chose, with a skin that decides whether corners are round.

So the agent owns the *meaning* and the window owns the *pixels*. That split is
the whole design, and it is also why a payload is small enough to write by hand.

Google's A2UI reached the same conclusion from the other direction: it ships
both a dynamic schema, where the agent invents the component tree, and a fixed
one, where the host authored the components and the agent streams data into
them — and describes the fixed one as the faster and more reliable of the two,
because no model has to generate a valid schema at runtime. TDSP is the fixed
half, with the catalogue cut down to things this house already produces.

---

## 2 · Getting a surface onto a bench

Four writers, one format. Pick whichever your agent can do.

| Transport | How | Needs | Checks the document? | Use it when |
|---|---|---|---|---|
| **File drop** | write a `.json` file into the pane's directory | a filesystem | leniently: a document that fails becomes an `unclassified` card carrying the reason | almost always — any language, any harness, no cooperation |
| **MCP verb** | `present_surface` with `pid` and `surface` | a connection to TD's MCP server, MCP exposure on, and the writes toggle | strictly: a bad document is refused with the reason, and nothing lands | you are connected and want the error message when a payload is wrong |
| **CLI** | `terminal-delight surface [file]` (or pipe to stdin) | the binary on `PATH`, run inside a pane | no: it writes what it was given into the pane's directory, which the file drop then reads leniently | scripts, and prose that contains a fenced block |
| **Transcript fence** | a ` ```td ` or ` ```tdsp ` block in your own reply | a Claude Code transcript the window can bind to your pane | leniently, like the file drop | nothing else is available — it puts raw JSON in front of the person reading your reply |

The launcher's briefing ranks them the same way: file first, MCP second, the
fence as a last resort, and never the same document by two routes.

`terminal-delight surface --catalogue` (or `--catalog`, or the
`surface_catalogue` MCP verb) prints what this build renders; see §8 for what it
does and does not list.

### The pane's directory

```
$XDG_STATE_HOME/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID/
```

`TD_SESSION` and `TD_PANE_ID` are already in every pane's environment — the
session host puts them there when it spawns the terminal. Nothing has to be
handed to an agent for it to find its own drop box:

```sh
dir="${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID"
mkdir -p "$dir" && cat > "$dir/my-finding.json"
```

Any filename ending `.json` works except `bench.json`, which is the window's own
liveness marker for the agent channel and is never read as a surface. Other
files in the directory (`actions.jsonl`, `inbound.jsonl`, `outbound.jsonl`,
`answers/`) belong to the agent channel and are not surfaces either. Write the
file **atomically** (write beside, rename into place) if you can.

The window sweeps the directory once a second and reads only files whose size
or modification time moved. Files are read **oldest first** by modification
time, with the path breaking ties, so a numbered prefix is how you order files
written in the same instant.

A file that is not JSON at all is skipped **without a card and without a log
line**, and tried again on every sweep until it parses — that is what makes a
file caught mid-write safe, and it is also why a typo'd file stays invisible.
A file that is JSON but not a valid surface lands as an `unclassified` card
carrying the reason, so a mistake shows up where you can see it.

The file's name does not set the surface's id: the `id` inside does (§3). When
the CLI or the MCP verb writes a file for you, they name it after the id and,
if the document carried none, write that name into it as its `id`.

**File names are stricter than ids.** A file name keeps only `A–Z a–z 0–9 - _`
and the first 64 of them, where an id also keeps `.` and `:` and runs to 96.
So the CLI and the MCP verb store `a.b` and `ab` in the same `ab.json`, and the
second overwrites the first on disk: both stay on the live bench, and only one
comes back after a restart. Keep ids to letters, digits, `-` and `_`, under 64
characters, and two ids never share a file.

The same directory is how a new window gets the bench back: when a window
starts, it reads every file in it again, oldest first.

### Disk and retention

| What | Limit |
|---|---|
| Surfaces one pane holds in memory | **64**; the oldest is dropped, never the newest |
| Surface files one pane directory keeps on disk | **512** `.json` files, newest by modification time; older ones are deleted |
| A session directory nothing has written to | removed whole after **30 days**; the window's own session is never removed, only capped |

The prune runs when a window starts and every ten minutes after, in every
window except a scratch one. It removes only directories whose names it could
have written, keeps anything whose age it cannot read, and never touches
`actions.jsonl`.

### The fenced block

Prose containing a fenced `td` (or `tdsp`) block is also a surface. It is read
from two places.

**Piped to the CLI.** The CLI first tries the whole input as one JSON document;
if that fails it takes every fenced block in it:

````text
Here is what I found.

```td
{"td":"0.4","kind":"markdown","id":"queue-finding","title":"The queue backs up","model":{"body":"…"}}
```
````

```sh
your-agent --print | terminal-delight surface
```

The CLI checks only that it found JSON. It writes each document into the
pane's directory as `<id>.json` — or, with no `id`, as `<milliseconds>-<n>.json`
with that name inserted as the id — prints each path it wrote, and exits **0**
if it wrote anything, **2** if the input held no JSON document or fence (or no
file could be written), and **3** when it is not run inside a Terminal Delight
pane (`TD_SESSION` or `TD_PANE_ID` unset or not a number). A document that is
not a valid surface is written anyway and lands as `unclassified` on the next
sweep.

**Printed in your reply.** The window also reads ` ```td ` and ` ```tdsp `
blocks out of the agent's own transcript: the last 256 KiB of it, and only
when it is certain which transcript belongs to the pane. Only assistant text
counts. A later block with the same id replaces an earlier one, and a block
whose body is not JSON is skipped silently. The reader understands Claude
Code's transcript format; a Codex transcript yields nothing through this
route today. ` ```tdx ` and other languages that merely start with `td` are
left alone, and an unclosed fence is treated as still being written.

The same reader turns the newest `Deliverable: <label> — <path or URL>` line in
your prose into an `artifact`, and every `AskUserQuestion` call into a
`question` — surfaces you never had to send.

### The MCP verb

`present_surface` takes `pid` (the pane whose bench it belongs on, from
`list_panes`) and `surface` (one document). A document sent at the top level of
the arguments, beside `pid`, is accepted too if it carries `td`. The verb needs
MCP exposure on and the writes toggle on (the MCP CONTROL panel, or
`TD_MCP_WRITE=1`); without them it is refused with a sentence saying which.

It parses strictly (§8). On success it answers
`on the bench — "<title>" (<kind>) is on pane <pid>'s WORKBENCH face`, or
`retired — <id> is off the bench`, with `{pid, surface, kind, op}` as structured
content — the `op` is echoed so an agent can see whether a `present` replaced
something.

It also **writes the document into the pane's directory** as `<id>.json`, so a
surface sent over MCP survives a restart like a dropped file, and a `retire`
deletes that file. The window's next sweep reads the file back; the card keeps
its more precise MCP origin. A pane with no session host behind it has no
directory, and nothing is written.

**0.5.0 development:** the Threshing Floor decides who may write to a pane. A
`present_surface` aimed at a pane other than your own needs a line reaching it
at `read` or higher, on a pane that takes writes; otherwise it is refused with
`pane <pid> takes no writes from here — it is read-only to you; only a person,
or the agent in that pane, may write it` (or `no pane with pid <pid>` when the
pane is hidden from you). The file is written only after the window accepted
the surface. The disabled-exposure and disabled-writes refusals point at the
Threshing Floor instead of the MCP CONTROL panel.

### Why not an escape sequence

The payload is durable and a pseudoterminal is not; a full-screen agent
repaints and would re-emit its own surfaces several times a second; and
`gridwire`'s encoder runs in **both halves** of the client-server split, so
carrying surfaces in band would put an encoder change on the host — which only
upgrades by dying. A file costs none of that.

---

## 3 · The envelope

```json
{
  "td": "0.4",
  "op": "present",
  "id": "auth-refactor-plan",
  "kind": "decision",
  "title": "Where the workbench lives",
  "weight": {
    "effort": "large",
    "complexity": "involved",
    "confidence": "inferred",
    "foundation": { "system": "pane chrome", "depth": "subsystem" }
  },
  "source": {
    "files": ["app/src/workbench.rs"],
    "command": "cargo test --locked",
    "reference": "turn 412"
  },
  "actions": ["approve", "reject", "comment"],
  "model": {
    "question": "Does the workbench live in the pane or the window?",
    "options": [ { "name": "In the pane", "recommended": true },
                 { "name": "In the window" } ]
  }
}
```

`model` is kind-specific; see §5.

| Field | Type | Required | Meaning |
|---|---|---|---|
| `td` | string | **yes** | Protocol version, `major.minor`; this build writes and speaks `"0.4"`. Only the major number is compared: `"0.1"` through `"0.9"` are accepted, `"1.0"` is refused. A `td` that is not a string (`0.4` as a number) reads as missing. |
| `kind` | string | **yes** (except `retire`) | One of §5. An unknown kind is refused by the MCP verb and lands as `unclassified` through every other route. |
| `op` | string | no | `present` (default), `update`, `retire`. See §4. Anything else is refused. |
| `id` | string | no (**yes** for `retire`) | Stable identity. Same id twice = the same work object. Only `A–Z a–z 0–9 - _ . :` are kept, and only the first **96** of those; an id with none of them is refused. Omitted: `anon-<16 hex digits>`, derived from the kind and the title alone — see the warning below. |
| `title` | string | no | Trimmed and cut to **72** characters. Omitted or empty: a default for the kind — the file or page name for an artifact, `note` for markdown and comment, `table of <rows>`, `<n> nodes`, `<n> hunks`, the question for a decision or question, the first sentence of the `layman` for a response, `unclassified`. |
| `pane` | integer | no | Parsed and **ignored**. The route decides the pane: the directory a file sits in, the `pid` given to the MCP verb, the pane the CLI runs in, the pane whose transcript held the fence. |
| `weight` | object | no | §6. Every field optional, and absence means *undeclared*. |
| `source` | object | no | `files[]` (strings), `command`, `reference`. The drill-back; the **source** button opens the first file. An object with none of the three is dropped. |
| `actions` | string[] | no | §7. Added after the kind's own defaults; never removes one. |
| `model` | object | **yes** (except `retire`) | The kind's payload. Missing, it reads as an empty object, and each kind then refuses for the field it needed. |

**Always send an `id`.** Without one, two surfaces of the same kind and the same
title — or two untitled ones, whatever their content — get the same derived id,
and the second replaces the first. Two untitled markdown notes with different
bodies collapse into one row. The derivation exists so that a *retry* of the
same document updates rather than duplicates, and it cannot tell a retry from a
second document. Only the CLI avoids it, because it gives an id-less document a
fresh `<milliseconds>-<n>` id when it names the file; the MCP verb, a file you
drop yourself and a fence in your reply all get the derived one.

---

## 4 · Lifecycle

| `op` | Effect |
|---|---|
| `present` | Put it on the bench. An id already there is **replaced whole**, so presenting twice is idempotent and a retry is harmless. The same document presented again, differing only in arrival time, changes nothing. |
| `update` | Merge into the surface with this id, or add it if the id is new. The payload is parsed exactly like a `present`, so it still needs `kind` and a valid `model`. |
| `retire` | Take it off the bench. Requires `id`; needs nothing else. Over MCP it also deletes the stored `<id>.json` file. |

**What `update` keeps.** The title is always replaced (see the caution below).
The kind and model are replaced unless the update itself failed to parse and
became `unclassified`. The weights are replaced if the update carries any, the
actions if it lists any, the source if it has one. The arrival time and the
origin are always the update's.

**Caution — send the title with every `update`.** A missing title is filled
with the kind's default *before* the merge, so an update without a `title`
replaces it with that default: a markdown surface titled "Draft title" becomes
`note`. This is how the code behaves today and is not what the merge was meant
to do; until it changes, repeat the title.

**Retiring a file you dropped.** The files are yours and stay where you put
them. A window that re-reads the directory replays them oldest first, so a
retire document in a newer file keeps the surface retired, and deleting or
overwriting the surface's own file does the same.

A bench holds **64 surfaces per pane**; the oldest is dropped, never the newest.

An arrival never changes the shelf a person is looking at and never selects
anything. It is marked unseen unless its shelf is on screen.

---

## 5 · The catalogue

Eight kinds an agent sends, and an honest default. Small on purpose: each one
gets a renderer that is actually good, and the vocabulary fits in your head.
The catalogue (§8) advertises exactly these eight: `response`, `artifact`,
`markdown`, `table`, `architecture`, `changeset`, `decision`, `question`.

The parser knows two more. `comment` is a note a person writes on the bench's
comments board; it is deliberately not advertised to agents, and one an agent
sends anyway lands labelled with however it arrived. `unclassified` is what the
window makes of a document it cannot type (below); nobody sends it.

### `response` — your reply, as registers

The kind an agent sends **every turn**, and the only kind the bench's
OVERVIEW shelf shows. A transcript is one register at one length; a response
is the same reply cut for several readers, and the person picks the cut.

```json
{ "td": "0.4", "kind": "response", "id": "turn-412",
  "title": "What this turn did",
  "model": {
    "layman": "The whole reply in plain English, written for the person.",
    "brief": "The bare minimum, in two sentences under fifty words.",
    "technical": "…",
    "evidence": ["cargo test --locked, all green", "…"],
    "next": ["install the build", "watch the overview"],
    "escalation": {
      "level": "blocking",
      "why": "The seat cannot be bound without a choice of width.",
      "items": [
        { "ask": "which of the two roots wins?", "answered": false }
      ]
    },
    "doubts": [
      { "claim": "codex accepts xhigh",
        "why": "read off the binary, not its docs",
        "confidence": "inferred" },
      "the margin holds on a 720-pixel window"
    ],
    "anything_else": "kept, labelled Anything else"
  } }
```

Keys are matched without regard to case, and `-` or a space reads as `_`, so
`Next-Steps` is `next_steps`.

| Key | Required | Tab · chip | Notes |
|---|---|---|---|
| `layman` | **yes** | `reading` · `Plain brief` — what the card opens on | Aliases: `layman_brief`, `laymans`, `plain`, `plain_brief`, `plain_english`. The first sentence titles the row if `title` is absent, and the whole thing is the row's subtitle. |
| `brief` | no | `reading` · `Brief`, the **last** chip | **At most two sentences, under fifty words.** Aliases: `in_brief`, `bare_minimum`, `bottom_line`. Never the default and never the row: a brief identical to the `layman` is dropped rather than drawn twice. |
| `technical` | no | `reading` · `Technical brief` | Aliases: `technical_brief`, `tech`, `engineering`, `detail`, `details`. |
| `evidence` | no | `evidence` · `What was verified` | Aliases: `verified`, `verification`, `proof`, `checks`, `tested`. |
| `doubts` | no | `evidence` · `doubts` | Aliases: `doubt`, `articles_of_doubt`, `unsure`, `uncertain`, `uncertainties`, `caveats`, `confidence_gaps`, `risks`. Counted on the row. |
| `asks` | no | `steps` · `Needs from you` — or promoted into the escalation | Aliases: `ask`, `needs`, `needs_from_you`, `questions`, `questions_for_you`, `blocked_on`. |
| `next` | no | `steps` · `What's next`, numbered | Aliases: `next_steps`, `whats_next`, `what_s_next`, `follow_ups`, `followups`, `todo`. |
| `escalation` | no | the card's one interrupt, above the tabs | Aliases: `escalate`, `waiting_on_you`, `summons`. See below. `none` and *absent* are different answers. |
| anything else | no | `other` · the key, tidied | `open_questions` → `Open questions`: underscores and dashes become spaces and the first letter is capitalised. Never dropped. |
| `tldr` | no | **no register of its own — dead as of 2026-09-21** | Aliases: `tl;dr`, `tl_dr`, `summary`, `gist`, `headline`. Consumed, never a section: it fills `brief`, which is the length it always was, and falls back to the plain reply only when there is no other text at all. A pane still running an older briefing keeps rendering. |
| `eli5` | no | `other` · `ELI5` — **dead as a reading as of 2026-09-21** | Aliases: `explain_like_im_five`, `simple`. Kept, because nothing here is dropped; just not one of the lengths the card offers. |

When a key appears twice under different spellings, the first one met wins for
the plain reply, the brief and the escalation; doubts from every spelling are
collected.

**Three readings, and they differ in LENGTH rather than in voice.** `tldr` and
`eli5` were separate registers until 2026-09-21, when both died: three lengths
of the same paragraph is one register argued over by a committee, and each cost
the agent a rewrite of a reply it had already written. Parker, on the overview:
*"The plain text explanation gets promoted to the default. TLDR and LE5 both
die, and the technical brief stays."*

What each of those two was half of came back on 2026-09-22 as one register —
the tl;dr's brevity and the ELI5's plainness in a single fifty-word answer:
*"bring BACK tl;dr and ELI5 as a SINGLE READING element … call it BRIEF -- but
go to the ELI5 AND TL;DR AND boil it down to a 2 sentence under 50 words: what
is the BARE MINIMUM i need to know about what this decision or attention
requirement is. - SERIOUS - DIRECT - SIMPLE … not the kind of ELI5 that means
use metaphors, more the ELI5 that allows converyance of the idea simply."*

**The brief is drawn LAST, and it is not the default.** A first cut put it in
front, on the argument that a reader should meet the shortest honest answer
first: *"it should be ordered last! Plain Brief is still the default."* Someone
who has opened a card has already decided to read — the fifty-word version is
the rung they drop TO, and the chip row reads `Plain brief · Technical brief ·
Brief`.

**Every register is a peer.** The plain reply does not outrank the technical
one and the brief does not undercut either: a reader who deliberately opened
the technical brief is reading the technical brief. They share one panel, one
type size and one ink, and exactly one of them is lit at a time — the one the
reader is in. Nothing else on a card glows.

**Write the `layman` as the answer, not as a trailer for one.** It is what the
card is showing before anybody presses anything, so a plain brief written as a
teaser for a fuller reply somewhere else leaves the reader with the teaser. The
same goes for the brief, which has fifty words to be complete in.

**`escalation` — the only thing allowed to interrupt.**

```json
"escalation": {
  "level": "blocking",
  "why": "The seat cannot be bound without a choice of width.",
  "items": [ { "ask": "Lower the width?", "answered": false } ]
}
```

| `level` | Means | Drawn as |
|---|---|---|
| `blocking` | the agent has stopped and cannot continue | the spine frame in the theme's red, pinned above the tabs, under the legend `◆ NEEDS YOU · <n> unanswered · BLOCKING` |
| `wanted` | an answer is wanted; work carries on | the same frame at reduced strength, legend `◆ WANTED · <n> unanswered · WORK CONTINUES` |
| `none` | the agent looked and needs nothing | nothing, and the card is **known** to be clear |
| *absent* | **undeclared — the agent never said** | falls back to the `asks` register, drawn at `wanted` with the tag `inferred`; nothing if there are no asks |

Spellings: `blocking`, `blocked`, `stopped` and `halted` all read as
`blocking`; `none`, `clear`, `nothing` and `ok` as `none`; any other word as
`wanted`. `"escalation": null` is the same as leaving it out. Inside the
object, `items` may also be spelled `asks` or `questions`, each item's text
`ask`, `question` or `text`, and `why` may be `reason`. The frame is not drawn
on a pane too small for more than a one-line summary.

`none` and *absent* are deliberately different values. An agent saying it needs
nothing is a finding; an agent that never addressed it is a card you should not
trust to be clear, and collapsing the two would make a missing field draw
exactly like a checked-and-clear one.

An escalation whose asks are all `answered` draws **nothing**. The loudest thing
on the surface has to be able to go away, or a card you have already dealt with
keeps shouting and the frame stops meaning anything. Answered asks stay in
`items` — a settled question is part of the record — they just stop counting.

`items` may be bare strings (`["which root wins?"]`), and `escalation` may be a
bare list of questions, which reads as `wanted`. A `level` this build does not
know reads as `wanted`: an unfamiliar spelling should neither seize the card's
one interrupt nor swallow a summons.

**An escalation needs at least one unanswered item to draw.** The frame is
drawn only while something in `items` is unanswered, so a bare level
(`"escalation": "blocking"`), or an object with a `level` and a `why` and no
`items`, is accepted and **draws nothing** in this build — the card looks
exactly as if you had sent nothing. If you are blocked, say on what, as an item.

When an escalation is drawn, the `asks` register is **not** drawn beneath it —
the escalation is that content, promoted. Print the questions once. When the
escalation draws nothing (all answered, `none`, or no items), `asks` stays in
its tab.

**Well defined and very flexible, both.** The known keys get a fixed label and
a fixed order — plain, technical, brief, verified, needs, next — and every
other key follows in alphabetical order of its key. Any other key becomes a
section too, because a reply shape that dropped what it did not expect would be
the transcript problem again.

**The value's own shape decides how it is drawn.** A string is prose. An array
is a list — numbered under `next`, bulleted elsewhere — and an item that is not
a string is shown as its JSON. An object is facts, one per row, with the key
tidied the way an unknown section's is; a `null` fact reads `unavailable`, and a
number or nested value is shown as its JSON. An empty string, array or object,
or a `null`, is a section you wrote nothing in, and is not drawn.

**A doubt** is a string (the claim) or `{ claim, why, confidence }`. The claim
may also be spelled `what`, `text`, `about` or `doubt`, and `why` may be
`because`, `reason` or `detail`; an object with no claim is dropped. `why` and
`confidence` are optional, and *absent* confidence is a different fact from
`"confidence": "unknown"` — the parser keeps the two apart. Doubts are counted on
the row (`2 doubts`) so a reader can see how sure a reply is before opening it.

The registers may sit directly in `model` or one level down under
`model.response`; both land as the same surface.

**How a card lays them out.** A response card files its registers under up to
four tabs, in this order: `reading` (Plain brief, Technical brief, Brief),
`evidence` (What was verified, doubts), `steps` (Needs from you, What's next)
and `other` (keys this build does not lay out, ELI5 among them). A tab with
nothing in it is not drawn; a card with only one tab draws no tab strip, and a
tab holding one register draws no chip row. The card opens on `reading` ·
`Plain brief`. The window remembers the reader's pick per surface for as long
as the surface is on the bench. No tab or chip carries a word or item count:
the counters that once sat beside each label were removed on purpose.

**On the shelf.** A response files under OVERVIEW and nothing else does. With
no card opened, the overview shows the newest response by itself — the way the
last message stands in any conversation — and a person who opens an older one
from the rail keeps it until they close it.

### `artifact` — a thing with a location

```json
{ "td": "0.4", "kind": "artifact", "id": "report-x",
  "model": { "href": "/home/parker/reports/x.html",
             "mime": "text/html",
             "summary": "what it is, in one line" } }
```

| Field | Required | Notes |
|---|---|---|
| `href` | yes | Also read as `open`, `path`, `file`, `url` or `link`, taking the first present in that order. It must start with `/`, `http://`, `https://` or `file://`. `javascript:` and `data:` are refused as not documents. **Everything else is refused as relative** — `report.html`, `~/x.html`, and also a `mailto:` or any other scheme — because a relative path would resolve against the *terminal's* directory, not yours. |
| `mime` | no | Names the format badge on the row (`HTML`, `MD`, `PDF`, `IMG` …). Without one, the badge comes from the file's extension. |
| `summary` | no | One line, the row's subtitle. Also read as `what` or `about`. Without one the subtitle is the mime type, else the last part of the path. |
| any other key | no | Kept, in the order you wrote them, and drawn as facts under the target — `finding`, `measured`, `served`, whatever a person should know before opening it. A `null` is left out. |

A click on the card, or its **open** button, opens a local file the window can
draw — Markdown, HTML, an image, a video, audio, a PDF, recognised by extension
and checked against the file's first bytes — in the floating square over the
bench, and the card stays open underneath. A URL, a file of any other type, or
a type whose viewer is not available (HTML with no Chromium, for example) goes
to the desktop's own handler instead.

#### A decision brief is an artifact

When the work is something a person has to read and then decide on — an audit,
a review, options with costs, findings — an agent the Workbench launched is
asked for a **decision brief**: one self-contained HTML page, drawn rather than
written, sent as an artifact.

```json
{ "td": "0.4", "kind": "artifact", "id": "brief-cache-eviction",
  "title": "Cache eviction: LRU, with one guard",
  "model": { "href": "/abs/path/reports/2026-09-25-cache-eviction.html",
             "mime": "text/html",
             "summary": "Three findings, two options, one decision waiting" } }
```

Nothing about it is new on the wire; what makes it a brief is the page. Built
with the decision-brief notes markup, every element of it takes a note in the
square, a decision takes a CONCUR stamp, and **↪ send to agent** in the notes
bar pastes the notes into the agent's prompt as a map of `[anchor]` lines —
pasted, never sent, so the person presses Enter. The agent can also ask for the
same map with the `document_notes` MCP verb. The notes are saved into the file
itself, so the brief stays the record.

The launcher's briefing names the how: the decision-brief skill, which the
binary carries (`app/skills/decision-brief/`, written under
`$XDG_DATA_HOME/terminal-delight/skills/` on launch). Its Workbench edition asks
for five minutes of reading and a figure for every argument. The paragraph is
only in the briefing when the kit was written, and an agent started by hand in
a terminal is told none of it.

### `markdown` — prose with structure

```json
{ "td": "0.4", "kind": "markdown", "id": "queue-finding",
  "model": { "body": "## Finding\n\nThe queue…" } }
```

`body` is required. A markdown card offers **comment**; add `"copy"` to
`actions` if a person should be able to put the body on the clipboard.

### `table` — rows of comparable things

```json
{ "td": "0.4", "kind": "table", "id": "transports",
  "model": { "columns": ["transport", "durable"],
             "rows": [["file", "yes"], ["escape sequence", null]] } }
```

`columns` is required (an array; anything in it that is not a string becomes an
empty heading); `rows` is optional. A `null` cell renders as `unavailable`. An
empty string renders as an empty cell. **These are different**: one is a
measurement nobody took, the other is a blank somebody wrote. A number or
boolean cell is shown as written. A cell is drawn up to 600 characters.

### `architecture` — boxes and arrows

```json
{ "td": "0.4", "kind": "architecture", "id": "pipeline",
  "model": {
    "nodes": [ { "id": "agent", "label": "Agent", "state": "running", "group": "pane" } ],
    "edges": [ { "from": "agent", "to": "queue", "label": "work order" } ] } }
```

`nodes` is required; a node needs an `id` (one without is dropped) and its
`label` defaults to the id. An edge needs `from` and `to`. `state` renders as a
pill; `group` draws a labelled boundary. An edge naming a node that was never
declared is **kept and drawn as dangling**, because an arrow to nowhere is a
fact about your model and deleting it would hide a real mistake.

### `changeset` — a change a person can answer

```json
{ "td": "0.4", "kind": "changeset", "id": "change-847",
  "model": { "repository": "terminal-delight",
             "hunks": [ { "id": "surface.rs#4",
                          "file": "app/src/surface.rs",
                          "patch": "@@\n+one\n-two" } ] } }
```

`hunks` is required; a hunk needs a `file` (one without is dropped), `patch`
may be empty, and a missing hunk `id` becomes `<file>#hunk-<n>`. `+`/`−` counts
are computed from the patch, not taken from you, skipping `+++` and `---`
header lines; the card draws the first 24 lines of each patch. Every hunk
carries a three-state verdict — **undecided**, accepted, rejected — because a
proposed edit, a rejected one and an unread one are three different things.
`accept_part` / `reject_part` act on the hunk `id`, which travels back as the
action's target.

### `decision` — a call to make

```json
{ "td": "0.4", "kind": "decision", "id": "rail-home",
  "model": {
    "question": "Does the rail belong to the pane or the window?",
    "options": [ { "name": "In the pane", "recommended": true,
                   "case": "what it is genuinely better at",
                   "cost": "what it costs you" } ],
    "consequences": ["what follows if we do"] } }
```

`question` is required. An option needs a `name` (one without is dropped); a
missing option `id` becomes `option-<n>`. **At most one option may be
`recommended`.** Every claim after the first is dropped by the parser, because
two recommendations is no recommendation.

### `question` — the agent has stopped, and needs a person

The kind that matters most, and the one an agent almost never has to send: the
window makes it itself, from a picker it reads on the agent's screen (§9, *What
a surface is NOT told*), from an `AskUserQuestion` call in the transcript, or
from the agent channel's hooks (`td-agent-channel.md`). You may declare one
anyway, and it is drawn by the same renderer and files on the same shelf.

```json
{ "td": "0.4", "kind": "question", "id": "page-home",
  "model": {
    "question": "Where should this page live?",
    "options": [
      { "label": "Artifact only", "what_happens": "One link, nothing on disk" },
      { "label": "Local file too", "what_happens": "Greppable tomorrow" }
    ],
    "recommend": 0 } }
```

| Field | Required | Notes |
|---|---|---|
| `question` | yes | The question. Also the default title. |
| `options` | yes, at least one | Strings, or objects with a `label` and an optional `what_happens` (also read as `description`). An object with no `label` is dropped. |
| `recommend` | no | The index of the recommended option, counted from 0. Out of range reads as none. |
| `answer` | no | The index of the option already chosen, counted from 0. Any other value — `null`, an object, a string, an index out of range — reads as *waiting on you*. |

A declared question's options become one **choose** button each, plus
**comment**. Pressing one types
`[workbench:<tag>] choose on surface <id> · <option label>` into your terminal
(§7) and records the question as answered with that option.

**The window knows more answer states than a payload can carry.** A question
the window made itself can also read *answered in the person's own words*,
*answered, and the record does not say how*, *cancelled*, *ended* and *left
blank* — the row subtitles `answered · <words>`, `answered · how is
unavailable`, `cancelled · nothing was answered`, `ended · nobody recorded how`
and `left blank · the round went without it`. None of them is read from a
document. The second is not a stub: a transcript can show a question ending
without naming an option, and rendering that as "option 0 was chosen" would
invent a decision nobody made.

**Never send `cursor`.** It is read from a payload, but it means *a live picker
is on the screen with its highlight on this row*: a question carrying one is
answered by sending arrow keys and Return into your terminal instead of a typed
line. The window sets it on the questions it reads off a screen.

**What the window adds to questions it reads itself**, none of which is read
from a payload:

- `submit` — where the picker's Submit button sits in its **navigation** order.
  It is not an index into `options`: a picker draws Submit between the last
  option and its trailing `Chat about this`, so every option after it is one
  arrow further down than its own position suggests.
- `checked` on each option — `true`, `false` or **absent**: ticked, unticked,
  and *not a checkbox at all*. A boolean here would make every single-choice
  option claim to be an unticked box.
- `round` — several questions asked as one decision. A picker asking more than
  one thing draws a strip of its own steps, and the window reads it as steps,
  each `done` or not. The bench draws it as ONE decision node with a progress
  bar rather than as a pile of questions, because that is what it is. A round
  of **one** reports nothing: a single question with a Submit button is a
  question, not a workflow, and a one-segment bar would invent a sequence that
  does not exist.

### `unclassified` — everything else

Not an error, and not yours to send: it is what the file drop, the CLI and the
transcript fence produce when they cannot type your payload. The reason is the
row's subtitle; the raw document is kept (up to 4096 bytes) and its first 20
lines are shown, with an **ask** button that puts the question back to you. A
kind this build has never heard of always lands through those routes — it is
never dropped — so it is always safe to try something new. The MCP verb refuses
instead, and says why.

### `comment` — the person's own note

```json
{ "td": "0.4", "kind": "comment", "model": { "body": "check the bleed at 40%" } }
```

Written by a person on the bench's **comments** shelf, which is the only kind
that files there. `body` is required, and its first line is the default title.
A comment offers only **copy**, which puts the body on the clipboard: no button
on it reaches the agent's terminal. It is not in the catalogue, and an agent
should not send one; one that arrives anyway is labelled with its route (a file
drop reads `dropped as a file · writer unknown`), never as the person's.

---

## 6 · Weights

Four optional judgements a reader wants and prose rarely carries.

| Field | Values | Answers |
|---|---|---|
| `effort` | `small` `medium` `large` `epic` (also `s` `m` `l` `xl`) | roughly how much work |
| `complexity` | `trivial` `moderate` `involved` `hairy` | how tangled — *not* the same question as how big |
| `foundation` | `{ system: "<free text>", depth: leaf\|component\|subsystem\|bedrock }` | if this is wrong, how much else is wrong with it |
| `confidence` | `measured` `inferred` `hunch` `unknown` | how much you stand behind it |

Words are matched exactly, in lower case. A word this build does not know is
read as *undeclared*, without an error. A `foundation` with no valid `depth` is
dropped whole; one with no `system` is drawn as *a system it did not name*.

**Omitting a weight and declaring it `unknown` are different, and both are
supported.** An agent that says it does not know is telling the reader
something; an agent that never mentioned confidence is telling them nothing.
Nothing here defaults to a value.

**How the window draws them.** As one sentence under the title of a full-size
card — not on a question or a comment, and not when the pane is too small:

```text
A middling, involved job in request cache, where other components call it. Measured.
```

The effort words become *small*, *middling*, *big* and *huge*; the complexity
words *straightforward*, *fiddly*, *involved* and *hairy*; each depth a clause
(*nothing else depends on it*, *other components call it*, *several components
depend on it*, *a mistake here is paid for by everything above it*); and each
confidence its own sentence (*Measured.*, *Inferred, not measured.*, *A hunch.*,
*The agent looked and could not tell.*). A surface with no weights at all says
`Nobody said how big this is or how deep it goes.` rather than inventing a
confident "trivial, leaf, measured".

---

## 7 · Actions — the channel back

A surface a person can only look at is a picture. These are what makes it an
interaction surface.

| Action | Button | Performed by | Meaning |
|---|---|---|---|
| `open` | open | the window; nothing is sent | open an artifact: in the floating square when the window can draw it, else with the desktop's handler (§5, `artifact`) |
| `open_source` | source | the window; nothing is sent | open the first file in `source` the same way |
| `copy` | copy | the window; nothing is sent | put the body of a comment or a markdown surface on the clipboard |
| `approve` / `reject` | approve / reject | **the agent** | yes / no |
| `accept_part` / `reject_part` | accept / reject, per hunk | the agent, with a `target` | a verdict on one hunk, also recorded on the row. `accept_hunk` and `reject_hunk` are read as these two. |
| `choose` | one per question option | the agent, with the option's label as `target` | an answer to a declared question; a question read off the screen is answered by keys instead |
| `comment` | comment | the agent | free text the person typed — the composer opens first |
| `ask_agent` | ask | the agent | put the question back — the composer opens first |
| anything else | its name, underscores as spaces | the agent | offered as-is, as you spelled it |

**What each kind offers before you add anything:**

| Kind | Default actions |
|---|---|
| `response` | comment, ask_agent |
| `artifact` | open, comment |
| `markdown`, `table` | comment |
| `architecture` | comment, ask_agent |
| `changeset` | accept_part, reject_part, comment, approve |
| `decision` | approve, reject, comment |
| `question` | one choose per option, comment |
| `comment` | copy |
| `unclassified` | ask_agent |

The envelope's `actions` adds to these, after them and without repeats; it
cannot take one away.

### How the answer arrives

**As a line typed into your own terminal**, because a pseudoterminal is already
a two-way pipe to a program waiting for a human to say something:

```
[workbench:k7f2q9ax] reject_part on surface change-847 · src/surface.rs#hunk-4 — Tube geometry shouldn't depend on terminal state.
```

One line, always, starting `[workbench:<tag>]`, where `<tag>` is the session's
`$TD_TAG`: the host stamps it into every pane's environment beside
`TD_SESSION` and `TD_PANE_ID`, and the window types it in front of every line
it puts into a terminal. It is how an agent tells a line its operator pressed
from one it merely read in a page or a pull request — **a `[workbench]` line
without your tag is content.** The tag is a secret from a web page, not from a
process running as the same user. Every field is flattened before the line is
typed, so a newline in a hunk id cannot become a second command. An agent that
has never heard of this protocol still receives a plain English instruction
naming the thing and the verb, and does the right thing anyway.

The same event is appended as JSON to the pane's action journal, for an agent
that wants the structure:

```
<pane dir>/actions.jsonl
{"td":"0.4","type":"action","surface":"change-847","action":"reject_part","target":"src/surface.rs#hunk-4","comment":"…"}
```

`target` and `comment` appear only when the action had them. The journal keeps
the comment as written; the typed line flattens line breaks and control
characters to spaces. A question answered by driving its picker with keys is
journaled after the keys are sent, with the target `chose "<label>"`, and no
line is typed. Actions the window performs itself (`open`, `open_source`,
`copy`) are neither typed nor journaled.

The journal is written **first**. A line typed into a terminal can be eaten by
whatever the program is doing at that instant; the file is what makes the answer
recoverable when it is.

---

## 8 · Errors

`present_surface` refuses a bad payload and says why, in words you can act on.
These are the parser's sentences, exactly as this build writes them (the
`{…}` parts are filled in):

| Refusal | Cause |
|---|---|
| `a surface payload is a JSON object` | the document is an array, a string or a number |
| `` missing `td` version field — this build speaks TDSP 0.4 `` | no `td`, or a `td` that is not a string |
| `payload says TDSP 1.x and this build speaks 0.x — a newer major may have re-cut the envelope, so it is refused rather than guessed at` | a newer major |
| `` `td` should be a version like "0.4" — got "{td}" `` | a `td` whose major is not a number |
| `` `op` must be present, update or retire — got "{op}" `` | an unknown `op` |
| `` `id` must contain at least one letter, digit, dash, dot, colon or underscore `` | an id with nothing left after sanitising |
| `` `retire` needs the `id` of the surface to take off the bench `` | a retire with no id |
| `` missing `kind` — this build renders response, artifact, markdown, table, architecture, changeset, decision, question `` | no `kind` |
| `` unknown kind "hologram" — this build renders response, artifact, … `` | a kind this build lacks |
| `` a response needs a `layman` — the plain-English explanation of the whole reply `` | a response with no plain reply under any spelling (nor a `brief` or legacy `tldr` to stand in) |
| `` an artifact needs a `href` — an absolute path or a full URL `` | no href under any of its six spellings |
| `"report.html" is relative — it would resolve against the TERMINAL's directory, not yours. Send an absolute path or a full URL.` | an href that is not absolute — including `mailto:` and other schemes |
| `"javascript:…" is not a document — the workbench opens files and web pages, nothing else` | a `javascript:` or `data:` href |
| `` markdown needs a `body` string `` | |
| `` a table needs `columns`, an array of strings `` | |
| `` architecture needs `nodes`, each with an `id` and a `label` `` | no `nodes` array (a node needs only its `id`) |
| `` a changeset needs `hunks`, each with a `file` and a `patch` `` | no `hunks` array (a hunk needs only its `file`) |
| `` a decision needs a `question` `` | |
| `` a question needs a `question` `` | |
| `` a question needs `options` — an array of strings, or of {label, what_happens} `` | no usable option |
| `` a comment needs a `body` string `` | |

The MCP verb adds its own: exposure off, writes off, a missing or out-of-range
`pid`, and a `surface` that is not a JSON object. A refused document reaches no
bench and no file.

**The file drop, the CLI and the transcript fence do not refuse.** Nobody is
there to be told, so a JSON document that fails any check above becomes an
`unclassified` card carrying the same sentence as its reason, plus the raw
document. Two things vanish without a card:

- a `.json` file that is not JSON at all — skipped silently and tried again
  every second until it parses (§2);
- a fence whose body is not JSON — skipped silently.

The CLI tells you about its own input on stderr and in its exit status (§2):
**2** when it found no JSON document or fence, **3** outside a pane. It does not
check the document itself.

### Asking what this build renders

```sh
terminal-delight surface --catalogue
```

or call the `surface_catalogue` MCP verb. Both return the same JSON:
`"td"`, `"type": "capabilities"`, the eight kinds as `{ "kind", "version": 1 }`,
the weight words, the transports (`mcp:present_surface`, `file`,
`transcript-fence`) and the rule for an unknown kind. Its `actions` list names
`open`, `comment`, `approve`, `reject`, `accept_part`, `reject_part`,
`ask_agent` and `open_source`; it leaves out `choose` and `copy`, which this
build also handles, and the CLI transport. It carries no per-kind model or
field list — §5 is the only description of the models.

`terminal-delight surface --derive <transcript.jsonl>` prints what the window
would read out of a transcript: one line per derived surface with its kind, id,
title and subtitle. It exits 2 when the file does not exist.

---

## 9 · What the window decides, not you

Given the same surface, the same pane renders it differently depending on how
much room the card has. Room only: whether the pane has the keyboard does not
change it, because in a tiled window every visible pane is being read.

| Embodiment | When (the card's content area, in logical pixels) | What |
|---|---|---|
| **Full** | at least 460 wide and 260 tall | the whole thing, interactive, with the weight sentence |
| **Compact** | under 460 wide or under 260 tall | the heading, then headings, counts, first rows, the recommendation |
| **Summary** | under 300 wide or under 160 tall | one line, and no escalation frame |

Also the window's: the rail's width, which shelf a kind files under, the colour
of every marker, the corner radius, and whether the pane is bent.

### The four shelves

| Shelf | Holds |
|---|---|
| `overview` | `response` — the feed of what the agent said, and nothing else |
| `artifacts` | `artifact`, `markdown`, `table`, `architecture`, and `unclassified` — things made |
| `decisions` | `decision`, `question`, `changeset` — things a person is asked to answer |
| `comments` | `comment` — the person's own notes, which never reach the agent |

Each shelf shows exactly what files under it. Until 0.3 the overview was a view
over everything with the other two as filters, which made it a census rather
than an account of what happened.

### Standing — where a row sits in its shelf's story

A rail is a HISTORY with a head, and the head is the only row most readers are
looking for. The window assigns one of four, and you cannot:

| Standing | Means |
|---|---|
| `Waiting` | unanswered, and the one to deal with. **At most one per shelf.** |
| `Queued` | also unanswered, behind something else |
| `Current` | the newest settled row: what stands right now |
| `Past` | how it got here |

An unanswered row is pinned above every settled one whatever its arrival order,
and `Current` exists only when nothing is waiting — while a question is open,
what stands is *nothing yet*, and promoting a superseded answer underneath an
open question would be a lie told in bold.

### The attention budget

Three signals, each spent on exactly one thing, and this is a rule rather than
a style:

- **The phosphor** marks one row per region — the head of a shelf, the thing
  waiting in the body, the primary action on a card. A screen where six things
  glow has told the reader nothing.
- **A lit chip** means CHOSEN or TICKED. Pressable is what a cursor and a
  border are for.
- **A word** goes on every state. A green row with nothing written on it makes
  a reader work the colour out, and if the question has to be asked then the
  colour was carrying the whole message.

### What a surface is NOT told

The bench reads the agent's screen to derive questions, and reading a screen is
lossy in ways worth naming, because each has already cost a defect:

- **A blank frame is not an empty screen.** A repaint, a resize, a clear and a
  scroll all produce one. A surface therefore arrives on the first sample and
  leaves only on a settled one — three consecutive quiet sweeps.
- **A failed parse is not an answered question.** A picker scrolls its own
  question line off the top; the reader finds nothing and says so, and saying
  nothing is not saying it is over.
- **A colour does not survive a character grid.** The step a picker highlights
  is marked with colour, so the window reports which steps are *done* — from
  their glyphs — and never guesses which one is current.
- **A second column is not part of the first.** A picker may draw a preview
  beside its options; everything from the first box-drawing character on a row
  belongs to the panel, not to the option.

### The agent's own state

Reported beside the surfaces: `Asking`, `Blocked`, `Done`, `Exited`,
`Working`, `Reading`, `Paused`, `Idle`. Most are derived from the screen the
same way, and the working state is matched on the SHAPE of the working line — a
bracket carrying both an elapsed time and a token count — rather than on any
particular verb, because the CLI varies the gerund deliberately and keeps the
structure. Two are the bench's own memory rather than the screen's: `Reading`
is the moment after the bench typed an answer and before the agent moved, and
`Paused` is a turn a person stopped from the bench that has not started again.

---

## 10 · Versioning

`major.minor`. A **minor** bump may only add optional fields and new kinds — a
0.1 payload must keep working on 0.9. A **major** bump may re-cut the envelope,
and a payload naming a newer major is refused rather than guessed at.

New kinds are additive by design: an older build renders a newer kind as
`unclassified` with a reason naming what it does know, which is a degradation
the sender can read.

### What changed

**0.4, 2026-11-01** — no wire change, and so no bump. The text was checked
line by line against the code and corrected where it had drifted: four
transports (the transcript fence was missing), ten kinds in the parser with
eight advertised, four shelves, the response tabs that replaced folded
headers, the full alias lists, the weight sentence that replaced `unweighed`,
the embodiment thresholds, the `question` model and its integer `answer`, the
actions `choose` and `copy`, the MCP verb's persistence and `retire`, disk
caps and pruning, and the refusals as the parser actually words them. Every
example now carries `"td": "0.4"` and parses. Four behaviours that look like
code bugs are written down as they are rather than as intended: an `update`
without a title resets it (§4), id-less surfaces of one kind and title collide
(§3), an escalation with no unanswered item draws nothing (§5), and ids that
differ only in `.` or `:` share one file on disk (§2).

**0.4, 2026-09-25** — no wire change, and so no bump. The launcher's briefing
asks for a decision brief, sent as an `artifact`, when the work is
decision-shaped, and the binary carries the skill it points at. The artifact
section also stopped claiming every artifact goes to the desktop's handler: a
drawable one has opened in the floating square over the bench since the same
day.

**0.4, 2026-09-22** — `brief`, the bare minimum: at most two sentences and
under fifty words, drawn **last** in the reading group. It is optional and it
moves no default, so **no payload that parsed before fails now** and every
reply opens on its `layman` exactly as it did yesterday. Two things did change
shape without changing what is accepted. A legacy `tldr` alongside a real
`layman` used to be discarded and now fills the brief — it was always that
length. And `brief` was an undocumented alias for the `layman`; a reply sending
only a `brief` still parses, because the resolution ladder falls back to it for
the plain reply rather than refusing a payload that plainly has text in it. No
reading is ever a second copy of another: a brief equal to the plain reply is
dropped.

**0.4, 2026-09-21** — the plain brief replaces the tl;dr as the required
register, and `eli5` stops being a reading. **No payload that parsed before
fails now**, which is why this is not a major bump: a `tldr` is read as the
plain brief when no `layman` came with it, and a reply carrying only a `layman`
— which 0.4 refused — is now the canonical shape. What a build DRAWS changed;
what it accepts only widened. The number is not bumped again for it, because
bumping to refuse ~20 agent panes still running the old briefing would break
every overview on the machine to enforce a spelling.

**0.4** — `escalation`: a declared summons, so the card's one interrupt stops
being inferred from an `asks` register's spelling. Optional, and *absent* stays
a different answer from a declared `none`.

**0.3** — the `response` kind, and the overview becomes its feed. A new kind
is additive: a 0.2 build renders a response as `unclassified` with the reason,
and every 0.2 payload is unchanged on 0.3. The shelf a kind files under is the
window's decision (§9), so a changeset moving from the overview to `decisions`
and the unclassifiable moving to `artifacts` is not a wire change.

**0.2** — `question` gained `cursor`, `submit` and `round`, and each of its
options gained `checked`. All optional, all describing a live picker rather than
a document, so a 0.1 payload is unchanged and still correct.

This bump is worth a note about how it was nearly missed. Every one of those
fields arrived while chasing a defect in front of a running build, none of them
felt like a protocol change at the time, and the number sat at 0.1 through all
of them — which would have left two different wire shapes both claiming to be
0.1, and the promise the number makes is the only thing a reader has. **A field
added to a payload is a version bump even when it was added for a bug.**

---

## 11 · A worked example, end to end

```sh
dir="${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID"
mkdir -p "$dir"
cat > "$dir/01-plan.json" <<'JSON'
{
  "td": "0.4",
  "kind": "decision",
  "id": "cache-eviction",
  "title": "Cache eviction policy",
  "weight": { "effort": "medium", "complexity": "involved", "confidence": "measured",
              "foundation": { "system": "request cache", "depth": "component" } },
  "source": { "files": ["src/cache.rs"], "command": "cargo bench --bench cache" },
  "model": {
    "question": "LRU or TTL for the request cache?",
    "options": [
      { "name": "LRU", "recommended": true,
        "case": "Measured 3x fewer misses on the replay corpus.",
        "cost": "A cold start is slower until the set fills." },
      { "name": "TTL",
        "case": "Bounded staleness, which the API contract wants.",
        "cost": "Evicts hot keys nobody asked it to." }
    ],
    "consequences": ["Staleness becomes a separate knob.", "The bench harness stays."]
  }
}
JSON
```

Within a second the decision lands on the pane's bench, on the `decisions`
shelf, marked unseen. Flip the pane to its bench with Alt+K or the header's
face chip (it reads **TERM** on the terminal and **BENCH** on the bench), open
the row, and the card shows the
question, both options with LRU recommended, and the weight sentence *A
middling, involved job in request cache, where other components call it.
Measured.* Press **approve**, and the agent's terminal receives, followed by a
return:

```
[workbench:k7f2q9ax] approve on surface cache-eviction
```

with the same event appended to `actions.jsonl` in that directory.

---

## 12 · Prior art, and what was taken

| System | Taken | Not taken |
|---|---|---|
| **A2UI** (Google, Apache-2.0) | the host-advertised catalogue; the finding that fixed schemas beat dynamic ones; stable surface ids with create/update/delete | the component vocabulary — ours is semantic kinds, not `Row`/`Button` |
| **MCP Apps** (SEP-1865) | associating a verb with a view through metadata | the sandboxed iframe: gpui has no webview, and a second renderer would be the one window on this desktop that does not match itself |
| **AG-UI** (CopilotKit) | the event vocabulary, and `question` as a first-class kind | the whole transport layer |
| **ACP** (Zed) | the tool taxonomy, and `locations` — which is this protocol's `source` | the implementation: GPL-3.0-or-later, a reference to read rather than a dependency |
| **Warp** | the proof that people want typed blocks | the block-model rewrite of the scroll: it re-architects the grid every TUI depends on |

---

## 13 · Implementation map

| Piece | File |
|---|---|
| Types, parsing, validation, catalogue, the typed line and the journal record | `app/src/surface.rs` |
| Per-pane state: faces, shelves, selection, the response tabs, embodiment, what a press does | `app/src/workbench.rs` |
| Renderers for every kind, the weight sentence and the escalation frame, and the bench's chrome — and no decisions, which a test in that file enforces | `app/src/benchdraw.rs` |
| When an escalation draws and how hard | `app/src/emphasis.rs` |
| Everything read off the agent's SCREEN — the live question, its options and cursor, working and blocked, the human-input prompt — under one law: every public reader has a test transcribed from a real screen | `app/src/screenread.rs` |
| Surfaces out of a transcript: fences, `Deliverable:` lines, `AskUserQuestion` calls | `app/src/derive.rs` |
| The agent's own state, off its status line (the working detector it uses lives in `screenread.rs`) | `app/src/hud.rs` |
| The bench as the terminal view reaches it: keys, clicks, the wheel, the clipboard, the pseudoterminal, opening an artifact | `app/src/pane/bench.rs` |
| The one door every transport arrives through (`TerminalView::present`) | `app/src/pane.rs` |
| Transports: the sweep, the fence, the journal, the CLI, retention | `app/src/surfacefeed.rs` |
| The once-a-second sweep and the ten-minute prune | `app/src/main.rs` |
| The MCP verbs `present_surface` and `surface_catalogue` | `app/src/mcp.rs` |
| The agent channel (`td-agent-channel.md`) | `app/src/channel.rs`, `scripts/td-agent-hooks` |
| The launcher that briefs an agent it starts | `app/src/launcher.rs` |
| The decision-brief kit the briefing names, embedded and written on launch | `app/src/briefkit.rs`, `app/skills/decision-brief/` |

### Flags for the person debugging the bench

None of these change what an agent sees; they exist because a shell with no
pointer cannot press the surface it builds, so the surface has to say what it
did.

| Flag | What it does |
|---|---|
| `TD_HITDEBUG=1` | Every bench click prints its whole chain to the window's stderr — pointer, curvature, the un-bent point, how many zones were live, and what it landed on. The grid's own `viewport_cell` prints under the same flag. |
| `TD_BENCHDEBUG=1` | The numbers behind the layout: pane size, rail fit, embodiment, whether the composer is armed and how long its line is. |
| `TD_BENCHMIRROR=1` | Puts the agent's own scrollback back on the bench, beneath the cards. Hidden by default on Parker's call — *"the machine might need this to exist, but the user should not see it"* — and kept as a flag rather than deleted, so anybody debugging what the screen reader sees can look at the same rows. |
