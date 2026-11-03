# Product: Process authority

Status: selected by Parker for autonomous implementation, 2026-10-21. The first release scope is the offline command profile in 02-architecture.md.

## Problem

Terminal Delight is for one person, and that person can use a terminal for anything. Today, programs running under the person's account share the authority to control Terminal Delight. The person cannot give one workload a smaller, enforceable scope while retaining full access elsewhere.

The product should let a person decide what a workload may reach without requiring Terminal Delight to understand the person's profession or the purpose of every command.

## Intended behavior

Existing terminals remain Standard: programs have the account access the person normally gives a terminal. The interface never describes that as containment.

A person can explicitly start a Restricted workload. Before it starts, they choose its accessible work and any additional reach. The restriction applies to the workload and its children. A running Standard workload cannot acquire a Restricted badge by changing a preference; the person starts a separate restricted run.

Restricted work proceeds autonomously inside its approved scope. The person does not approve every action. A request for more access is a proposal, with the affected resource and consequence visible. The program cannot approve its own proposal or impersonate the person doing so. Requests must be bounded so a program cannot force a stream of approval interruptions.

The person can inspect effective access and revoke future TD access. Tightening file or network access can require ending the run and starting a new restricted run; the interface must say when that is necessary. Ending a run ends its live grants. Reopening a window does not silently transfer grants to a replacement workload. Already disclosed information and completed changes cannot be recalled; stopping new communication does not necessarily stop work already accepted.

Restricted handoffs retain their origin and permitted purpose. Under that bounded contract, a stronger workload never automatically executes a weaker workload's request with its broader access. Deliberately authorized Standard-to-Standard operator automation retains its existing shared-account trust. The person can authorize a specific handoff route and its allowed effects. Bounded automatic handoffs require independently constrained recipients; ordinary full-access sessions can receive proposals for the person's review.

If a required restriction cannot be established or verified, the workload does not start as Restricted. Standard is a separate explicit choice. Unmeasured isolation stays unavailable rather than becoming a green status.

## Success metric

Zero unauthorized effects in the approved cross-boundary adversarial corpus, measured against canary files, recipients, control actions and external effects. Denials only count when the attempted operation demonstrably reached the intended boundary. Every listed case reports pass, fail, unsupported or unmeasured.

The same exercise must complete its allowed work without a per-action permission prompt. Compatibility coverage is reported as tested programs and workflows; no claim of universal restricted-mode compatibility is made.

## Internal product promise

This is planning language, not public announcement copy. Standard terminals keep ordinary account access. Restricted work runs only with the reach the person granted, and can request more without granting it to itself. The person can see the effective access and stop future use. Unsupported restrictions prevent a restricted launch.

## Screens

- mockups/launch.html: Standard and Restricted launch choices, plus unavailable restriction state.
- mockups/access.html: effective files, connectivity and TD reach for one run; a request for more access; revoke and human-authorized handoff.
- The review brief draws the restriction states, cross-workload handoff and proposed rollout.

## Scope decisions proposed for approval

1. One person owns the session; work is arbitrary. No named user roles or workflow taxonomy.
2. Preserve Standard and make Restricted an explicit launch choice. Do not relabel existing processes.
3. Enforce Restricted boundaries outside the workload's control; restrictions apply to child processes.
4. Grant a useful scope once, run autonomously within it, and require independent authority to expand it.
5. Bound delegation as well as direct actions; messages do not confer broader recipient authority.
6. Expose enforcement failures and unsupported cases; no silent fallback or unsupported safety claim.

## What approval does

Approval selects this behavior. Architecture will decide feasibility, boundary ownership, compatibility and the first release scope. Program Design and the slice plan follow their own gates. No application code is part of this gate.
