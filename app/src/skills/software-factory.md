---
name: software-factory
description: Plan substantial software changes before implementation. Use when starting a feature or a project: size the task, choose the appropriate planning gates, and build testable vertical slices with a tracer bullet first.
license: MIT
---

# Software Factory

Make important decisions before implementation, while changing them is cheap. Start by stating an intuitive difficulty score out of ten and one sentence explaining it. Judge how long an error could remain hidden and what reversing it costs, rather than counting files.

| Difficulty | Process |
|---|---|
| 1–3 | State the score and do the work; no planning gate. |
| 4–6 | One combined plan: purpose, design, least certain decisions and slices. Get approval once. |
| 7–10 | Product, Architecture, Program Design and Vertical Slices, approved in that order. |

The person's explicit instructions control the process. Honor delegated implementation and an explicit request to skip approvals; record that delegation rather than inventing approvals. Their difficulty score wins. Two consecutive bare approvals indicate excessive ceremony: say so and propose combining the remainder.

Keep a state file and the decisions in docs/plans/<feature>/. Resume from the first unfinished stage; read existing approvals before changing the plan.

1. Product: describe the person's problem, success criteria and visible behavior. Draw any UI they need to judge. Keep implementation choices for the next stage.
2. Architecture: read the current code, then name the boundaries, data and end-to-end flow. Explain external dependencies and tradeoffs.
3. Program Design: name files, types, interfaces, tests and the least certain decisions before writing implementation bodies.
4. Vertical Slices: order small working slices. The first is a tracer bullet wired end to end; the next replaces its fixtures with the real happy path. Add one capability at a time.

At each required gate, present the concrete plan and ask for approval. After each slice, demonstrate the behavior and run relevant tests. Continue under any standing delegation. Never accumulate separate horizontal layers that cannot yet run together.

Record departures and validation honestly. Tests should reject the broken behavior they protect against. Keep user work intact, follow the repository's contribution rules, and deliver a human-readable explanation with evidence.

This portable TD edition adapts the house Software Factory workflow, inspired by Dex Horthy's HumanLayer approach. It has no dependency on a particular terminal, provider or private service.
