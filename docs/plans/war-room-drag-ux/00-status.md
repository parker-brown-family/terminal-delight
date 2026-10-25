# War Room Drag UX

**Difficulty: 6/10** — membership gestures change access as well as layout; errors need native interaction and permission tests.
**Turned out to be:** 6/10. Native event tests caught the preview-root positioning mistake; removal also needed to invalidate undelivered copy results.

The combined plan is authorised by Parker's 2026-10-25 instruction: "I am AFK so you have the full reigns to take a bit of risk. we can always roll back!" Continue through implementation and verification without intermediate approval prompts.

- [x] Read the current implementation, release context and existing follow-ups.
- [x] Research drag feedback, cancellation, pointer alternatives and menu dismissal.
- [x] Dismiss menus on outside press and bind War Room menus to the intended card.
- [x] Connect pool and bin removal to the same access-revoking operation.
- [x] Highlight destinations, dim and frost unrelated surfaces, and support cancellation.
- [x] Run native interaction tests, permission/persistence checks and project gates.
- [x] Deliver illustrated review and install the qualified development build for the next launch.

Worktree: `/home/parker/Work/td-authority-lab/war-room-drag-ux`, branch `fix/war-room-drag-ux`, based on `origin/dev/0.5.0` at cd914c00. Keep unrelated working trees untouched. Intended release: 0.5.0, milestone 2; integration base: `dev/0.5.0`.

Final code checks: Rio 2,846 passed / 17 ignored; Alacritty 2,793 passed / 17 ignored. Clippy with `-D warnings` passed for both cores; formatting and release build passed. Native screenshots cover pickup, hover, pool/bin removal and outside-press menu dismissal. The final preview-edge adjustment is verified by GPUI bounds assertions; captures precede that adjustment.

The review page has three figures and the source-to-behaviour map. HTML structure checked (unique IDs, embedded notes, no external assets); the browser tool rejected its local file URL, so responsive/browser annotation interaction remains unverified. Narrative gate after reordering: journey, opening 0.64, headings 0.63. These are editorial signals, not a correctness test.

## Local installation

Code commit: 429bf789. Installed candidate: `~/.local/lib/terminal-delight/td-429bf789-0.5-war-room-drag`, SHA-256 `19a6fc9b64b918b9496fd7b41abf845a9a4f8470471561a7d6648fd187fb6f30`. The launcher points at it. The running session-1 window remains on `td-c12e07b6-0.5-artifact-continuity`: the existing swap guard found five recoverable live panes outside the saved layout (8, 69, 93, 95, 100) and refused the restart. No live pane or host was restarted. Do not bypass the guard; restore or resolve held tabs before a live swap.

PR: https://github.com/parker-brown-family/terminal-delight/pull/1143. Deployment receipt is in `reports/2026-10-25-war-room-drag-evidence/installation.json`; full local guard output stays under `/home/parker/Work/td-authority-lab/deploy-dev-429bf789/`.
