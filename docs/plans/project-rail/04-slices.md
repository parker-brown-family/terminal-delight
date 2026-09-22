# Slices: the project rail

Built in order, each ending in a state that runs and was photographed on the
rig. Parker's brief for the night: *"use a tracer round to hello world the
simplest possible build, then morph that hello world into more and more
complex pieces, then MVP, then full implementation."*

1. **Tracer bullet — done, `0fbab3d`.** `engstate` model + scan + badge +
   frames; the corner names the project and carries the badge; the middle
   shows one rotating frame and the heartbeat; the strip returns with the
   tree shut. Photographed: `rail-01-tracer.png`, `rail-02a…d.png`,
   `rail-03-shut.png`. Real numbers on the real worktrees: 5 checkouts,
   3 repositories, 2 foreign, 1 visitor, 989ms a scan.
2. **Morph — the pieces that make it an instrument.** Warn segments get a
   wash so `SHARED` reads as a warning on every palette; the heartbeat
   frame stops repeating the bars in text; **afterglow** — a pure diff of
   two readings yields the events that happened between scans (a commit
   landed on a line, a checkout went dirty or clean, main moved on, a pane
   wandered in or out) kept for fifteen minutes as a frame and a dot on the
   badge, so that *something happened while you were elsewhere* is evidence
   rather than a notification.
3. **MVP.** Click the badge for the checkouts table — one row per checkout:
   line, WT or SHARED, dirty and ±, ↑↓ against main, who is writing there —
   plus the foreign, visitor and no-git rows; guards for the wiring; the
   help modal mentions the rail; the branch pushed and a pull request
   opened.
4. **Landing — the teeth.** Parker, mid-build: *"The real value we would
   gain from this personally tomorrow is when we have 3 or 10 worktrees all
   in hot agentic dev. ALL OF THAT HAS TO GET MERGED IN — unstaged commits,
   etc. What will a coordinating agent have to make true in order to
   successfully land all the work that is in flight? … having this ticker,
   if we add some teeth behind the scenes, can give that agent something
   TANGIBLE to go to — it might get them 90% of the way there without any
   significant inference."* So the reading grows the facts a landing needs,
   per checkout: commits **unpushed** (ahead of its upstream), **stashes**,
   the **files it touches** against main (committed and uncommitted), and
   whether it **would merge cleanly** into main today — a `git merge-tree
   --write-tree` dry run, which names the conflicting files without touching
   a working tree. **Idle worktrees** on disk that no pane is in are measured
   too, because that is where lost work hides. From those, two derived
   things: **collisions** — pairs of lines that touch the same files, the
   "◇ converging" signal before there is a conflict — and the **landing
   list**: the ordered set of things that must become true for everything in
   flight to be on main, each with the checkout it belongs to.
5. **Exposure.** The reading and the landing list over MCP (`engineering_state`)
   and `ctl rail`, as JSON, so a coordinating agent gets the whole picture in
   one call and spends its inference on the merge, not the census.
6. **Pull requests.** `gh` on a five-minute cadence per repository,
   `unavailable` on failure and never zero; the branches frame marks a line
   that has a PR; the landing list knows which merges are already asked for.
7. **Stretch.** Hold the project name to overlay the inferred topology for
   three seconds.

Not in any slice, on purpose: localising the frames (they are derived
English sentences; the help modal's nine languages are a different kind of
string), an MCP read of the engineering state (the model is shaped for it),
and any writing back — the rail moves nothing. *"TD should never silently
decide that one overrides the other."*
