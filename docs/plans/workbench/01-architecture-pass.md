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
| A renderer contains no decisions; a decision is a pure function in `workbench.rs` with a table test | not yet mechanical — slice 3 |

---

## 2 · The slices

| # | Slice | State | Commit |
|---|---|---|---|
| 1 | `screenread.rs` — 11 readers out of 3 files, under the transcribed-test law | **landed** | `cd8005c` |
| 2 | `pane/bench.rs` — 22 methods + the key handler out of `pane.rs` (11,368 → 9,487 lines), as a child module so no field changes visibility | **landed** | `ae38d42` |
| 3 | `body()` has three arms each matching every kind — collapse to one kind-match per embodiment, so a kind added once is drawn everywhere | next | |
| 4 | Mechanical guard for rule three: a scan of `benchdraw.rs` for `if`/`match` on state that is not a `Shows`/`Standing`/`Peel` value | after 3 | |
| 5 | `bench_type` appended on the PTY and replaced in the mirror — the diagnostic's first finding, root-caused (§3) | **landed** | `72950ad` |
| 6 | TDSP 0.2 with a compatibility test that parses 0.1, 0.2, 0.9 and refuses 1.0 | **landed** | `03a1358` |
| 7 | `host_socket` — flaked FIVE times under a parallel release build across TWO different tests, green every time alone; that points at the binary's shared PTY/socket harness, not at either test | file as a falsifiable issue when the branch lands | |
| 8 | The CRT warp on the bench — Parker decided; the bench bends and hit-tests through the warp's inverse | **landed** | `46ec6b6` |
| 9 | The composer sits in the vignette's darkest band — confirmed from code, §3-03 | **decision** — §4 | |

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

*Residue, so nobody rediscovers it as a bug:* the scroll wheel and hover
cursors are still gpui-dispatched flat — `ScrollHandle` has no public setter.
The composer sits bottom-centre where barrel displacement is smallest, and a
wheel a few pixels off scrolls the same box. *Verification channel:* a shell
with no virtual pointer cannot press the surface, so `TD_HITDEBUG=1` prints
each click's whole chain — pointer, curvature, un-bent point, zone count, hit —
and a person's clicks are read back from the window's log.

**The composer in the vignette.** If §3-03 confirms, the options are to exempt
the bench body from the vignette (it is flat already, so the grade is the only
CRT term it carries) or to lift the composer's floor. **Recommendation: exempt
the bench body** — the vignette is the tube's, and the bench is not in the tube.

**`TD_BENCHMIRROR`.** The agent's scrollback stays off the bench by default and
comes back under that flag for anybody debugging what the screen reader sees.
Nothing was disconnected to turn it off. **Recommendation: keep the flag,
document it in the spec's implementation map.**

---

## 5 · What "done" looks like for this pass

- Rules one and two are mechanical and mutation-tested (they are).
- The warp is on the bench and every control still lands (it is; verify by clicking under `TD_HITDEBUG=1`).
- Rule three is mechanical (slice 4).
- `body()` matches each kind once per embodiment (slice 3).
- The three decisions in §4 have an answer written next to them.
- The flake is an issue with a reproduction, not a memory.
- Difficulty in hindsight recorded at the foot of `00-status.md`.
