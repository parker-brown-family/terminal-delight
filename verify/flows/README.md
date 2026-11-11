# User flows, as checked models

The flows a person lives in most, each a small TLA+ model with the code it stands for named in its header, and a `*-view.ts` that gives the pstack `/statechart` skill the reader's words.

- `Inbox.tla`: one agent through the INBOX shelves (`app/src/inbox.rs`, `shelve`, `Store::advance`). Holds: a question asked after you put a row away comes back to LOOK AT ME; a question is never on the quiet shelf.
- `SettingsBook.tla`: the Settings book (`app/src/settings_view.rs`). Holds: a panel shown in place never outlives the book; Esc goes back one level.

Run: `verify/check.sh verify/flows/Inbox.tla` and `verify/check.sh verify/flows/SettingsBook.tla`. Both rules of each were broken on purpose with `verify/mutate.sh` and caught.


## Three everyday flows (2026-11-11)

- `Launch.tla`: launching an agent from the composer (`entry_view.rs` `composer_send`, `open_composer`). Holds: a refusal says why and keeps what was written; Enter takes you to the agent's tab; Ctrl+Enter leaves you on an empty composer that says it started.
- `PromptAcross.tla`: one agent prompting another along a line drawn at `prompt` in the Threshing Floor (`prompt.rs`, `mcp.rs` `prompt_agent` and `prompt_status`). Holds, for two prompts to one worker: typed only while the worker is at rest; Enter only on proof the paste landed, and once; one prompt typed at a time, in send order; no line, nothing typed.
- `OpenBeside.tla`: an agent opening its document beside itself (`mcp.rs` `open_document`, `pane/doc.rs` `open_here`). Holds: an agent never replaces a document a person opened (it is held, onto the artifact rail); what was held is offered on the rail.

Run each with `verify/check.sh verify/flows/<Model>.tla`. Every rule was broken on purpose with `verify/mutate.sh` and read `BREAKS`: dropping the rest guard, the send-order guard or the paste-proof guard in `PromptAcross`; opening over any square in `OpenBeside`; a refusal with no note in `Launch`. The charts below were drawn by the pstack `/statechart` skill from each model with its `*-view.ts`.

### Launching an agent

```mermaid
---
title: "Launching an agent from the composer"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: Away
    lg1: Composing
    lg2: Refused
  }
  class lg0 teal
  class lg1 yellow
  class lg2 tan
  s0: Composer put away
  s1: Composer up, nothing written
  s2: Composer up, saying why it did not start
  s3: Composer up, ready to send
  s4: In the agent's new tab
  s5: Composer up and empty: it started behind
  s6: Composer up, a choice it will refuse
  [*] --> s0
  s0 --> s1: you open the composer
  s1 --> s0: you put it away
  s1 --> s2: you send: refused
  s1 --> s3: you write
  s1 --> s6: you write
  s2 --> s3: you write or you change a chip
  s2 --> s0: you put it away
  s2 --> s1: you change a chip
  s3 --> s4: you press Enter
  s3 --> s0: you put it away
  s3 --> s5: you press Ctrl+Enter
  s3 --> s6: you change a chip
  s4 --> s1: you open the composer
  s0 --> s3: you open the composer
  s5 --> s2: you send: refused
  s5 --> s3: you write
  s5 --> s0: you put it away
  s5 --> s1: you change a chip
  s2 --> s6: you write or you change a chip
  s6 --> s3: you change a chip
  s6 --> s0: you put it away
  s6 --> s2: you send: refused
  s0 --> s6: you open the composer
  class s0 teal
  class s1 yellow
  class s2 tan
  class s3 yellow
  class s4 teal
  class s5 yellow
  class s6 yellow
```

### Prompting across the Floor (the first of two prompts)

```mermaid
---
title: "One agent prompting another across the Floor (first prompt)"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: Moving
    lg1: Done
    lg2: Stopped
  }
  class lg0 yellow
  class lg1 teal
  class lg2 tan
  s0: Not sent
  s1: Refused: no line at prompt
  s2: Waiting for the worker to rest
  s3: Pasted, Enter not pressed
  s4: Failed, and says where its text may be
  s5: Entered, not yet seen working
  s6: The worker is working on it
  s7: Finished
  [*] --> s0
  s0 --> s1: the agent calls prompt_agent
  s0 --> s2: the agent calls prompt_agent
  s2 --> s3: the worker rests: it is pasted
  s2 --> s4: the line is erased
  s3 --> s4: no proof the paste landed
  s3 --> s5: Enter, once
  s5 --> s6: the worker starts
  s6 --> s7: the worker rests again
  class s0 yellow
  class s1 tan
  class s2 yellow
  class s3 yellow
  class s4 tan
  class s5 yellow
  class s6 yellow
  class s7 teal
```

### An agent opening its document

```mermaid
---
title: "An agent opening its document beside itself"
---
stateDiagram-v2
  state "Legend" as legend {
    lg0: Clear
    lg1: Yours
    lg2: Agent's
  }
  class lg0 teal
  class lg1 yellow
  class lg2 blue
  s0: Nothing over the pane
  s1: The agent's document is open over its pane
  s2: Your document is open over the pane
  s3: The agent's document is open over its pane, and one waits on the rail
  s4: Your document is open over the pane, and one waits on the rail
  s5: Nothing over the pane, and one waits on the rail
  [*] --> s0
  s0 --> s1: the agent opens one here
  s0 --> s2: you open a document
  s1 --> s2: you open a document
  s1 --> s3: the agent opens one: held, onto the rail
  s1 --> s0: you close it
  s2 --> s0: you close it
  s2 --> s4: the agent opens one: held, onto the rail
  s3 --> s2: you open it from the rail
  s3 --> s5: you close it
  s3 --> s4: you open a document
  s4 --> s2: you open it from the rail
  s4 --> s5: you close it
  s5 --> s2: you open it from the rail
  s5 --> s4: you open a document
  s5 --> s3: the agent opens one here
  class s0 teal
  class s1 blue
  class s2 yellow
  class s3 blue
  class s4 yellow
  class s5 teal
```
