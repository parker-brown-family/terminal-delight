# Agent Memory Hover

**Difficulty: 3/10** — reuse cached memory samples and an existing hover surface; preserve unavailable readings before showing the totals. No approval gates.

**Authorization:** Parker: “Great - get i t in! <3”. Target: dev/0.5.0 and the local daily window.

Hovering the top-right counter shows RAM and swap by awake, resting and asleep state, observed RAM decrease since rest, measurement coverage and sample age. It performs no process reads and leaves the click opening the War Room.

Unavailable measurements stay absent. Subtotals always carry their measured/eligible pane counts. The decrease compares each dormant pane with its own before-rest RAM reading; missing baselines after a window restart remain unmeasured. Shared services outside pane scopes are excluded.

- [x] Preserve independent RAM/swap readings and unavailable process lists.
- [x] Aggregate cached memory with coverage, including sleeping shells.
- [x] Add a themed hover and strings in all nine locales.
- [x] Verify the hover interaction, both terminal cores, formatting and lint.
- [x] Open one pull request targeting dev/0.5.0 and validate the guarded GUI-replacement preflight.

Delivery is tracked in [the memory-hover pull request](https://github.com/parker-brown-family/terminal-delight/pull/1158). Its current status and activation receipt establish whether integration and the local refresh have finished.

The measurement repair closes the [unavailable-memory accounting issue](https://github.com/parker-brown-family/terminal-delight/issues/1157). Shared Codex helper retention remains tracked separately in the [shared-helper issue](https://github.com/parker-brown-family/terminal-delight/issues/1155).

**Validation:** Rio: 2,931 passed; Alacritty: 2,878 passed. Both retain 17 existing ignored tests. Formatting and both strict Clippy checks pass. The original unavailable-scope reproduction now passes. The native GPUI test opens the real hover, updates an open card from cached data, retains the resting state and War Room click, and checks the card fits a 968px window. The GUI-replacement preflight preserves the host, all 32 panes, the saved split document, policy and geometry; no replacement was applied in the preflight.

**Preview:** [the reconstruction](preview.png) shows the layout with illustrative values. It is not a desktop pixel capture.

**Turned out to be:** 3/10 held. The existing sampler and hover builder supplied the mechanism; the main work was keeping unavailable readings and partial comparison coverage intact.

## Agents Toolbar Shortcut

**Difficulty: 2/10** — add the missing Agents settings shortcut through the existing UI-choice mechanism. No gates. Parker identified its omission from MENU GLYPHS on October 27.

The 🤖 toggle controls a direct Agents settings button in the bottom toolbar. It is independent of the top-right counter. Standard and All show it; Clean folds it into CONFIG. The CONFIG entry remains available either way, and both buttons use the same settings-opening function. Hand choices persist as `ui.outer.agents`.

[Native capture](toolbar-native.png): an isolated 1200×900 logical-pixel window using Ops and Hacker, showing the new MENU GLYPHS chip and matching toolbar shortcut. The private capture window explicitly opted into scripted operator control; the daily window's policy was unchanged. The GPUI interaction test clicks the real chip, hides and restores the shortcut, checks serialization and counter independence, and opens settings through both doors.

**Turned out to be:** 2/10 held; the existing UI set, persistence and menu patterns supplied the change. The pull request records final checks and local activation.

