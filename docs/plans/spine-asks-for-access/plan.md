# An agent asks, and a person answers yes or no

**Difficulty: 6/10.** The pieces exist (the rail, the MCP write pipeline, the line
the bench types into an agent's terminal), but this is the line between what an
agent may do and what only a person may do, and a hole in it is silent: an agent
that could answer its own ask would look exactly like one that was told yes.

## Where this came from

Parker, on the off-limits figure of the Splinterm scuttle brief (2026-10-15):

> Seems like a popup prompt to say: THIS AGENT IS TRYING TO ACCESS THAT PANE would be useful... thoughts?
>
> We get this in as fallback security behaviour... then establish new features around it like a prompt popup (maybe on the agent spine to the right).
>
> Actualy - decision made - the agent spine will now allow requests to show up with a SIMPEL YES NO if they are trying to modify TERMINAL DELIGHT SETTING THAt require human provenance (agent behaviour and access)... but are not blocked by human decision for simple things like themeing etc.

So the product decision is his and made. Off-limits stays as the fallback. What
follows is the design, and the parts marked **mine** are the ones to argue with.

## What an agent can ask for

Five changes, and only these. Each is a setting an agent can already be refused
by, and none of them is appearance:

| Ask (`change`) | What Yes does | Today, who can do it |
|---|---|---|
| `open_pane` + `pid` | lifts off-limits from that pane | the person, right-click menu |
| `close_pane` + `pid` | marks that pane off-limits | the person, right-click menu |
| `writes_on` | turns MCP writes on | the person, MCP panel |
| `expose_shells` | shows plain shells to agents, not only agents | the person, MCP panel |
| `events_on` | lets agents tail other agents' transcripts | the person, MCP panel |

**Mine:** the narrowing twins of the last three (`writes_off` and so on) are left
out. An agent that wants less access can simply not use it, and every extra verb
is one more thing on the rail. `close_pane` stays because an agent that sees a
pane full of credentials should be able to say so.

Appearance (`set_pane_config`: brightness, warp, colour, the CRT switch) asks
nobody. That is Parker's "not blocked by human decision for simple things like
theming", and it is how it already works.

## The flow

1. The agent calls `request_access {change, pid?, why}`. It works with writes off,
   because `writes_on` is one of the things it may need to ask for. It needs the
   MCP server on and a pane to be calling from.
2. The ask is stored on the **asking** pane, one at a time. A second ask from the
   same pane replaces the first, so one agent cannot fill the rail. The ask dies
   with its pane.
3. The rail shows that pane's row in a new first lane, **ACCESS**, with what is
   being asked, the agent's reason in quotation marks (its words, not ours), and
   two buttons: **Yes** and **No**. `y` and `n` answer the row under the cursor.
4. Yes applies the change exactly as the person's own control would, and saves.
   No changes nothing. Either way the answer is typed into the asking agent's
   terminal as one tagged line, through the same held-until-visible path the
   bench uses: `[workbench:<tag>] request_access open_pane 4412: yes, pane 4412 is
   open to agents`.
5. Nothing answers on a timer. An unanswered ask is a row waiting, like any other.

## What cannot answer an ask

- **No MCP verb.** The ask rides `ConfigPatch` as a `#[serde(skip)]` field, so no
  agent JSON can carry one in, and there is no field or verb that carries an answer.
- **No `ctl` verb.** `ctl bench click|hover` drives a pane's bench through that
  pane's own handlers and cannot reach the rail. A test pins that neither `ctl.rs`
  nor `mcp.rs` can name the answering function.
- **The agent's own terminal.** The buttons are on the rail, not in the pane.

**What it does not stop, said in the PR:** an agent with a shell runs as you. It
can edit TD's settings file, or inject input through the compositor. This gate
is the front door, the one TD offers and labels. Splinterm draws the same line.

## Slices

1. **The ask, end to end (tracer).** `request_access` verb with its validation;
   `Ask` stored on the asker; the ACCESS lane in `attention.rs`; the row with Yes
   and No; Yes and No applied; the answer line typed. Tests at each layer, and a
   mutation per guard.
2. **Docs and the refusal text.** `docs/mcp.md`, `docs/attention-spine.md`, the
   docsite MCP page; the "writes are off" refusals name `request_access`.
3. **Land.** One PR, CI green, photograph the row in a hidden window.

## Out of scope

- Granting one agent access to one pane without changing the pane for everyone.
  A finer grant is a different model (who, not what) and wants its own decision.
- Asks that survive a window restart.
