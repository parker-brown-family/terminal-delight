# Problem

The remaining TD rendering cost may be reduced by retaining layout/paint for a quiet terminal pane while its visible sibling prints. The mechanism is present in `Workspace::render_node`: each leaf uses `.child(e.clone())` (main.rs around 26391), a plain GPUI view. GPUI `AnyView::request_layout` renders a plain view each parent pass; the native cached path instead checks dirty notifications, bounds, text style and clipping. The size of the saving for terminal panes is an untested hypothesis.

## Proof it's real

After the styling/document reuse candidate, a separate 12-second, user-only, 99 Hz main-thread profile in the four-visible-pane fixture recorded 600 samples, zero lost. The listed Taffy layout functions alone account for more than 41% of self samples. This measures layout cost, not its cause or an obtainable saving. Three panes were quiet and one targeted 20 Hz output. The complete raw paired CPU series and profile are under reports/2026-10-17-td-styling-evidence/ on branch perf/td-efficiency. Candidate normal-release binary SHA256 d6441bc744c95de50a49891f4e38fe70ae7276c00b73bea25534f7cb2ce69d57.

## Reproduce

Build normal release binaries and run the preserved four-pane diagnostic, then profile separately with profile.sh. Add a test-only TerminalView render counter to identify how many quiet sibling renders occur per busy-pane update. Do not infer this from a framework symbol alone.

## Verify it IS a problem

Try invalidation first. If quiet panes already avoid rebuilding, or skipping their preparation saves less than two CPU percentage points in five matched pairs, close this as invalid or not worthwhile with the measurements. Also invalidate if the remaining cost is principally the one changing pane or a GPU/driver path. Treat the layout profile as a lead, not proof of the candidate remedy.

## Done when

Either invalidate with measured evidence, or implement explicit quiet-pane layout reuse that improves the declared fixture by at least two CPU points without stale global theme, anchor direction, focus/header state, appearance, resize, selection, caret, notes, hover or hit geometry. Enumerate every source of visual change before relying on notification-based caching; process-global atomics are not automatically GPUI dependencies. Compare normal production dependency graphs, not a cargo-test binary against a cargo-build baseline.
