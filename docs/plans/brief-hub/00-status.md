# Status: one brief hub for the whole box

**Difficulty: 7/10.** It crosses Terminal Delight, the briefing skill and a long-running loopback service that writes into files, and a wrong shape (who owns the server, how a brief is registered) would be costly to unwind once every link depends on it. All four gates.

Asked by Parker, 2026-11-19, after browser notes on the CS329Z clip library needed a per-brief review server to save into the file: *"The desktop link when we have a glow pane open with an HTML doc should correctly open in the server that's generated. What happens if we have 20 runs of this skill? ... It seems like we would want to manage just one central server. So when Terminal Delight spins up, do we ... automatically open a server to manage all of this traffic? Let's figure this out."* And earlier the same day: *"That very popular tool that spins up a SERVER for the doc which we have looked at is a plausible architecture."*

Context already landed: agent-skills PR 97 (notes survive rebuilds, drafts save as typed) and PR 99 (review mode serves the files beside a brief).

- [x] Gate 1, product: `01-product.html` — **APPROVED 2026-11-19**: all four asks concurred (shape, every desktop route through the hub, the index page, retiring per-brief services), no notes; Parker: *"great work - simple elegant solution"*. Reading time: he read the asks and safety and scrolled past the background, so Gate 2 leads with its choices. It recommends one hub per machine owned by systemd and socket-activated on 127.0.0.1:8781, Terminal Delight as a client, registration only over a user-only Unix socket.
- [x] Gate 2, architecture: `02-architecture.html` — **APPROVED 2026-11-19** with one amendment: per-brief servers stop **1 hour** after the hub is installed, not 7 days (*"1hr no 7 day"*); the other three choices concurred. Two doors (private Unix socket to add a brief and get its address; public 127.0.0.1:8781 to read and save), four choices: hub in the briefing skill in Python, idle stop after 15 minutes, a 7-day grace for per-brief servers, and Terminal Delight falling back to the file with a stated reason
- [ ] Gates 3 and 4, combined: `03-plan.html` — waiting on Parker. Collapsed because Gate 1 came back as bare concurs and Gate 2 changed one number, and both times the reader read only the asks (the drawings and background were scrolled past or unseen). Three asks: change Terminal Delight inside `open_with_system` (27 callers in 7 files), install with `brief.py hub install` plus a dev-box-survival-kit line, and five slices with the tracer first.

**How the reader reads these pages:** in both gates the asks got all the visible time and every figure was scrolled past. Lead with the asks; keep background to what an ask needs.

Measured 2026-11-19: 3 `brief-review-*` units running at about 8.8 MB each, 4 remembered review ports. Terminal Delight hands a floating document to the desktop through `open_with_system()` (`app/src/pane.rs`), a file path to `xdg-open`.

Working in the worktree `.worktrees/brief-hub` on branch `plan/brief-hub`, because the main checkout carries another session's uncommitted work.
