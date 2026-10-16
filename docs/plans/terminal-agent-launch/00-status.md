# Terminal agent launcher

Difficulty: 4/10.

Combined plan: approved by Parker's concrete request for Alt+A, a per-pane header button, and exact reuse of the Workbench flow. See plan.md. Implemented. Both complete Rust suites pass (default 2,291; fallback 2,244; eight existing ignores each), along with fmt and clippy on both cores. Two added regressions exercise actual GPUI key/header gestures and modifier routing. HTML notes export/save and 1100/720/420px layout pass; narrative gate: argument, cold grasp 0.65. Source review and merge pending.

Actual difficulty: 4/10. Existing startup input guard required the pane test to use its settle helper before typing; production launch/seed/prompt routes were reused unchanged.
