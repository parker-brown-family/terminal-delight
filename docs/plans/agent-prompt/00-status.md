# Prompt — a managing agent prompts its workers through Terminal Delight

**Difficulty: 8/10.** This writes into another agent's conversation while nobody is watching. That is the failure the bench's write rule exists to refuse, so every decision below is about keeping the authority named, bounded and visible. Gates were not run with approvals: Parker asked for the whole thing while away (below), so the decisions are written here before the code and recorded as **decided under his AFK instruction**, for him to overrule.

## The ask

Parker, 2026-10-27, after the coordinator in cinema-delight could not drive two Codex workers along a "may prompt" line (issue 1169):

> SO we need to consider a rework ---- PROMPT --- the coordinating or PROMPTING agent NEEDS TO BE ABLE TO PROMPT THE AGENT (via MCP ideally) INSIDE AND THROUGH terminal delight... ASK is a different channel I ugess? … these are now feeling like SEAPARATE levels and actions. The POINT of a managing agent is to TREAT OTHER AGENTS AS SUB AGENTS!!!

> AFK so if you can sort all of that out - get it into 0.5.0 -0-- BE VERY THOROUGH in your build AND TESTING TESTING TESTING manual, and automated … unblock yourself by doing research on standards and our own project philosophiues and GOOD SOLID ENGINEERING

## Decisions

1. **Prompt is its own rung, above Ask:** See < Read < Ask < Prompt. Ask stays a channel that does not touch the worker: a cheap answer, or a read-only copy. Prompt enters the worker's own conversation as a turn, using the worker's own tools. The saved word is `prompt`. A build from before this reads it as an unknown word that grants nothing, so a rollback fails closed.
2. **No existing line is upgraded.** Ask lines were drawn under a label that read "may prompt". That label now says "may ask", and a person changes a line to Prompt in the line editor. Authority never widens without a person's act.
3. **Only a person grants Prompt**, the same way every rung is granted: on the Threshing Floor, seat to seat or to a whole org. An agent cannot request a Prompt line. `request_access` for a line stays at ask.
4. **A read-only or off-limits pane is never prompted.** A read-only pane's ceiling stays Ask.
5. **Delivery is typing into the worker's line editor**: one bracketed paste and a return, or a flat line when the terminal has not turned bracketed paste on. The encoder is the one the bench already uses. Any harness with a line editor (Claude Code, Codex, pi) takes it, and there is no shared Codex service (R4 of issue 1169).
6. **The text says who sent it.** It opens with one line naming the sending pane and the Threshing Floor line, so the worker knows a peer agent is speaking, not the person.
7. **It lands only at rest.** The pane must run an agent and its screen must read Idle or Finished, with no picker up:
   - **Working:** the prompt waits and goes in when the agent next rests.
   - **Blocked** (a permission question), Error, or Unknown: it is never typed. It waits, and fails after the deadline.
   - **Resting or Asleep:** the window wakes the agent the way a person's key would, and delivers once it rests.
   - **Deadline:** ten minutes, then the prompt fails, by name.
8. **Off screen is allowed for Prompt only.** The person-drawn line is the named authority that the on-screen rule stands in for. Every delivery is visible afterwards: the worker's pane shows who prompted it, and the record keeps it.
9. **The brief carries no control characters.** ESC and the other C0 and C1 controls are removed, except newline and tab, so a brief cannot close the paste early or type keystrokes. At most 32,000 characters.
10. **The sender learns the turn** through `prompt_status`:
    - the stages are waiting → delivered → working → finished, or failed;
    - the final reply comes back only when the worker's conversation is proven (not `Bond::Guess`); otherwise it is "unknown";
    - `prompt_agent` takes an idempotency `key`, so a retried call never types twice.
11. **One prompt in flight per sender and worker.** A second prompt while the first is unfinished is refused as busy.
12. **Record.** Every stage goes to the window's ask record (`threshing-floor/<session>/asks.jsonl`) with `via = "prompt"`, the brief's length and SHA-256, and never its words.

## Done when

The acceptance run from issue 1169: a Claude coordinator and two Codex workers, each with a Prompt line from the coordinator.
1. With the worker's tab off screen, the coordinator prompts a worker by pid with a 3 KB brief.
2. The worker's TUI holds the brief as a turn, under TD's header.
3. The coordinator reads working, then finished, and the worker's final reply.
4. Once the line is erased, the same call is refused by name.
5. A `--last` worker's card says "conversation unknown" until the worker is proven.

## What building it changed

Each change below came from the code or from a rig run, and each is held by a test.

- **No flat-line fallback (decision 5).** A worker whose prompt has not turned bracketed paste on gets nothing, the same rule ↪ follows (`docs/security/terminal-input.md`, row #9): typed flat, every newline in a brief would be an Enter. Claude Code and Codex both turn it on.
- **Rest is the screen's idle furniture, read before the bell (decision 7).** `Finished` is the bell promoting a resting screen *or one nobody could read*, so it is not evidence of a prompt. `TerminalView::prompt_seen` reads the parse alone.
- **Enter waits for proof.** After the paste, Enter is pressed only when a row that changed on the worker's screen carries the prompt's own number (`Threshing Floor prompt <id>,`, comma-terminated so 17 never matches 171) or the program's long-paste placeholder. A dialog does not echo a paste, which is the answer to the keepalive failure in the security doc. No proof in four seconds: no Enter, and the prompt fails saying where its words may be.
- **The reply is proven by the transcript, not the bond (decision 10).** The worker's last message is read from whichever conversation its pane resolves to, and only a message *after the line carrying this prompt's number* counts. A wrong guess finds no number and says "unknown"; it can never return another agent's words.
- **The reply is looked for again for five seconds.** Rig run 1: both short prompts finished with "no message after this prompt" while the transcript held the answer, written about half a second after the screen came to rest.
- **A turn between two looks still counts.** Rig run 2: Claude's three-second turn was never sampled as working, and the prompt failed at the two-minute take-up limit. The pane's own 120 ms scan now stamps when a turn begins (`turn_began_ms`), and a worker back at rest that was never seen working has its transcript probed every three seconds.
- **`wait_seconds` is at most 6.** Thirty was the first choice. The `terminal-delight mcp` relay gives up on any answer after ten seconds and exits, which costs the calling agent its whole TD tool set for the session; run 2's coordinator died that way.
- **A woken agent keeps its seat and its mark.** Waking an asleep agent, and the replica repair, build a fresh pane that carried neither the seat nor the read-only / off-limits mark, so a woken worker fell off every line and an off-limits pane came back open. Both now travel in `Presentation`. A woken worker's fresh pane runs a shell for a moment before the agent restarts; for 90 seconds after a wake that reads as waking, not gone.
- **The ask rung reads "ask" again.** It had been labelled "prompt" when a copy fork was the strongest thing a line allowed.
- **Rest must settle for two seconds** (five since Parker's answers, below). Rig run 4: while Claude Code streamed a 200-word story, its screen read at rest between chunks. A prompt was called finished mid-stream, and a second sender's paste went into a working Claude, which correctly withheld Enter. Rest now means at-rest looks for an unbroken two seconds with no turn beginning inside that window, both before a paste and before a prompt is called finished.
- **An unsent paste holds the worker** (taken out since, below). In the same run, the second sender's words stayed in Claude's composer, and the next prompt's paste joined them, so Claude answered both. A worker left holding a paste now takes no more prompts until its person types in the pane or a turn begins there.

## Where it stands (2026-10-27)

Built, and every local gate passes: `cargo fmt --check`; clippy `-D warnings` on both cores; 2865 tests on the default core and 2812 on the fallback core; the docsite build; and the release smoke. The prompt rules carry 26 unit tests in `prompt.rs`, three window tests through a real pseudoterminal, and protocol tests for both verbs. Ten mutations of the safety rules were each caught by a named test.

Against the acceptance run above:

1. **Done, by a real Claude coordinator** (rig 6, `evidence/rig6-claude-coordinator.png`). Given only an instruction, it called `list_panes`, `prompt_agent` and `prompt_status` and reported the Codex worker's `COORDINATED-OK`. Rigs 1–5 drove the worker tabs off screen with a 2.4 KB, 40-line brief.
2. **Done.** The worker's TUI holds the brief as a turn under the header (rig 2, transcribed in the brief, Fig 3).
3. **Done.** Working, then finished, then the reply, for Claude and Codex, short and long, including a queued second sender and a resting worker woken for the prompt (rig 5).
4. **Done in the window tests:** an erased line ends a waiting prompt with nothing typed. Refusal by name without a prompt line is covered at the rule, protocol and window levels.
5. **Shipped earlier**, in the change that shows an unproven conversation as unknown.

**Not verified live:** prompting an *asleep* worker. In rig 5, the sleep ended Claude but never marked its pane asleep, so the prompt was refused by name as "not an agent". That refusal is the correct answer for an unmarked shell. The defect is in the sleep, issue #1174. The wake-for-prompt path (`wake_sleeper_as`, the seat carry, the 90-second waking grace) is covered by unit tests and code reading only.

The brief for Parker: `reports/2026-10-27-agent-prompt.html` in the main checkout.

## Parker's answers, and what they changed (2026-10-27, evening)

He read the brief and answered its six decisions in notes and a reply.

1. **The window presses Enter: kept.** *"old security of terminal delight never pressing enter on its own did not consider using a real terminal as a sub agent ... the POINT is that it moves while we are away... this permissiion is enabled FROM the threshing floor."* Recorded in the security doc's section on prompts.
2. **Prompts reach workers that are not on screen:** concur.
3. **Settled rest: slower.** *"slower and safer is basically better ... SPEED is NOT the priority at all."* Rest now settles for five seconds, a person's key holds a pane for a minute, and Enter comes a second after the paste. The same note asked that a coordinator iterate toward good results and escalate to its person *"if something isn't being done right on 2-3 attempts"* or when token burn grows; that is guidance in the briefing and in `prompt_agent`'s description, because only the coordinator can judge a reply.
4. **The unsent-paste hold: removed.** *"This feels overthunk and over engineered... pasting the next prompt on top feels fine to me."* The next prompt is pasted after leftover words, and its Enter sends both.
5. **No Writes switch for prompting: kept, and stated more strongly.** Prompting and styling are unrelated; prompt access is *"MUCH MUCH MUCH MUCH MUCH more delicate and careful and foundational"*; agents prompt each other along Threshing Floor relationships *"AND ABSOLUTELY IN NO WAY"* without one. An agent asking for a relationship with a justified reason is out of scope; `request_access` asks for an ask line only.
6. **Old lines stay ask lines:** concur.

**New with these answers:** an agent Terminal Delight launches (alt+a, the launcher, Codex's first prompt, and a typed Claude through the briefing hook) is told about the Floor in its briefing (`surface::FLOOR_PARAGRAPH`), and `floor_position` reports where an agent stands, both directions. Parker's note: *"when TERMINAL DELIGHT spins up an agent (alt+a), we want that agent to be made aware of its threshing floor state ... AND IF IT DOESN'T EXIST YET... that the threshing floor is a thing and it should know HOW OT INTERACT AND REPORT effectively using this MCP!"* The hand-off guidance follows his: *"artifacts, specs, and careful instruction work best! clear acceptance criteria, known unkonwns ... context around the TRACER"*, planning sized by difficulty, and adversarial review by *"an agent from another provider (ie. claude builds and codex reviews)"*.
