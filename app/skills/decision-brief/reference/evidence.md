# Evidence

A brief is only worth reading if its claims can be trusted differently from one another. This
is how to make that visible.

## Confidence labels

Every assertion carries one. `base.css` renders them via `.tag`.

| Label | Class | Means | Requires |
|---|---|---|---|
| **measured** | `.tag.v` | You ran something and read the number | The command or method, in the brief |
| **inferred** | `.tag.w` | Follows from evidence but was not directly tested | What it follows from |
| **hunch** | `.tag.w` | Pattern-matching. Legitimate; say so | Nothing but the label |
| **contradicted** | `.tag.c` | Tested and found false | The measurement |
| **not established** | `.tag.w` | Someone might assume you checked. You did not | What would establish it |

The `inferred` / `verified` split is the discipline that makes a brief trustworthy, and it is
the reason the 2026-08 intake audit found anything at all: a rule in our own stack had claimed
a token saving nobody had ever measured, and it sat there for months because nothing forced
the label.

**The rule that follows:** if a claim is testable and cheap to test, test it before writing it
down. "Roughly 1,500 tokens of skill descriptions" took one `wc -c`. "Abbreviations save
tokens" went untested for months and turned out to be false.

## Invalidation criteria

Every claimed defect carries the check that would prove it wrong — and the picker-upper runs
that check **first**, before changing anything. Changing a non-problem is worse than leaving it.

State it as a condition, not a wish:

> **Invalidation:** if any pair shows the short form winning by more than one token, the rule
> stands and this finding is wrong.

> **Invalidation:** if `ctx_read(path, "reference")` over MCP returns a stub, this is a CLI-only
> bug and the issue is retitled, not dropped.

A finding that survives its own invalidation test is worth much more than one that was never
subjected to it — and saying "it survived" is a real claim you have earned.

The `/escalate-issue` skill carries the full falsifiable-issue template. A brief's findings and
that skill's issues are the same object at different resolutions.

## Record the confounds

When a measurement goes wrong on the first attempt, **put that in the brief.** It is not an
admission, it is the most useful paragraph you will write.

From the intake audit:

> My first run had `LEAN_CTX_RAW=1` in the environment — a documented kill-switch that passes
> output through uncompressed — and the subprocess inherited it. Every mode read as 0% and I
> nearly reported that lean-ctx does nothing. Re-run with the variable stripped.

That does two jobs: it tells the reader the number is trustworthy *because* it survived a
correction, and it warns the next person off the same trap.

## Proxies

When you cannot measure the real thing, say what you measured instead and how far it is from
the target.

Claude's tokenizer is not published. Measuring on `o200k_base` alone is one proxy. Measuring
across four published BPE vocabularies spanning 50k→200k merges is materially stronger, because
agreement across vocabulary sizes makes it a property of the encoding scheme rather than of one
vocabulary. That is still not the target, and the brief says so.

Never quietly upgrade a proxy result to a direct one.

## Conflicts of interest

Flag them inline, at the point of use — not in a footnote. When a number comes from someone
with a stake in it, say so beside the number:

> This head-to-head is a competitor benchmarking a competitor, in the competitor's own harness,
> with reconstructed task specs. Treat the vs-baseline figures as credible-with-caveats and the
> head-to-head as directional only.

## What the audit does not cover

Every brief needs a limits section, and it belongs in a modal rather than the page. Four
questions to answer:

1. **What method was used, and what does that method structurally miss?** (Static analysis
   cannot see runtime behaviour.)
2. **Is the audited artifact the artifact that runs?** (A repo is not the published npm package.)
3. **What was sampled rather than exhausted?** (Depth-200 clone; six files, not the tree.)
4. **Which numbers came from someone with an interest in them?**

A limits section makes the rest of the brief more credible, not less. Readers who find the
limits themselves stop trusting the parts you did establish.

## Grading your own first read

If a brief revises an earlier assessment, show the scorecard: what held, what did not, and
**why the wrong ones went wrong**. The failure usually has a shape worth naming.

In the intake audit, four wrong calls shared one cause: judging 37 skills by their
`description:` frontmatter instead of their bodies — measuring the label rather than the
content, which was the same error as the token rule found two sections earlier. Naming that
pattern was worth more than any individual correction.
