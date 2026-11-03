# MCP communication security battery

Difficulty: 7/10. Permission can change between admission, execution and delivery; a stale result can disclose another organisation's work without a visible failure.

Parker authorised autonomous overnight testing, fixes and durable automated evidence on 2026-10-25. This continues the Threshing Floor work on `dev/0.5.0`, from `cba36a6f8ad94bc9617888ec6d4635fc237bfbe7`. The four design gates below are recorded before implementation; his overnight authorisation covers proceeding without interactive approvals.

## Progress

- [x] Product, architecture, program design and slice decisions recorded.
- [x] Separate worktree; daily panes and dirty primary checkout left intact.
- [x] Reproduce stale communication across organisation changes.
- [x] Add denial and positive-control regressions; fix confirmed defects.
- [x] Exercise real MCP transport against disposable processes (45 assertions passed).
- [x] Run both terminal-core suites and required checks (Rio 2,852; Alacritty 2,799; five OS tests; strict Clippy and formatting).
- [x] Record evidence and remaining limits in the HTML review and its evidence directory.
- [x] Submit one integration PR: https://github.com/parker-brown-family/terminal-delight/pull/1144 (dev/0.5.0, milestone 0.5.0). Live CI and merge status are recorded there.

## Surprises and discoveries

Five defects were reproduced and repaired. Moving a member deleted relationship lines but retained pending copies and approval cards. Stored answers were delivered after relationship removal. The effect-time write check still accepted a captured own-pane boolean even when the live sender was another pane. An explicit negative PID was parsed as absence and redirected a note to the caller; the real native run caught this and a unit regression now covers null, string, fractional and oversized values too. The final queue audit found that grep carried no caller into its UI request and could read scrollback after relationship revocation; it now checks current Read permission before touching the grid, while retaining the admission snapshot as an additional result filter.

The controlled relationship-revocation write interleaving itself already passed. Its own-pane exemption was the failing branch. Request-access patches now carry the verified caller as well, preserving permission proposals through the corrected gate.

Two simultaneous native windows passed six additional manual assertions: forged session/pane environment values did not redirect either caller, a forged foreign-window pane claim did not allow a write, and each receiving note remained intact. Both windows and all twelve mock agents were stopped and their temporary saved layouts removed.

The first full suite identified one expected assertion update: request_access's patch now includes caller identity. The test now checks that identity explicitly and still proves no appearance setting rides the proposal. The final Rio suite passes 2,852 tests and Alacritty passes 2,799. Five real OS isolation tests passed separately. Both strict Clippy checks, formatting and diff checks pass. Both normal suites have 17 ignored tests, including the five separately exercised OS tests. All final native probes used binary SHA-256 aa85f6453ee386a459af1a07384387d9328a91d74f6efbcf57a5decc22b34a01.

## Decisions

Tests use synthetic canaries and owned processes. No real agent transcript, production permission or live message is needed to establish these boundaries. Successful direct MCP probes and native UI tests are distinct evidence; neither substitutes for the other.

## Outcomes

The battery establishes the enumerated cases, not universal security or sustained release qualification. The draft native-handoff feature is outside this branch and is not silently included. Real provider calls and Codex helper-agent inheritance remain untested here; the existing follow-ups remain open.

Copy checks run before launch, completion retention and collection. A consumed bypass is a receipt for its admitted question; it is not spent twice. Explicit relationship changes erase those receipts, and a membership change clears both directions. A running copy may finish after revocation, but its answer cannot be reinserted or delivered.

Post-hoc difficulty: 7/10. The stale-own flag and malformed explicit target were found by independent behavioral checks rather than source-shape assertions. Their normal paths had looked correct in the earlier suite. A final audit of queued reads then exposed the same admission-versus-effect mistake in grep.
