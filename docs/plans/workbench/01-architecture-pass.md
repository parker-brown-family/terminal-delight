# The architecture pass

**Difficulty: 6/10** — broad but mechanical. A mistake here shows up as a
compile error or as one of 1,172 tests, and the expensive part — the wire
vocabulary other agents will target — is already frozen at TDSP 0.2. One plan
page, one approval, and the slices are cheap to unwind.

**Where this comes from.** Two readings taken on the same afternoon. Parker's,
after a day of live rounds: *the front end is functionally there, the backend is
spaghetti and needs a refactor plan and pass.* And the composer diagnostic
another agent wrote from photographs, which found four things (§3). This page
folds both into one sequence, says which slices are already landed, and names
the decisions that are Parker's rather than the code's.

---

## 1 · What the pass is for

Every defect of the last day had the same shape: **a decision made where no
assertion could reach it.** A rule inside a renderer, a heuristic spread across
three files, a version number nobody was reminded to bump. The pass does not
add features. It moves each such decision to a place with a test, and puts a
guard behind it so it stays there.

The three rules the pass installs, each with a mechanical guard:

| Rule | Guard |
|---|---|
| Everything that reads the terminal grid as text lives in `screenread.rs`, and every public reader there has a test transcribed from a real screen | `screenread::tests::every_public_reader_has_a_test_in_this_module` — a source scan, not a list |
| The bench does not live inside the terminal's file | `pane::tests::no_bench_method_is_defined_in_pane_rs` — a source scan, not a list |
| A renderer contains no decisions; a decision is a pure function in `workbench.rs` with a table test | `benchdraw::tests::a_renderer_contains_no_decisions` — a scan of the renderer's code for a threshold (a comparison against a non-zero number) or a read of the environment or the clock; mutation-tested with three plants caught and two allowed shapes passing |

---

## 2 · The slices

| # | Slice | State | Commit |
|---|---|---|---|
| 1 | `screenread.rs` — 11 readers out of 3 files, under the transcribed-test law | **landed** | `cd8005c` |
| 2 | `pane/bench.rs` — 22 methods + the key handler out of `pane.rs` (11,368 → 9,487 lines), as a child module so no field changes visibility | **landed** | `ae38d42` |
| 3 | `body()` has three arms each matching every kind — collapse to one kind-match per embodiment, so a kind added once is drawn everywhere | **disproved, not done.** `compact()` and `full()` each match every `Kind` with no wildcard, so a kind added to `surface.rs` already fails to compile until every embodiment draws it — the guarantee the collapse was for. The issue's own invalidation criterion, met (#486) | — |
| 4 | Mechanical guard for rule three | **landed** — a scan of the renderer's code for a threshold or an environment/clock read, narrow on purpose so it cannot cry wolf | `47cbc76` |
| 5 | `bench_type` appended on the PTY and replaced in the mirror — the diagnostic's first finding, root-caused (§3) | **landed** | `72950ad` |
| 6 | TDSP 0.2 with a compatibility test that parses 0.1, 0.2, 0.9 and refuses 1.0 | **landed** | `03a1358` |
| 7 | `host_socket` — flaked FIVE times under a parallel release build across TWO different tests, green every time alone | **root-caused, in the tests.** Reproduced one run in eight under twelve busy cores: the wait returned on the terminal's echo of a typed line before `cat`'s own copy of it, which then landed in the next window's live stream after a snapshot that held the echo — the fence assertion was right. Both waits now require the second copy; the socket file's limits are one thirty-second constant (#438) | `9722402` |
| 8 | The CRT warp on the bench — Parker decided; the bench bends and hit-tests through the warp's inverse | **landed**, residue closed in 5299c1d: the wheel and the pointer's shape go through the same inverse, and the corners are tested (§4) | `46ec6b6`, 5299c1d |
| 9 | The composer sits in the vignette's darkest band — confirmed from code, §3-03 | **decided: exempt.** The bench renders with a vignette of zero; the terminal's grade is untouched (#488) | `5b3f1d6` |
| 10 | `ctl bench choose\|say\|type` took the first bench-face agent pane in tab order while their documentation promised the focused pane — in a restored window of seventeen tabs it typed into a pane nobody could see | **landed** — the focused pane first, then the nearest qualifying pane; the rule is a table (`workbench::bench_target`) (#489) | `2d63f9a` |
| 11 | The spec's implementation map credited `derive.rs` and `hud.rs` with what `screenread.rs` does, and did not know `TD_BENCHMIRROR` | **landed** (#487) | `120ab9c` |

What each landed slice cost and found is in its commit message; they are written
for the next reader, not for the log.

---

## 3 · The composer diagnostic's four findings, dispositioned

The other agent read photographs and reported what it saw, marking each
*measured*, *inferred* or *contradicted*. Its method was right and two of its
conclusions were wrong for reasons it could not have seen from pixels — which is
itself the finding: **a photograph shows the mirror, and the mirror is not the
agent.**

**01 · Appends concatenate without a separator** — *measured.* Three seams in one
submission: `sentencehalf`, `grow.Spin`, `Delight.I'm`. **Root cause was not a
missing separator.** `bench_type` sent bytes down the pseudoterminal, where the
agent's line editor appended them, while REPLACING the local mirror — so after a
second call the box showed one fragment, the agent held two, and the caret was
wrong by the length of the first. The seams were the harness sending three
`type` calls with no spaces, which is correct: a keystroke stream has no
separators either. The mirror now appends (slice 5). **No space is inserted** —
a verb that quietly added one could not spell a word across two calls.

**02 · The image arrives as literal `[Image #7]`** — *measured.* It is the
agent's own chip: Claude Code binds `ctrl+v` to its image paste, reads the
clipboard itself and writes that token into its prompt. The bench passes the
chord through and shows a count beside the line rather than guessing the number.
**Decided; in the spec, §5 `question`.**

**03 · Text dims toward the newest line** — *inferred by the diagnostic,
confirmed here from code.* The vignette is not in the warp shader; it is an
inset shadow on the pane's content box (`crt.rs`, the `vignette > 0.001` arm:
`hsla(0,0,0,0.78·v)` and `0.56·v` inset, darkest at the edges). The bench is a
child of that box, the composer is its bottom-most element, so the line being
typed sits in the darkest band by construction. A photograph could only have
shown the symptom; the code shows why it cannot be otherwise while the bench
shares the tube's grade. Slice 9, Parker's decision (§4).

**04 · Nothing says what happens past the visible height** — *measured then.*
Decided since: the box grows to a third of the pane, then scrolls, the end
stays visible, and a line says how many characters are above the fold. In the
spec.

**The one it got right that nobody asked** — *contradicted*: the two-tone border
it had reported from an earlier crop was the agent's own reading error, and it
said so. That is the standard.

---

## 4 · Parker's decisions, not the code's

**The warp — decided by Parker, and built.** The concern was real and is
written in `46ec6b6`: the barrel warp is a pixel post-pass while gpui hit-tests
the element tree flat. The bench now does its own hit-testing. Every control
records its flat rectangle as it paints (`benchdraw::zone`), the root mouse
handler un-bends the pointer with the pane's own coefficients through the
grid's inverse (`workbench::unwarp`, tested for fixed centre, identity at
`k = 0`, radial symmetry) and looks the flat point up (`workbench::hit_at`,
last-painted wins, edges do not double-claim). Fifteen closures became fifteen
`Hit` variants dispatched by one `match` the compiler makes complete.

*The residue, and what it turned out to be.* `46ec6b6` left the scroll wheel
and the hover cursor gpui-dispatched flat and gave as its reason that
`ScrollHandle` has no public setter. **That was false** — `set_offset` is
`pub` in the gpui checkout the app builds — and it had been written into a
commit and this page without a grep. Nor was the residue cosmetic, which the
same commit had also claimed: at the tube's real coefficients on a 1500×1000
pane a corner control is shown **76.8 px** from where it was laid, nearly four
times its own width, and the composer's top-left corner **62.5 px** — a wheel
aimed at the visible edge of the draft was scrolling the card above it. 5299c1d
closes it: one element, the bench's pointer hook, painted last over the whole
bench, un-bends the wheel in the capture phase (drives the composer's
`ScrollHandle` itself, or the mirror through the terminal's scroll, and stops
propagation so nothing under it ever scrolls flat) and requests the cursor
from the un-bent hit against its own hitbox, so the hand appears over what the
tube shows as a button. Every child's own cursor hint is gone. The composer
also stopped pinning its tail with `justify_end` — a gpui scroll container
cannot scroll into an overhang at the top, so the wheel could never reach the
first line of a long draft — and follows the caret instead
(`workbench::follows`). A test walks the four corners and the composer's
corner: the flat lookup of where the tube shows each control must miss, and
the un-bent lookup must land.

*Verification channel, still:* a shell with no virtual pointer cannot press
the surface, so `TD_HITDEBUG=1` prints each click's whole chain — pointer,
curvature, un-bent point, zone count, hit — and a person's clicks are read
back from the window's log. One click has been read back, at the centre. A
person's corner click and a wheel over the bent edge of a long draft remain
the final proof.

**The composer in the vignette — decided: exempt** (`5b3f1d6`, #488). §3-03
was confirmed from code; Parker asked for everything fixed and the
recommendation was taken. The bench renders the glass with a vignette of zero
— no edge fade, no specular — and keeps the scanlines, the bloom and the bend.
The terminal's grade is untouched. The alternative, lifting the composer's
floor, would have moved the composer for a reason nobody looking at the bench
could see. The rule is `workbench::vignette_on`, one call site in `pane.rs`,
one table test.

**`TD_BENCHMIRROR`.** The agent's scrollback stays off the bench by default and
comes back under that flag for anybody debugging what the screen reader sees.
Nothing was disconnected to turn it off. Documented in the spec's flags table
(`120ab9c`, #487). What the bench should show of the agent's own screen, if
anything, is the one question on this page still without an answer — #490, a
design pin, not a defect.

---

## 5 · What "done" looks like for this pass

- Rules one, two and three are mechanical and mutation-tested (they are).
- The warp is on the bench and every control, the wheel and the pointer go through its inverse (they do; the corners are tested, and a person's corner click under `TD_HITDEBUG=1` is the last proof still owed).
- `body()` draws every kind in every embodiment (it does — by exhaustive matches, which is why slice 3 was disproved rather than done).
- The three decisions in §4 have an answer written next to them (two do; the mirror is #490).
- The flake is root-caused and fixed in the tests, not a memory (it is; #438 carries the reproduction).
- Difficulty in hindsight recorded at the foot of `00-status.md`.
