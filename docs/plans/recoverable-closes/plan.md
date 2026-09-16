# One shutdown protocol

**Difficulty 6/10** — four verbs converge on one protocol, and what is being made
recoverable is live processes: a wrong hold keeps shells alive forever or ends them
early, and either is discovered hours later. Six buys this single plan page and one
approval, not four gates.

**Status:** awaiting approval. Slices below are the unit of work; each lands with tests.

## What is wrong today

Dropping a branch into the bay holds it. Every other way of ending something kills it
on the spot, and the bay's `n recoverable` never mentions it.

| Verb | Today | Recoverable |
|---|---|---|
| Drag a project or group into the bay | `delete_branch` → `Trash` | yes |
| Drag a tab into the bay | `trash_tab` → `Trash` | yes |
| Bar menu on a project or group → *Delete* | `ask_delete` → `delete_branch` | yes |
| **Bar menu on a tab → *Close*** | `ask_close_tab` → `close_tab` → `hangup` | **no** |
| **The tab's ✕, and ctrl+w** | `request_close_tab` → `close_tab` → `hangup` | **no** |
| **alt+w on the focused pane** | `close_pane` → `hangup` | **no** |
| Group menu → *Disband* | clears membership only | nothing ends |

`hangup` is the single place a shell actually dies: it asks the host to close the pane.
The three rows marked no call it inline. The three rows marked yes never call it — they
move the payload into `Trash<Deleted>`, and `end_held` calls `hangup` later, when a
holding is evicted by the cap or swept past its deadline.

**So the protocol already exists and three verbs ignore it.** This is a routing job plus
one genuinely new thing: a pane needs somewhere to come back to.

## The decision that is already made

The close-undo product gate (`docs/plans/client-server/01-product.md`, approved
2026-09-09) settled the policy, and `hold.rs` ships those numbers today:

- One pane held **one hour**; two or more held **four hours**; counted in **panes, not
  in the verb** that closed them. Cap of **ten**, newest first, eleventh evicts the oldest.
- Reopen is **ctrl+shift+z**, most-recently-closed first — not ctrl+shift+t, which is
  already this terminal's new-tab chord.
- A held leaf is a **third state** in the saved layout, carrying its deadline. It must not
  be encoded as an absence and must not be encoded as a live leaf.
- Reopening must say **which of the two things** you are getting: the live process back,
  or a restart from its resume line. Those are different facts.

Nothing in that gate has been built. This plan builds the first three; the fourth is
slice 4 and is the same finding the incinerator brief put at the top of its list.

## The shape

One enum for what a retirement took, because the three kinds come back differently:

```rust
/// What one retirement is holding, and everything needed to put it back.
enum Retired {
    /// A project or a group, with its tabs, its groups, and where they sat.
    Branch(Deleted),
    /// A whole tab, with the index it sat at.
    Tab { at: usize, tab: Tab, was_active: bool },
    /// One pane out of a split, with the seat it came out of.
    Pane(HeldPane),
}
```

`Trash<T>` is already generic and already holds a payload it never looks inside, so the
change there is `Trash<Deleted>` becoming `Trash<Retired>` and `end_held` learning to
walk all three shapes to find the leaves it must hang up. The timer, the cap, the
three-state `Recovered`, the live-reading tray: untouched.

### A pane needs a seat

A tab is put back by index. A pane has no index — it came out of a split tree, and
`remove_leaf` collapses its parent onto its sibling, so the shape it left is gone the
moment it leaves.

Splits already carry a stable `id: u64`. A seat is therefore the parent split's id, which
side the pane was on, and that split's direction and ratio:

```rust
struct HeldPane {
    leaf: Entity<TerminalView>,   // never dropped while held — this is what keeps it running
    seat: Seat,                   // parent split id, side, dir, ratio
    tab_at: usize,                // where the tab was, as a hint
    tab_ident: TabIdentity,       // what the tab was, for the fallback
}
```

Recovery tries three things in order, and **says which one happened**:

1. The seat still exists → split the sibling back the way it was, same ratio. *"back where it was."*
2. The tab exists but the seat is gone (the sibling moved, or split again) → add it as a
   new split at the end of that tab. *"back in its tab, in a new seat."*
3. The tab is gone → a new tab wearing the old tab's identity. *"back as its own tab."*

Case 3 is not a failure and is not an absence: it is drawn and labelled, because the
alternative is a tray that says "recovered" and quietly means something else.

### What the bay says

The bay already reads the trash live, so every retirement appears in `n recoverable` and
in the tray with no further work. Two additions:

- The tray row names the kind, since recovering a project and recovering one pane are
  different offers: `Recover "UX" — 3h 41m` against `Recover pane in "shorts" — 52m`.
- The first time a close is held, one hint, once: an undo nobody knows about is an undo
  nobody uses. (The product gate asked for this and for an F1 line.)

## Slices

**1 — Tabs stop dying.** `Retired` enum; `close_tab` hands its payload to the trash
instead of calling `hangup`; ✕, ctrl+w and the menu's *Close* all route there. The
confirmation for a multi-pane tab stays — it is a different question now ("this is
recoverable for four hours") and its wording changes with it.
*Tests:* closing a tab does not reach `hangup`; the tab comes back at its index with its
panes still running; an eviction hangs up exactly the evicted tab's leaves and nothing else.

**2 — Panes stop dying.** `HeldPane` and `Seat`; `close_pane` (alt+w) routes through the
trash; the three-case recovery above.
*Tests:* a seat survives and is reused; a collapsed seat falls back without losing the
pane; a vanished tab produces a new tab with the old identity; each case reports itself.

**3 — The key and the telling.** ctrl+shift+z, most-recent-first on repeat; an F1 help
line; the one-time hint.
*Tests:* repeat presses walk the stack newest-first and skip expired holdings.

**4 — Surviving the window.** The held layout entry with its deadline in the session
file, so a holding outlives the window that made it. This is the incinerator brief's
first finding and the product gate's third bullet. Bigger than the other three together,
and it needs the host to own the holding rather than the window.

## What this is not

- **Not a change to what closing means.** A manual close is still intent. The gate
  settled that in 2026-09-08 and the only amendment since is that intent is deferred.
- **Not a new place to look.** Everything retired shows up in the bay that already exists.
- **Not disband.** Disbanding a group ends no processes and stays immediate.

## How to invalidate this plan

If `close_tab` already avoids `hangup` on some path, or a pane close already leaves the
host holding the pty, then slices 1 and 2 are smaller than written and the tests below
should be written first to prove it. Check: close a tab, then ask the host for its child
count — if it does not drop, the shell is already being kept and this plan is describing
a problem that is not there.
