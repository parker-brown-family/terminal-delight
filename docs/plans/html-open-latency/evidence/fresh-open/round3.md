**Two findings in `HEAD~3..HEAD`:**

1. **P2 — CONFIRMED control flow; image corruption is INFERRED.** `app/src/docview/snapshot.rs:1312`: when a document is shorter than the first band, `open()` drops its pending early screenshot instead of storing it. The fused path in `page.rs:843–847` then requests the shorter first tile, and `capture_band()` finds no pending shot to wait for. Two captures can run on the same page at once—the overlap the latest commit says caused intermittent test failures. The new wait covers a pending shot stored in `page.first`, but not this short-page path.

2. **P3 — INFERRED.** `app/src/docview/page.rs:423–430`: if spawning the `Unadopted` cleanup thread fails, the error is discarded and the unadopted page stays open until idle shutdown. This leaves a resource-exhaustion path around the new ownership guarantee.

The guard is disarmed only after `adopt()` gives the page to the view; I found no ordinary path where it closes an adopted page or closes one twice. I found no lock-order deadlock, stale ready page after browser shutdown, cross-document context reuse, network-block regression, or new FileChanged, hidden-view, decode, or scroll-target failure in this diff.

This was a static, read-only review; I did not run the tests. APES and lean-ctx calls were rejected by the session’s approval policy, and the GitHub wrapper had no token to check the PR’s live base branch.