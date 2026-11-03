# Gate 2 core: rungs on a line, and where each is enforced

Status: **APPROVED 2026-10-22**, all eight questions concurred on `reports/2026-10-22-who-may-prompt-whom.html`, with the amendments below. The rest of Gate 2 (where lines are kept, the fork launcher, how answers return) follows Gate 1.

## Parker's amendments (note map, 2026-10-22)

- **An ask is the authority to fork.** *"the orchestrator or manager will need to have authority to make a new process, maybe headless. And be able to fork either a manager or a worker agent … This means that the worker-agent permission relationship is irrelevant. It's only does this authority agent have the ability to make a new agent process from a fork of a given agent session."* An ask rung on a line means exactly that.
- **Cheapest first.** *"TDSpy could get the answer that is being looked for without actually querying the agent. So that workflow needs to have the cheapest option first."*
- **Fork cost.** *"that fork will potentially read a very large context window and immediately cost half million tokens, five million tokens"*. Measured the same day: the median live Claude pane carries 360,090 tokens (`evidence/`, "Cost").
- **Bypass lines grant three prompts by default**, not one: *"if one prompt does not get the required answer.. an entire other bypass line will need to be written up costing context tokens... Consider the default bypass prompt limit of 3"*.
- Figure 3 (who can take which tool away): *"I will take your advice on this front! … THis also simplifies the org chart somewhat."*
- **Named, not solved here:** a long-lived coordinating agent's own context grows for days and needs constant compression. Options he listed: handoffs, someone else's harness (which needs its own model access), or a harness of our own. Recorded in `01-product.md` under what waits.
## Where this came from

Parker's notes on the read-only brief, 2026-10-22, turned per-pane positions into relationships:

> the managing agents would ONLY SEE agents under their purvue.. .the lead orchestrator COULD REQUEST specific access to unlock read, write or partial write (restricted by MCP allowance if POSSIBLE - this is the real guts of it), but would do so only in special cases. … It is an ORG CHART.

> The basic use case is: orchestrator got the managing agent to get the worker agent to do the work. The ORCHESTRATING agent COULD bypass the managing agent to query (an actual prompt!) the worker agent, but would need special permission. They aren't assinging a task or prompt that hsould have the worker doing WRITE or ACCESS or web browser style tasks, so those tools would not be available for the SPECIIFC PROMPT from the lower privilege user.

> we need to figure out if it is possiuble in TD with MCP and actual prompts in a way that is secure and reliable and fleaves no room for prompt injectiony or poisoning behaviour.

His message with the notes: *"a bit of confirmation and refining around the tool restrictions for a tiered system first."* This document is that confirmation. The chart's own product page (seats, projects, the corner panel) and the program design follow once it is answered.

## The rungs

A **line** runs from one agent pane (the sender) to another (the recipient) and carries one rung. Rungs are ordered; each includes the ones below it.

| Rung | The sender gets | Runs in | Tools | Enforced by | Enters the recipient's conversation |
|---|---|---|---|---|---|
| See | the recipient in its census and target list | — | — | TD's server | no |
| Read | watch: scrollback, tool calls, documents | TD | TD's reading verbs | TD's server | no |
| Ask | a question answered | a fork of the recipient's conversation | reading files in the recipient's folder | the fork's tool list, plus Codex's sandbox | no |
| Scoped assign | work done with a named tool list | a fork, in its own worktree | the named list | the fork's tool list, plus Codex's sandbox | no; returns as a branch |
| Assign | work in the recipient's own conversation | the recipient | its own | TD's server decides who may send | yes |

## Rules

1. **No prompt without a line.** No agent may ask or assign to a pane without a line, an open pane included. A pane off the chart can be watched under its position and is never prompted by an agent.
2. **The pane's position caps every line into it.** Off-limits beats any line. Read-only caps a line at ask. The window's switches cap the pane, as concurred on the read-only brief.
3. **A pane on the chart is seen only along its lines.** A pane off the chart keeps the census behaviour of its position, so the colleagues-in-one-building default holds for ad-hoc panes.
4. **A bypass line lasts three prompts by default** (amended from one). An agent asks for a line on the rail (`request_access` gains a line kind), a person's press grants it, it is drawn dotted on the chart and lapses when its prompts are used.
5. **Unknown is none.** A line whose rung cannot be read grants nothing. A fork whose tool list cannot be read, or holds anything outside its grant, is not sent the prompt.
6. **Only a person draws a line.** The corner panel and the rail's Yes are the only setters, the same rule that holds for pane positions.

## Where a tool can be taken away

- **Words in the prompt** take nothing away. A header TD stamps on a request (sender, line, rung, expiry) tells the recipient and the person who is asking; any text the recipient reads can print the same header. The header informs. It is never checked as authority.
- **TD's server** refuses its own verbs (notes, bench, documents, `agent_send`) and decides who may prompt whom. It cannot remove the shell, the file editor, web search or another MCP server from a running agent: those belong to the agent's program.
- **The process a prompt runs in** holds a tool list fixed at launch. Nothing the model reads can add a tool. This is the lock for ask and scoped assign.
- **The OS around that process**: Codex's read-only sandbox blocks file writes and the network for shell commands and patches. Claude has no OS layer from TD until Restricted runs gain an online profile (the first profile is offline only; `docs/plans/process-authority/`).

## Measured, 2026-10-22

`evidence/2026-10-22-fork-probes.md`. Both programs forked a scratch conversation into a copy that remembered it, could not write, and left the original byte-identical. Claude needed one set of flags and reports its tool list before the question goes in. Codex needed four switches. Two that looked sufficient left a door open: the ChatGPT apps connector answered under `--ignore-user-config`, and web search ran under `tools.web_search=false`. Codex printed no tool list to check.

Consequences for the design:

- TD checks the list the fork reports, not the flags it passed. For Codex that read is not found yet. Until it is, a Codex fork runs only on a program version whose switches passed a probe like this one, re-run whenever the version changes (the agent-management adapters already pin versions the same way).
- Narrowing a turn inside a live conversation is rejected for now. Codex's `turn/start` sandbox override applies to *"this turn and subsequent turns"*. Claude's peer channel carries no per-message tool list. On both, the prompt's text stays in the conversation for every later turn, the person's own included.

## Relation to the agent-management draft

Its approved architecture says delivery goes to the already-running process and never falls back to start, resume or fork. That stands: **assign** is delivery. **Ask** and **scoped assign** are separate operations that always fork and never deliver. They share its receipts, request keys and reply cursor (`agent_messages`), and they never use its delivery adapters.

## What this does not promise

- An agent misusing the tools its rung gives it. Injected text cannot climb past a rung; it can steer what happens inside one.
- A manager repeating in its own words what an orchestrator told it. The chart limits each hop; it cannot read intent. A request records its parent when the sender names one.
- File secrecy between agents on one account. Every Standard agent can already read what the user can read. The chart guards who can direct an agent and lend out its tools, its servers and its standing approvals. Restricted runs are where file reach narrows.

## Saved layout

Per Parker's notes on old windows (*"do not write code that will generate backwards facing techinical debt"*; *"No change to 0.4.0"*): the pane position gains one key and nothing is written for older builds. An older build, including a launcher rollback to an earlier 0.5 build, reads a read-only pane as open. Lines get their own store, designed in Gate 3.

## Least-confident decisions

1. **Scoped assign as a fork in its own worktree.** It keeps the recipient's tree and conversation clean, at the cost of a branch somebody has to take. The named tool lists, and who keeps them, are open.
2. **Read-only caps a line at ask.** An ask reads the recipient's conversation through a copy. If that is more than "watch", read-only should cap at read.
3. **Claude without an OS layer.** The first build relies on Claude Code's own tool list, checked before every question.
4. **Fork cost.** Each ask re-reads the recipient's whole conversation. Not measured.
5. **Forking a live conversation.** Not measured on either program.
