# Judgement Calls

Work keeps meeting decisions nobody made. A plan cannot foresee all of them, and stopping to ask at each one costs the person more attention than the work saves. This reference says when an agent decides for itself, how it writes the decision down, and how the person answers it later.

## The Policy

When the work needs a decision the person has not made, make it and keep going, unless one of these holds. Then stop and ask, offering the choice you would make.

1. **It contradicts a decision the person approved.** A CONCUR stamp, a recorded verdict or an approved plan outranks your own recommendation, whatever the readings say. Pass the approved anchor with `--conflicts`.
2. **It is a one-way door.** Undoing it needs the person: it reaches outside the work, touches something shared, destroys something, or spends what does not come back, and it is not an ordinary step of the work they asked for. Pass `--door one-way`.
3. **The judgement slot reads it as theirs to make.** Two readings decide it: how uncertain you are, and how much the choice is the person's own taste. The project's posture sets the line.

| Posture | Stops when |
|---|---|
| relaxed | either reading is high |
| standard, and undeclared | either reading is high, or both are medium |
| strict | either reading is medium or high |

A reading you cannot make is `unknown`, and unknown stops under every posture: an agent that cannot say how sure it is should ask.

Whatever the outcome, record the call. One that carries on is reported as a fact, *I decided this*, and never left in a paragraph of prose.

## The Posture

Projects differ in how much being wrong and fixing it later they can afford. A project's own instructions declare its posture, for example `briefing posture: relaxed`. New ground inside a project, such as a new medium, a new style or a first client, runs one step stricter until it has earned trust. An undeclared posture reads as standard, and the record says it was undeclared rather than pretending someone chose it.

## The Judgement Slot

`{System 1 JUDGEMENT Surface Placeholder}` is where the two readings come from. Today the agent makes them itself, and the record says `filled_by: agent`. The slot is named so a System 1 model, a typed judgement service that returns probabilities, can fill it later without changing the policy: code applies the table, and the slot supplies the readings. Whether a planned step contradicts a standing decision is the same kind of reading, and for now that is the agent's too.

## Recording a Call

```bash
python3 scripts/brief.py call work.calls.json --title "Drag footer at 28 px" --chose "a 28 px footer" --because "the map needs the room and 28 px still fits a thumb" --passed-over "40 px, the old height" --undo "one constant" --door two-way --evidence shots/footer.png --uncertainty low --taste medium --posture relaxed
```

Exit 0 means carry on. Exit 3 means stop and ask: the call is recorded as `asked`. The ledger is `NAME.calls.json`, one JSON object kept beside the work, and nothing else is ever written to a file of that name. Its fields follow the Markdown Architectural Decision Record (MADR), which has served teams writing decisions down for years:

| Field | MADR's name for it | Holds |
|---|---|---|
| `title` | title | the call in plain words |
| `context` | Context and Problem Statement | why a decision was needed |
| `passed_over` | Considered Options | what else was possible |
| `chose`, `because` | Decision Outcome | the choice and its justification |
| `undo`, `door` | Consequences | what reversing it takes |
| `evidence` | Confirmation | the screenshot or file it was decided on, with its hash |
| `status` | status | `open`, `asked`, `concurred`, `overturned` or `superseded` |
| `judgement` | none | the slot, who filled it, both readings, the posture and the outcome |
| `under` | none | the approved brief the work ran under, with its revision |

A concur covers the evidence it was given, by hash, and the row it was given on. A row is generated, never written. `check`, `calls --sync` and `orient` render each call again from the ledger and require the page's row to be exactly one of its renderings, under any status the call could have had when it was printed; a row with anything added, removed or reworded, in words, pictures, links or structure, is a mismatch, and so is a tag that repeats an attribute, because a browser shows the first and a parser keeps the last. A row for a call the ledger does not hold, a row nested in another, and a brief that shows call rows without naming its ledger are refused by `check`. A waiting call needs a complete row of its own; an element that merely carries its id does not count. A stamp on a row that does not show the ledger's call is reported as `mismatched` rather than applied, and so is a stamp made on another version of the page. A built page records when it was built, so a stamp made after a rebuild counts even when older stamps on the same page do not. If the evidence of a call that still stands (open, asked or concurred) changes afterwards, `calls` reports it and the work is no longer settled until someone looks again. An overturned call's evidence is expected to change, since the fix replaces it, and a superseded call no longer stands; neither holds the work open. Writes to a ledger take a lock beside it, `NAME.calls.json.lock`, so two agents recording at once lose nothing.

## Reporting It

A call that carries on reaches the person three times, so it cannot be missed by someone who reads only one of them.

1. **Your next reply** names it in a line: what you decided, and that you are carrying on. A host with a typed channel for wanted-but-not-blocking asks gets it there as well. In Terminal Delight that is a `response` surface whose `escalation` has level `wanted`.
2. **The closing brief** carries it as a row the reader can stamp. `brief.py calls work.calls.json --html --for closing.html` prints the section with each call's evidence drawn in, and links made relative to the page that will show them; paste it into the source before the footer. Regenerating the section later leaves answered calls out, and their answers still stand: `orient` reads them from the ledger. Start that brief with `new --calls work.calls.json`, and `check` will refuse it if it leaves an open call out.
3. **Settling.** `brief.py calls work.calls.json` exits 2 until every call is concurred, overturned or superseded and every call still standing has its evidence unchanged. Until then the work is finished, but it has not settled. `calls --html` only prints, and exits 0 whenever it printed; ask `calls` alone whether the work has settled.

## Answers

- A CONCUR stamp on a call's row: `brief.py calls work.calls.json --sync brief.html` records it, with the revision of the brief it was stamped on.
- An answer given in chat: `brief.py verdict work.calls.json CALL concurred`, or `overturned --note "their reason"`, or `superseded --superseded-by CALL`. An overturn without the person's reason is refused, because the reason is what the follow-up is built from.
- A note on a row without a stamp comes back under `notes_to_answer`. Answer it, then record the verdict it leads to.
- An overturn becomes work: a follow-up issue or the next slice, linked with `--follow-up URL`. A concurred call becomes a standing decision, and `orient` lists it beside the plan's own.
- Silence is never agreement. An unanswered call stays open.

## Prior Art

| Source | Taken | Not taken |
|---|---|---|
| [T3 Code](https://github.com/pingdotgg/t3code) (MIT) | A question that lets the agent keep working and survives a restart; a thread that will not settle while a question is open; a run that points back at the plan that authorised it | Its code and runtime. These are ideas re-implemented here |
| [MADR](https://adr.github.io/madr/) | The record's fields and its status words | The file-per-decision layout; calls are many and small, so they share a ledger |
| Amazon's [2015](https://www.sec.gov/Archives/edgar/data/1018724/000119312516530910/d168744dex991.htm) and [2016](https://www.aboutamazon.com/news/company-news/2016-letter-to-shareholders) shareholder letters | One-way and two-way doors. "Most decisions should probably be made with somewhere around 70% of the information you wish you had", and "being slow is going to be expensive for sure" | The organisation around it |
| [12-Factor Agents](https://github.com/humanlayer/12-factor-agents), factor 7 | Asking a person as a structured record the work can pause on and resume from | Its SDK and transport |
