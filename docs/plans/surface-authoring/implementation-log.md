# Surface authoring implementation

Parker approved all three asks on 2026-10-20: complete the existing vocabulary, make the catalogue and native proof part of authoring, and ship the skill after a second file-only look works. His note was “Basically concur all”; he also warned that the sibling had done substantial work and the refactor must account for it.

## Baseline and coordination

The sibling's OPS implementation adds cut corners, bars, pip gauges, heading/data fonts, a grid, chevrons and focus hatch. The source branch incorporates that implementation rather than recreating it. The confirmed sibling pane received a note that the approved refactor is building on OPS in `td-surface-authoring`. The bound TD connector later closed; the local control socket delivered the note. A send is not an acknowledgement.

The first two source checks passed before the later document/menu adoption. The first complete suite passed 2,557 tests across seven targets, with ten ignored. Later native-driven corrections still require another check. Builds are serial, capped at two jobs and run at low CPU and I/O priority after the machine's cleanup agent reported pressure. No normal TD window or session host is restarted by this work.

## Implementation record (earlier checkpoints)

- Optical font fitting is specified independently of material. Caveat retains its legacy fallback; Chalk states it explicitly. The existing type ramp remains the source of reading sizes.
- Choices, actions, captions, meters, scrubbers and frame treatment dispatch through the skin. Stateless palette/switch controls live under `skin/parts.rs`; application renderers no longer import the paper backend or ask its material predicate.
- Terminal glass/stock and note decoration/Contrast stock are explicit policies, with defaults preserving existing files.
- Existing stock grain, roughness, lift and marker opacity are bounded data. The grain atlas remains shared. An explicit button-colour recipe is honoured; Chalk expresses its previous resolved colour through the palette's lighter foreground/background role.
- Document backends receive UI skin separately from authored-content theme. Notes controls, reading rails, video transport, PDF counters and floating frames use native chrome policy; authored content remains its own renderer's responsibility.
- `app/skin-surfaces.json` contains the original 46 families and 18 child entries. The binary exports JSON and Markdown from that source. It records routing rather than a coverage percentage, and native receipts remain separate.
- The native atlas runner is exported by the binary, so its version travels with the vocabulary and catalogue. Its default backend uses a private hidden Hyprland window; a visible private-window fallback accepts an explicit capture argv. The runner records actual build identity and leaves visual verdict/state review absent.
- The public skill is staged in the isolated `td-skin-skill` worktree. It has not been activated or shipped. The native second-look proof remains a prerequisite.

## Checks required at that checkpoint

Complete the Rust checks and native dark/pale, narrow, actual-grade, warp, zero/unavailable and content-boundary reviews. Use the resulting captures to correct geometry and record only the states actually inspected. Exercise valid/invalid reload, persistence and restore in an isolated session. Then publish the skill and update the review page with the implementation evidence. No current entry in this log establishes zero overhead or full native certification.

## Native checks changed the implementation

The capture runner initially treated a control command's enqueue acknowledgement as an applied MCP exposure policy. It now polls the UI's status mirror (`enabled=on writes=on expose=all`) before issuing tools. This is a readiness check, not a fixed sleep.

The first successfully captured Subscriptions scene failed visual review. The agent wall behind it kept registering its narrower backdrop mask, so the wider foreground panel was blurred and cut off at both sides. The wall now yields both frost and focus-tube registration while Subscriptions is in front. Both failed captures and their failed verdicts are retained under the private native evidence root. A new binary is being built for the corrected comparison.

## Feedback that shaped the boundary

| Parker's feedback | Resulting rule or check |
|---|---|
| “CHALK — is the surface and then it FOLLOWS the TD theme” | One material follows palette roles; no light/dark skin split. |
| Menu type was too small, bottom-aligned and cut off | Optical scale/minimum/leading are explicit data; native fitting uses the actual UI family. |
| “JUST the highligher stroke… JUST like the agent state rail” | A choice marks its label once; an action has separate raised stock. |
| The outer frame, spine, rails, palette, sliders and Subscriptions missed treatment | Per-facet inventory and native fixture receipts, instead of a single checked surface label. |
| “we really want this near tactile feel” | Existing contact shadow, deckle and grain remain the material renderer; lift and roughness become bounded data. |
| Terminal glass/stock could collide with CRT warp | Tube policy is explicit and independent of chrome material; native warped proof remains necessary. |
| “TRACK THE STEPS YOU TOOK AND THE FEEDBACK” | Preserve the approved plan, source audit, implementation log, failed captures and versioned proof. |
| The full goal is a skill anyone can use | Binary exports the contract and matching runner; the portable skill holds the workflow, not a copied inventory. |

## Integration and checks

The default full suite passed 2,559 tests with ten ignored after the shared-control migration. Both CI Clippy variants passed. A stricter all-target/all-feature invocation found ten test-only lints; the identical errors reproduced with this work stashed out. Additional evidence is attached to the existing test-lint follow-up rather than a duplicate issue: https://github.com/parker-brown-family/terminal-delight/issues/537#issuecomment-6294915399.

Main advanced during verification. The branch incorporates the quieter Intrusive scan lines, removal of the redundant theme skin chip, current link-opening fixes and the bug-report feature. The new bug panel was added to the inventory and routed through skin frame/action/type helpers immediately. That brings the catalogue to 65 entries and 26 registered fixtures. Its disabled state retains semantic dimming.

The live `ctl skin snapshot` endpoint reports the actual registry on demand. The isolated runner now checks a valid edit, a rejected syntax error, exact byte/token restoration, saved selection and persistence across a restart of only its own preview. No normal TD session needs a restart for a custom file edit.

## Final proof and handoff

The final default suite passed 2,570 tests and the fallback suite passed 2,523, each with ten ignored. Subsequent boundary changes passed 36 skin and 24 allowance tests and both CI Clippy variants. The action-order and private-atlas isolation corrections passed strict Clippy and release builds. The integrated preceding head passed the complete GitHub CI workflow.

The final binary SHA-256 is `7579cf94965627de3f57d31f5bd35c4ec1445a3d7f1b903dd8cfdb19a9ae952b`. Twenty-nine receipts cover 25 distinct native scenes; each names its actual palette, geometry, grade and inspected static states. Seven catalogue families have no dedicated fixture. Hover, drag and full keyboard interaction remain unverified. The final isolated set was visually compared in montage against the earlier detailed static reviews.

A long workbench preview exposed a privacy gap: filtering environment variables did not prevent the normal usage collector from loading the operator’s real cached account readings. Those exploratory screenshots remain private and are not in the proof bundle. The runner now uses an explicit fixture shell, private discovery root, disabled account cache/collection and a blocking canary executable. All 29 final receipts report the canary untouched. The ten-second workbench sample exceeds the first refresh interval. HOME is unchanged.

The initial persistence experiment was invalid because `TD_SKIN` deliberately overrides saved selection on startup. The corrected test uses the private config’s normal skin file and proves default persists across a private restart before restoring custom. Failed experiments remain labelled separately.

The public td-skin skill merged in agent-skills and is installed through discovery symlinks from its clean tracked worktree. Agent parity passes. It references the binary-exported vocabulary and inventory rather than copying either.

Parker challenged the elapsed time after two hours. The work had expanded from a tracer into repeated full builds and a broad native matrix. The concrete process correction is to freeze a small proof matrix up front, batch source fixes into one build and stop once its acceptance checks pass. No claim of zero overhead or full visual certification is made.
