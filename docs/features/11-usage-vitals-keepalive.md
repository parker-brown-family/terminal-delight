# Usage, vitals, and the keepalive that was withdrawn

The usage card and the agent wall's three bars are described on the docs site,
where each page was checked against the code and names the commit it is true of:

- Usage and vitals: [https://docs.terminal-delight.brownfamilysports.com/usage](https://docs.terminal-delight.brownfamilysports.com/usage) (source: [`docsite/pages/usage.html`](../../docsite/pages/usage.html))
- The agent wall's bars and the call: [https://docs.terminal-delight.brownfamilysports.com/agent-wall](https://docs.terminal-delight.brownfamilysports.com/agent-wall) (source: [`docsite/pages/agent-wall.html`](../../docsite/pages/agent-wall.html))

What stays here is the record of a feature that was removed, and why.

## Keepalive — removed, and the measurement that outlived it

| Feature | What it does | Evidence | Status |
|---|---|---|---|
| **Cache keepalive** | Typed into an idle agent's prompt before its prompt cache expired, keeping the session warm | removed in #317 | **Removed** |
| **AWAY** | An AWAY campfire in the menu bar marked you gone, so keepalive could tell idle from absent | removed with it | **Removed** |
| **The measurement** | Across 32,037 billed turns in thirty days on this machine: 87 cold returns, 24.2M tokens re-written, average gap 5.0 hours — about **$242, of which roughly $230 buys nothing** | the removed module's doc | measured here |

Terminal Delight no longer types into a terminal on its own initiative. The saving
was real and the mechanism was not defensible: a terminal that sends bytes nobody
asked for is a terminal you cannot trust with a production shell, and no cache
rebate buys that back. The number stays on the page because it is the honest
statement of what idle agents cost — a fact about the industry rather than a
feature we sell.

## Honesty note

The keepalive figures are one machine's thirty days, not a benchmark. Quote them as
exactly that — a measurement taken here — wherever they appear in public material,
and say the feature was withdrawn.
