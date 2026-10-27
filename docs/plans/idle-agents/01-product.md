# Product: idle agents stop costing memory

## Problem

> "Idle Agents take up a lot of memory. … idle agents that sit for more than, sometimes it's upwards of a week for me. … An idle agent, I think, soaks up something like 500 megabytes of memory. So it would be nice if we would be able to take those processes and somehow … make an idle agent process that sits for days become less expensive inside of Terminal Delight." — Parker, 2026-10-26

Measured the same afternoon, on his machine: six Claude agents had not been touched for four to seven days and still held 240–294 MB each with their helpers. Every idle Claude keeps waking up, about a hundredth of a core each, and pulls its own memory back out of swap as it does. Swap on this machine is compressed RAM, and it is full, so memory pushed to swap is cheaper, not free. A Codex window is small, but the helpers Codex starts for it live elsewhere and are never let go.

## Success metric

**Memory held by agents nobody has touched for a day: from about 1.6 GB of Claude agents today (six panes, measured 2026-10-26) to under 0.1 GB**, read off the per-pane memory figure this feature adds, which counts the agent, everything it started, and what it has in swap.

Two guard numbers that must hold while it does:

- **Waking a resting agent:** the first keystroke shows within half a second (measured 128 ms after a freeze; the swapped-out case is in the brief).
- **Waking a sleeping agent:** its prompt is back within three seconds (measured 1.4 s for a 20 MB conversation), on the same conversation, in the same pane, with nothing a person typed lost.
- **No agent ever rests or sleeps while it is working, asking, or running anything** — counted, every time it refuses, so a refusal can be seen and argued with.

## Announcement — the blog post before the feature

Terminal Delight now lets the agents you are not using rest. An agent you have not touched for a while stops waking up and gives its memory back, and the moment you look at it or type, it is exactly where you left it. Leave one for a day and it goes to sleep: the agent stops, its conversation stays, and its pane shows what it last said and when it fell asleep. Press a key and it picks the conversation back up in a second or two. Nothing that is working, asking you something, or running a job ever rests, and every pane now shows what it costs.

## Screens

Drawn in the Gate 1 brief, at the size they would be read:

- `mockups/states.html` — the same agent pane awake, resting, and asleep, including what a sleeping pane shows and how it wakes
- `mockups/memory-readout.html` — the per-pane memory figure, including the costs it cannot see

## Decisions — Gate 1 APPROVED 2026-10-26, with amendments

Parker's notes on the brief (`reports/2026-10-26-idle-agents-put-to-rest.html`), four concurs and four amendments:

1. **Build both, rest first.** ✓ concur.
2. **Rest after ten minutes untouched and quiet.** ✓ concur. Amendment: the settings live in a new **CONFIG › Agents** page with a ROBOT glyph, which gathers *every* agent setting made so far — the LAUNCH AGENT CONFIG section (harness, model, effort) and any other agent configs "floating around" — plus these new ones.
3. **Sleep after 2 hours of rest** (amended from "a day"). The threshold is a non-linear slider: sliding is perfectly smooth, but a click lands on a threshold through a large target — 20 min (click 13–29 min), 45 min (30–59), 90 min (60–120), 4 h (121 min–8 h), 24 h (above 8 h). Clicking hits the threshold; sliding allows specific values.
4. **A key or a click wakes a sleeper; focus wakes a resting pane.** ✓ concur.
5. **Codex's helpers are handled outside Terminal Delight** by the Codex sibling. ✓ concur.
6. **Release: in with 0.5.0** (amended from "after 0.5.0"). "Rebuild current build immediately when done — coordinate with MCP work!"
7. **Ship the per-pane memory figure as well, and do all the work.** Resting and asleep are **two new agent states**, and the agent summary at the top right (the robot counter: ▶ running, ‖ paused, ✕ failed, ✓ done, ○ idle, ? unknown) shows them.

Parker: *"Yeah - do all the work! <3"* — authorization to carry Gates 2–4 and the slices through without stopping at each gate; the gate documents are still written, as the record a fresh session resumes from.

## Decisions this gate asked for (as asked)


1. Which of the two states to build: **rest** (lossless, automatic), **sleep** (ends the agent, keeps the conversation), or both. Recommended: both, rest first.
2. When an agent rests: recommended, after ten minutes untouched and quiet.
3. When an agent sleeps: recommended, by hand from the first build, and on its own after a day untouched, with a per-pane "keep awake".
4. What to do about Codex's helpers, which no pane owns: recommended, a separate piece of work outside Terminal Delight (owned by the Codex sibling), with Terminal Delight only showing the cost.
5. Release: 0.5.0 says it adds no process termination or resume, so sleeping an agent is either after 0.5.0 or an exception Parker makes. Recommended: its own branch, after 0.5.0.
6. The per-pane memory figure ships with the first slice, because it is how the success metric is read.
