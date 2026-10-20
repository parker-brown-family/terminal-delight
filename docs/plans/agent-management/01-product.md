# Product: A Managing Agent Can Direct Another Agent

## Problem

Parker asked a managing agent to tell a specific running agent to write a Markdown paragraph. The manager could find it and leave a note, but could not deliver a request that started work. This prevents delegation, steering and reliable result collection across agents.

## Intended Behavior

A manager finds an agent, sends a whole request and receives a receipt it can inspect later. Delivery works in background tabs. Replies, questions and reported results stay associated with the request. The person never needs to visit a tab merely to release delivery.

Delivery evidence and work evidence are separate. Accepted, written to the process and acknowledged are different observations. Work can be started, need input, have a reported result, or remain unknown. A turn ending never establishes task success.

## Product Decisions Approved

1. Allow background delivery under existing local trust, exposure and write settings. Record sender, recipient and request. No confirmation per message. Explicitly amend the human bench's visibility requirement for management messages.
2. Bind each request to the selected conversation and its current run. A replacement agent or shell never inherits it. Require fresh selection after identity changes.
3. Hold ordinary requests while busy when an adapter establishes a safe delivery point. Otherwise report unsupported or unknown readiness. Interrupting is a separate explicit operation.
4. Reuse request identity on retry and return the existing receipt. Never automatically resend an uncertain process write. Do not promise exactly-once execution.
5. Preserve the person's unsent draft and approval controls. Identify management messages as such; do not impersonate a person's response.
6. Deliver, inspect, reply and cancel pending requests first. Add explicit interruption only where the adapter supports it. Launch and termination remain separate product decisions.

## Success Metric

This manager prompts the selected Claude agent to write the requested Markdown paragraph. It retrieves a correlated reply and independently verifies the file. Repeat with Codex. Background, busy, duplicate, cancelled, replaced and unknown recipients must report their actual outcomes without input reaching a shell. A queue entry alone cannot pass.

## Screens

The brief draws the behavior, evidence states and acceptance cases. A compact request history belongs in the existing workbench; detailed interactions follow Product approval.

## Scope

Parker explicitly approved directing an agent process already running in another Terminal Delight tab or pane. This feature never creates, forks, resumes or replaces an agent process.

Communication among existing local agents, retaining human stop controls and the accepted local trust model. Explicitly address the authorized instance for cross-window routing. Remote users, network access, automatic tool approval, arbitrary terminal input and a replacement task system are outside this gate.

## Gates

Product settles observable behavior. Architecture chooses ownership, receipts and adapters. Program Design fixes types, failures and tests. Vertical Slices begin with the actual Markdown task.
