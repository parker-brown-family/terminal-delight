# Agent Memory Hover

**Difficulty: 3/10** — reuse cached memory samples and an existing hover surface; preserve unavailable readings before showing the totals. No approval gates.

**Authorization:** Parker: “Great - get i t in! <3”. Target: dev/0.5.0 and the local daily window.

Hovering the top-right counter shows RAM and swap by awake, resting and asleep state, observed RAM decrease since rest, measurement coverage and sample age. It performs no process reads and leaves the click opening the War Room.

Unavailable measurements stay absent. Subtotals always carry their measured/eligible pane counts. The decrease compares each dormant pane with its own before-rest RAM reading; missing baselines after a window restart remain unmeasured. Shared services outside pane scopes are excluded.

- [x] Preserve independent RAM/swap readings and unavailable process lists.
- [x] Aggregate cached memory with coverage, including sleeping shells.
- [x] Add a themed hover and strings in all nine locales.
- [ ] Verify the hover interaction, both terminal cores, formatting and lint.
- [ ] Land one pull request into dev/0.5.0 and refresh the local window safely.

The measurement repair closes the [unavailable-memory accounting issue](https://github.com/parker-brown-family/terminal-delight/issues/1157). Shared Codex helper retention remains tracked separately in the [shared-helper issue](https://github.com/parker-brown-family/terminal-delight/issues/1155).

**Validation so far:** twelve memory-related tests passed, including empty versus unavailable process lists, independent missing swap, grouping, comparison coverage and binary units.

**Turned out to be:** pending.
