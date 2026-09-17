# TDSP — the Terminal Delight Surface Protocol

**Version 0.1 · 2026-09-17 · implemented in `app/src/surface.rs`**

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

Three writers, one format. Pick whichever your agent can do.

| Transport | How | Needs | Use it when |
|---|---|---|---|
| **MCP verb** | `present_surface` with `pid` and `surface` | a connection to TD's MCP server, and the writes toggle | you are connected and want the error message when a payload is wrong |
| **File drop** | write a `.json` file into the pane's directory | a filesystem | anything else — any language, any harness, no cooperation |
| **CLI** | `terminal-delight surface [file]` (or pipe to stdin) | the binary on `PATH` | scripts, and prose that contains a fenced block |

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

Any filename ending `.json` works. Write the file **atomically** (write beside,
rename into place) if you can; a file caught mid-write is skipped and retried on
the next sweep rather than lost, but a rename makes that impossible.

The window sweeps the directory once a second. Files are read **oldest first**,
with the filename breaking ties, so a numbered prefix is how you control what a
person sees first.

### The fenced block

Prose containing a fenced `td` block is also a surface. Pipe it to the CLI:

````text
Here is what I found.

```td
{"td":"0.1","kind":"finding","title":"…","model":{…}}
```
````

```sh
your-agent --print | terminal-delight surface
```

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
  "td": "0.1",
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
  "model": { "…": "kind-specific, see §5" }
}
```

| Field | Type | Required | Meaning |
|---|---|---|---|
| `td` | string | **yes** | Protocol version, `major.minor`. A newer **major** is refused; a newer minor is accepted (a minor bump may only add optional fields). |
| `kind` | string | **yes** (except `retire`) | One of §5. An unknown kind is **kept** and rendered `unclassified`. |
| `op` | string | no | `present` (default), `update`, `retire`. See §4. |
| `id` | string | no | Stable identity. Same id twice = the same work object. Omitted: derived from kind + title, so a retry updates rather than duplicates. Sanitised to `[A-Za-z0-9-_.:]`. |
| `title` | string | no | Up to 72 characters. Omitted: derived from the model (a filename, a question, a count). |
| `pane` | integer | no | The pid whose bench this belongs on. Omitted: the pane the payload arrived through. |
| `weight` | object | no | §6. Every field optional, and absence means *undeclared*. |
| `source` | object | no | `files[]`, `command`, `reference`. The drill-back. |
| `actions` | string[] | no | §7. Merged with the kind's own defaults; never replaces them. |
| `model` | object | **yes** (except `retire`) | The kind's payload. |

---

## 4 · Lifecycle

| `op` | Effect |
|---|---|
| `present` | Put it on the bench. An id already there is **replaced whole**, so presenting twice is idempotent and a retry is harmless. |
| `update` | Merge into the surface with this id. Fields the payload does not mention are left alone — a progress update need not restate a document to change its title. |
| `retire` | Take it off the bench. Requires `id`. The file on disk is the agent's own business. |

A bench holds **64 surfaces per pane**; the oldest is dropped, never the newest.

---

## 5 · The catalogue

Six kinds and an honest default. Small on purpose: each one gets a renderer
that is actually good, and the vocabulary fits in your head.

### `artifact` — a thing with a location

```json
{ "kind": "artifact",
  "model": { "href": "/home/parker/reports/x.html",
             "mime": "text/html",
             "summary": "what it is, in one line" } }
```

| Field | Required | Notes |
|---|---|---|
| `href` | yes | Absolute path or full URL. **Relative paths are refused** — they would resolve against the *terminal's* directory, not yours. `javascript:` and `data:` are refused as not-documents. |
| `mime` | no | Shown when present; the row says nothing rather than guessing from the extension. |
| `summary` | no | One line. |

Opened with the desktop's own handler, so Markdown goes wherever this machine
sends Markdown. The window never hard-wires a viewer.

### `markdown` — prose with structure

```json
{ "kind": "markdown", "model": { "body": "## Finding\n\nThe queue…" } }
```

### `table` — rows of comparable things

```json
{ "kind": "table",
  "model": { "columns": ["transport", "durable"],
             "rows": [["file", "yes"], ["escape sequence", null]] } }
```

A `null` cell renders as `unavailable`. An empty string renders as an empty
cell. **These are different**: one is a measurement nobody took, the other is a
blank somebody wrote.

### `architecture` — boxes and arrows

```json
{ "kind": "architecture",
  "model": {
    "nodes": [ { "id": "agent", "label": "Agent", "state": "running", "group": "pane" } ],
    "edges": [ { "from": "agent", "to": "queue", "label": "work order" } ] } }
```

`state` renders as a pill; `group` draws a labelled boundary. An edge naming a
node that was never declared is **kept and drawn as dangling**, because an arrow
to nowhere is a fact about your model and deleting it would hide a real mistake.

### `changeset` — a change a person can answer

```json
{ "kind": "changeset",
  "model": { "repository": "terminal-delight",
             "hunks": [ { "id": "surface.rs#4",
                          "file": "app/src/surface.rs",
                          "patch": "@@\n+one\n-two" } ] } }
```

`+`/`−` counts are computed from the patch, not taken from you. Every hunk
carries a three-state verdict — **undecided**, accepted, rejected — because a
proposed edit, a rejected one and an unread one are three different things.
`accept_part` / `reject_part` act on `id`.

### `decision` — a call to make

```json
{ "kind": "decision",
  "model": {
    "question": "Does the rail belong to the pane or the window?",
    "options": [ { "name": "In the pane", "recommended": true,
                   "case": "what it is genuinely better at",
                   "cost": "what it costs you" } ],
    "consequences": ["what follows if we do"] } }
```

**Exactly one option may be `recommended`.** A second claim is dropped by the
parser, because two recommendations is no recommendation.

### `unclassified` — everything else

Not an error, and not yours to send: it is what the window produces when it
cannot type your payload. The reason and the raw bytes are both kept and shown.
A kind this build has never heard of always lands — it is never dropped — so it
is always safe to try something new.

---

## 6 · Weights

Four optional judgements a reader wants and prose rarely carries.

| Field | Values | Answers |
|---|---|---|
| `effort` | `small` `medium` `large` `epic` | roughly how much work |
| `complexity` | `trivial` `moderate` `involved` `hairy` | how tangled — *not* the same question as how big |
| `foundation` | `{ system: "<free text>", depth: leaf\|component\|subsystem\|bedrock }` | if this is wrong, how much else is wrong with it |
| `confidence` | `measured` `inferred` `hunch` `unknown` | how much you stand behind it |

**Omitting a weight and declaring it `unknown` are different, and both are
supported.** An agent that says it does not know is telling the reader
something; an agent that never mentioned confidence is telling them nothing.
Nothing here defaults to a value, so a silent payload renders `unweighed`
rather than inventing a confident "trivial, leaf, measured".

---

## 7 · Actions — the channel back

A surface a person can only look at is a picture. These are what makes it an
interaction surface.

| Action | Performed by | Meaning |
|---|---|---|
| `open` | the window | open the artifact with the desktop handler |
| `open_source` | the window | open the first file in `source` |
| `approve` / `reject` | **the agent** | yes / no |
| `accept_part` / `reject_part` | the agent, with a `target` | a verdict on one hunk, also recorded on the row |
| `comment` | the agent | free text the person typed |
| `ask_agent` | the agent | put the question back |
| anything else | the agent | offered as-is, labelled as you spelled it |

### How the answer arrives

**As a line typed into your own terminal**, because a pseudoterminal is already
a two-way pipe to a program waiting for a human to say something:

```
[workbench] reject_part on surface change-847 · src/surface.rs#hunk-4 — Tube geometry shouldn't depend on terminal state.
```

One line, always, starting `[workbench]`. An agent that has never heard of this
protocol still receives a plain English instruction naming the thing and the
verb, and does the right thing anyway.

The same event is appended as JSON to the pane's action journal, for an agent
that wants the structure:

```
<pane dir>/actions.jsonl
{"td":"0.1","type":"action","surface":"change-847","action":"reject_part","target":"src/surface.rs#hunk-4","comment":"…"}
```

The journal is written **first**. A line typed into a terminal can be eaten by
whatever the program is doing at that instant; the file is what makes the answer
recoverable when it is.

---

## 8 · Errors

`present_surface` refuses a bad payload and says why, in words you can act on:

| Refusal | Cause |
|---|---|
| `missing 'td' version field` | no `td` |
| `payload says TDSP 1.x and this build speaks 0.x` | newer major |
| `unknown kind "hologram" — this build renders artifact, markdown, …` | a kind this build lacks (the **verb** refuses; a **file** lands as unclassified) |
| `"report.html" is relative — it would resolve against the TERMINAL's directory` | relative href |
| `'retire' needs the 'id' of the surface to take off the bench` | retire with no id |

The file and CLI transports never refuse: nobody is there to be told, so a
payload that cannot be typed becomes an `unclassified` surface carrying the
reason. **Nothing is ever silently dropped.**

Ask what this build can render:

```sh
terminal-delight surface --catalogue
```

or call the `surface_catalogue` MCP verb.

---

## 9 · What the window decides, not you

Given the same surface, the same pane renders it differently depending on how
much room it has and whether anyone is looking:

| Embodiment | When | What |
|---|---|---|
| **Full** | a focused pane with room | the whole thing, interactive |
| **Compact** | under 460px wide, or 220px tall | headings, counts, first rows, the recommendation |
| **Summary** | not focused, and under 640px | one line |

Also the window's: the rail's width, which shelf a kind files under
(`artifacts` / `decisions` / `other`), the colour of every marker, the corner
radius, and whether the pane is bent.

---

## 10 · Versioning

`major.minor`. A **minor** bump may only add optional fields and new kinds — a
0.1 payload must keep working on 0.9. A **major** bump may re-cut the envelope,
and a payload naming a newer major is refused rather than guessed at.

New kinds are additive by design: an older build renders a newer kind as
`unclassified` with a reason naming what it does know, which is a degradation
the sender can read.

---

## 11 · A worked example, end to end

```sh
dir="${XDG_STATE_HOME:-$HOME/.local/state}/terminal-delight/surfaces/$TD_SESSION/$TD_PANE_ID"
mkdir -p "$dir"
cat > "$dir/01-plan.json" <<'JSON'
{
  "td": "0.1",
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

The pane's header badge increments. Flip it to **BENCH**, and the decision is
drawn with the recommended option lit. Press `approve`, and the agent reads:

```
[workbench] approve on surface cache-eviction
```

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
| Types, parsing, validation, catalogue | `app/src/surface.rs` |
| Per-pane state: faces, shelves, selection, actions | `app/src/workbench.rs` |
| Renderers for the six kinds | `app/src/benchdraw.rs` |
| Transports: the sweep, the fence, the journal, the CLI | `app/src/surfacefeed.rs` |
| The MCP verbs | `app/src/mcp.rs` |
| The launcher that briefs an agent it starts | `app/src/launcher.rs` |
