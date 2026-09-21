# Program Design: The workbench follows the agent — store half

Scope is the half Parker kept: the store, the key, the sentinel, the
file-then-remove invariant, and `Bench` gaining a conversation. The four screens
are held for the decoupling. The ledger reader (`tenancy_for` / `tenancy_of`) is
the other pane's and is already built on `bench/ledger-tenancy`.

Read against `origin/main` at `033a7ce`.

## Files

| File | Why it lives there |
|---|---|
| `app/src/benchstore.rs` — new | The only writer of `conversations/`. No gpui, no pane, no window, so it tests like `session.rs` does — std only, against a scratch directory. |
| `app/src/surfacefeed.rs` | The mailbox gains the sentinel and the drain. Both are facts about the inbox, and the inbox is this module's subject. |
| `app/src/workbench.rs` | `Bench` gains the conversation it is showing. The type already owns "what is on this bench"; this is the missing half of that sentence. |
| `app/src/pane.rs` | `set_mode`'s two edges drive it — the departure clears, the arrival loads. Already the single funnel, already acting on one edge. |
| `app/src/main.rs` | The sweep loop asks `tenancy` first and `paneident` second, and files what it takes. |
| `app/src/tenancy.rs` | Not ours. Consumed, not written. |

## Types & signatures

### `benchstore` — the conversation's record

```rust
/// Which conversation a bench is showing, and which segment of it.
///
/// `root` is a session id the ledger minted; `seq` counts continuations
/// (a compaction or an in-pane resume). A `/clear` mints a NEW ROOT, so two
/// segments under one root are always the same conversation carried on.
#[derive(Clone, PartialEq, Eq, Debug, Hash)]
pub struct ConvKey {
    pub root: String,
    pub seq: u32,
}

impl ConvKey {
    /// `conversations/<root>/<seq>/`, or `None` when `root` could name a path.
    ///
    /// Validated HERE as well as in the hook. The ledger is a user-writable
    /// file, so a root reaching a directory name is validated at both ends by
    /// design — see the architecture's note on why one end is not enough.
    pub fn dir(&self, root_dir: &Path) -> Option<PathBuf>;
}

/// One event in a conversation's record. Append-only, oldest first.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Turn {
    /// What the person said, as the pane's scrollback latch saw it.
    Ask { n: u32, at_ms: u64, lines: Vec<String> },
    /// A surface that answered turn `n`.
    Said { n: u32, at_ms: u64, id: SurfaceId, bond: Bond },
}

/// Write a surface into a conversation's record, then return the inbox path to
/// retire. NEVER removes it — the caller does, after this returns Ok, so a
/// crash between the two loses nothing.
pub fn file(root_dir: &Path, key: &ConvKey, turn: u32, s: &Surface, bond: Bond)
    -> io::Result<()>;

/// Open a turn. Called on the edge where the pane latches a human message.
pub fn ask(root_dir: &Path, key: &ConvKey, n: u32, lines: &[String]) -> io::Result<()>;

/// Every segment of one conversation, oldest first, capped at
/// [`crate::surface::PANE_HISTORY_CAP`].
///
/// Reads the WHOLE ROOT — `<root>/0/`, `<root>/1/`, … — because segments are
/// continuations, not boundaries. Segments are ordered NUMERICALLY: `10` sorts
/// before `2` by name, and a nine-segment conversation is an ordinary week.
pub fn load(root_dir: &Path, root: &str) -> Vec<(Surface, Option<u32>)>;

/// The turn each surface answered, for captioning a card that is not the
/// newest one. `None` where the record does not say.
pub fn asks(root_dir: &Path, root: &str) -> Vec<Turn>;

/// Conversations untouched for longer than the window, removed whole.
/// Keyed on the conversation's own last activity, never on a window session.
pub fn prune(root_dir: &Path, now_ms: u64) -> Pruned;
```

### `surfacefeed` — the sentinel, and the drain

```rust
/// The names present the first time a build carrying this feature swept a pane
/// directory. Written once, never rewritten; membership IS "pre-existing".
///
/// Not `*.json`: `sweep_pane` and `cap_pane` both skip any other extension
/// (211, 931), so a dotfile is invisible to every existing reader, and
/// `swept.json` would be read back as a surface by the sweep it informs.
const SENTINEL: &str = ".swept";

/// Three states, because a directory whose history cannot be established is
/// not the same as one with nothing in it.
pub enum FirstSeen {
    /// Named in the sentinel — present before this build ever looked.
    Preexisting,
    /// Not named — this build watched it arrive.
    Witnessed,
    /// No sentinel and none could be written. Neither claim is available.
    Unknown,
}

impl Feed {
    /// Write the sentinel if absent, then classify. Idempotent and write-once:
    /// re-writing it would make everything then present look pre-existing,
    /// which is the per-run bug this replaced.
    pub fn first_seen(&mut self, dir: &Path, file: &Path) -> FirstSeen;
}

/// Remove one taken surface from the inbox, AFTER it is in the store.
/// Wraps the existing [`retire_surface`]; named apart so the ordering rule has
/// somewhere to be stated and somewhere to be tested.
pub fn drain(dir: &Path, id: &str) -> bool;
```

### `workbench::Bench` — the conversation it is showing

```rust
impl Bench {
    /// Which conversation these surfaces belong to. `None` on a shell pane,
    /// where the bench stays pane-keyed exactly as today.
    pub fn conversation(&self) -> Option<&ConvKey>;

    /// Show a conversation. Clears first, ALWAYS — including when the key is
    /// equal, because the caller only reaches here on an edge.
    pub fn set_conversation(&mut self, key: Option<ConvKey>);

    /// Empty the bench, keeping the face and the shelf the reader chose.
    /// A pane whose agent left is still a pane they were looking at.
    pub fn clear_surfaces(&mut self);
}
```

`Bench`'s doc comment loses its first sentence. *"Per pane, deliberately"* becomes
*"Per conversation on an agent pane, per pane on a shell pane"*; the second
sentence — *"a shared store would put one agent's diagram on another agent's
bench"* — was always the real reason and stays.

## Call stack

**A surface arriving.** `main.rs` sweep loop
→ `tenancy::tenancy_for(agent_pid)` → `ConvKey`, falling back to
`paneident::certain` → no key at all
→ `Feed::sweep` → `Arrivals`
→ `Workspace::deliver_surfaces`
→ `TerminalView::present`
→ `benchstore::file(...)` **then** `surfacefeed::drain(...)`
→ `Bench::apply`.

**An agent departing.** `pane.rs::set_mode`, `departed` edge
→ `Bench::set_conversation(None)` → `clear_surfaces`. The store is untouched.

**An agent arriving or resuming.** `set_mode`, `arrived` edge
→ the next sweep binds the pane
→ `Bench::set_conversation(Some(key))`
→ `benchstore::load(root)` — every segment, numerically ordered
→ `Bench::apply` per surface.

**A human turn.** Two sources, and the premise decides which is primary.

- **Authored** — the composer sends a draft through the decoupled API
  → `benchstore::ask(key, n, lines)` with the exact text this window sent.
  Primary, once the boundary exists. No scrape, no second copy, no drift.
- **Observed** — `pane.rs::latch_asked` scraping the pane's scrollback.
  The terminal face's fallback, and all there is until the boundary lands.

`ask` takes lines and an ordinal from either source, so the store does not know
or care which one it was. That is deliberate: it is what lets this half ship
before the API exists and improve when it arrives, without a migration.

## Test plan

Named for what they assert, and every one of them fails on `origin/main` today.

**The defect this feature exists to remove**

- `two_agents_under_one_shell_do_not_share_a_bench` — two `ConvKey`s from one
  `shell_pid`; each files and loads only its own. This is the live case on this
  machine (shell 1320940) and the reason the ledger outranks `paneident`.
- `a_surface_is_never_filed_under_a_conversation_nobody_named` — no tenancy and
  no certain binding ⇒ nothing written under any root.
- `a_session_id_in_the_payload_is_not_read` — a surface claiming a foreign root
  files under the resolved one.
- `a_root_that_could_name_a_path_is_refused` — `../../etc/passwd` as a root
  yields no directory. (The reader's twin of the hook's own test; both ends.)

**The store**

- `a_clear_starts_an_empty_bench` — root A seq 0, then a clear mints root B;
  loading B returns nothing.
- `a_compaction_keeps_the_whole_conversation` — root A seq 0 and seq 1; loading
  A returns both, oldest first. **This is the one my own design broke**, so it
  is the one to write first.
- `segments_load_in_numeric_not_lexical_order` — eleven segments; `10` after `9`.
- `a_conversation_outlives_the_window_that_made_it` — file under one window
  session key, load under another.

**The sentinel**

- `a_file_present_at_first_sweep_is_pre_existing`
- `a_file_arriving_later_is_witnessed`
- `the_sentinel_is_written_once_and_not_rewritten` — sweep twice, assert mtime
  and contents unmoved after a new file lands.
- `a_directory_whose_history_cannot_be_written_is_unknown` — read-only dir ⇒
  `FirstSeen::Unknown`, not `Preexisting`.
- `the_sentinel_is_not_read_back_as_a_surface` — after writing it, `sweep_pane`
  returns nothing new.

**The drain**

- `a_filed_surface_leaves_the_inbox`
- `a_surface_that_failed_to_file_stays_in_the_inbox` — store write errors ⇒ the
  file is still there. The crash-window rule, as a test.
- `a_drained_surface_does_not_come_back_on_restart` — a fresh `Feed` re-reading
  the directory finds nothing.

**The bench**

- `an_agent_leaving_clears_the_bench_and_not_the_store`
- `a_shell_pane_bench_is_untouched` — the script and demo drop path, which slice
  1 would otherwise break.
- `the_face_and_shelf_survive_a_clear`

## Least confident decisions

1. ~~**`ask`'s turn ordinal is counted by the window watching the terminal.**~~
   **Retired 2026-09-21 by the decoupling premise.** A workbench that holds the
   draft and sends it through an API authors every ask rather than observing one,
   so the ordinal is exact. The scrape survives as the terminal face's fallback
   and keeps the old exposure only there. Kept visible rather than deleted
   because the signature is shaped by having had two sources.
2. **`load` caps at `PANE_HISTORY_CAP` (64) across the whole root, not per
   segment.** A conversation with twenty segments shows its newest 64 surfaces
   rather than 64 from each. That seems right and is not measured.
3. **`file` writes before `drain` with no fsync between them.** The ordering
   protects against a crash; it does not protect against a power loss that
   reorders the two writes. Stating it rather than solving it — the existing
   `drop_surface` has the same exposure.
4. **`set_conversation` clears even when the key is unchanged.** Safe, and it
   means a spurious edge costs a reload. Cheaper than the alternative failure.
