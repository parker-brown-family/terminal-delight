# Git Coordination Rail

Difficulty: **4/10 for this design brief** — the surface can be judged and revised before application code changes. This score does not size the future schema and UI implementation.

Implementation difficulty: **8/10** — permission-scoped graph publication, a native inspector and bounded background collection must agree without leaking hidden activity.

Status: **implementation verified locally, ready to land**. Native and MCP are wired; the exact source passed 2,445 default-core tests and 2,398 fallback-core tests (nine intentionally ignored in each), strict production Clippy on both cores, formatting and release build. Native focus, narrow layouts, permission cycling and immediate copied-context revocation passed. Two deliberate model faults failed assertions. Parker concurred with all three amended recommendations and explicitly requested a complete build while AFK. Architecture, program design and slices record the autonomous decisions under that authorization.

Previous brief status: **interface revision v2 delivered for review**. The collector, graph and permission-scoped MCP direction is retained following the review. Entry-point and signal-policy recommendations were amended. The subsequent AFK instruction authorizes implementation.

The combined plan is [the illustrated decision brief](../../../reports/2026-10-18-git-coordination-rail.html). Its three decision rows cover entry point, signal policy and first-release scope. Notes and concurs in the HTML are the review channel. No approved gate is being reopened: the earlier `project-rail` plan describes the shipped scanner and ticker that this proposal extends.

Source handoff: `/home/parker/Downloads/TERMINAL_DELIGHT_GIT_COORDINATION_FRICTION_HANDOFF.md`, dated 2026-10-18. Read completely. Audit baseline: fetched `origin/main` at `f1d99f922b0914709ab916467cd9654224d9660f`. The working checkout was `ef6dcde524a13b2e9c2d0b9477d1a93436a4afbd` and retained unrelated changes.

The revised route replaces the rotating ticker with one compact window-wide Git signal. Detailed Git metrics remain in project inspection. All projects → project clusters → joined evidence retain three semantic depths, with expandable preview shortcuts. Source-labelled footprints and stable identities precede file grouping. The first tracer crosses scanner, native UI and permission-scoped MCP; authorization precedes graph derivation. Existing `engineering_state` semantics remain stable subject to the required exposure audit. Reuse the existing running-window MCP relay.

The original planning task changed no TD application code. Implementation now proceeds in the isolated `feat/git-coordination` worktree. Validation covers the document's layout, prototype navigation, notes and saved-file format. Native UI, performance, Rust regression suites and end-to-end exposure reproduction belong to implementation acceptance, not this brief.

Post-hoc difficulty: **4/10** — the principal design work was separating pane occupancy from agent identity and coverage from severity; source inspection also corrected the handoff's relay assumption.

The first [verification record](verification.json) checked Escape only after forcing focus to the demo container; it missed the review's stronger focus-loss case. [Before evidence](keyboard-before.json) reproduces that defect: all three Escapes left BODY focused and the project open.

## Interface Revision

The revision follows `/home/parker/Downloads/TERMINAL_DELIGHT_GCF_PROPOSAL_REVIEW.md` and `/home/parker/Downloads/prototype_test_results.json`, read with the embedded annotation. The [first version](../../../reports/2026-10-18-git-coordination-rail-v1.html) preserves the original proposal and saved ticker note; that note also remains at the same anchor in the revised page.

- A clear selected project cannot conceal another project's alert. Unknown sorts ahead of clear; selection remains a quiet marker, and project reading ages are visible.
- Ranked cluster previews expand in the overview. Evidence joins identities, panes, checkout roots, branches, sources, timestamps and expandable refs. B/C can represent one known session.
- Target checks distinguish not checked, checked-none, conflict, unavailable and stale. Known-member evidence coverage is separate from graph/membership coverage.
- The bridge fixture joins two groups without implying less friction. The dense fixture retains 11 inspectable hotspots across 40 checkouts, with bounded pages.
- Native acceptance includes hidden-only changes to IDs, titles, ordering, authorized revisions/cursors and deltas. One shared fixture must drive actual native and MCP response tests; HTML projections do not establish production parity. All four read/delta tools remain in scope.
- Navigation restores the originating selection and scroll position. The exact Focus host B → Escape → Escape sequence passes without a compensating focus call.

The revised [browser verification](verification-v2.json) covers those interactions, keyboard-only entry/back, all five target states, notes/concur save/reopen and five viewport widths from 390 to 1600 CSS px. [Reproducible browser check](prototype-check.cjs). The [revised narrative gate](narrative-v2.json) classified the page as a journey. The source audit above is retained from the first version; this interface revision does not claim a fresh audit of current main.

Revision post-hoc difficulty: **4/10**. The focus oracle needed repair; the rest corrected attention scope and completed the relationship inspector while retaining the observation architecture.

## Implementation acceptance

The native fixture has exercised a clear selected project with a warning elsewhere, both group types, joined branches/panes/path sources, natural repeated Escape after focusing Host B, and widths of 390, 520 and 1500 logical pixels. Agent exposure changes were exercised on the fixture window, with explicit reset and legacy aggregate refusal. The final release evidence will be recorded in `implementation-verification.json`.

A loaded full run reproduced the existing host environment timing race tracked by the issue titled “a pane knows which session and pane it is can read the child's environment before it has exec'd” (<https://github.com/parker-brown-family/terminal-delight/issues/933>). It passed twice in isolation; production host behavior remains unchanged; the test now waits for exec before reading the child environment. CI-equivalent product Clippy passes. The separate existing test-target lint gap remains tracked by <https://github.com/parker-brown-family/terminal-delight/issues/537>.

The [built-feature report](../../../reports/2026-10-18-git-coordination-built.html) is the human reading. [Exact-source acceptance](implementation-verification.json), [browser checks](implementation-browser.json) and [narrative measurement](implementation-narrative.json) record the evidence. Native clipboard checks clear the clipboard to a sentinel before every copy: an unchanged value is a failure, never a successful navigation observation. Scope changes are acknowledged through status without requesting a snapshot before copying, so the test cannot accidentally repair the cache it is examining.

Implementation post-hoc difficulty: **8/10**. The difficult boundaries were permission projection before identities/history, independent scope epochs after policy cycling, and deferred native focus after pane navigation. The host test-only exec wait passed twenty loaded full unit-suite runs. Optional line analysis and push notifications remain outside the approved release.
