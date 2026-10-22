# Close the reviewed boundary

Difficulty 8/10. Parker delegated the remaining decisions and completion while AFK. This extends the selected product, architecture, program design and slice plan; it does not record unread documents as approved.

## Product and architecture decisions

The offline profile accepts a canonical directory strictly below the OS account home, with no dot-directory in the selected path. Canonical paths outside that home, including system trees and volumes mounted at /mnt or /media, need a copy into a dedicated project directory. The rule checks the selected path, not a disk label: a mount already below an allowed home path is part of that explicit grant. Nested mounts within the selected root are refused. Hidden files already inside that project remain explicitly granted content. This keeps the first scope explicit and predictable.

Directory validation walks open descriptors with openat and fdopendir, never a reconstructed child path. Every entry must retain the root descriptor's device and statx mount ID. Symlinks are inspected without following them. A replacement of the selected path after pinning refuses the run. The existing limits on special files, hard links and scan size remain. A malicious Standard host can still mutate shared files; isolation protects in the opposite direction.

The ctl connection captures the peer PID from SO_PEERCRED. Against the current UI snapshot, a claimed pane survives only if the peer descends from that pane's shell. Unknown credentials or ancestry remove own-pane authority. A genuine relay and the Restricted supervisor remain descendants. Session names still prevent misrouting; they do not authorize a pane.

Validated note/deliverable operations alone may populate their internal config fields. A document marked off-limits is available only to its own authenticated caller, including its own floating square. Other callers cannot read a hidden split's notes. Scripted tab operations cannot edit a tab containing an off-limits pane. Queued bench bytes do not drain while the pane is off-limits. Hidden and absent targets answer alike.

One access proposal per run is printed by the supervisor in the run's terminal, naming the current resource and the workload-supplied request. It grants nothing and is inspectable through status. Only the outside operator command can dismiss it; dismissal never resets the lifetime proposal budget. The attention-rail integration and a structured scope-grant UI remain unavailable, and changing scope requires a new explicit launch.

## Verification and integration

Test pin/scan swaps, same-device mount identity, allowed projects and hidden/external roots, forged and genuine peer ancestry, internal-field injection, own/foreign document permissions, queued input and hidden-target receipts. Run real Linux probes and scratch native windows against both scripted and ordinary startup. Add development-push CI. Apply each mutation separately, restore exact bytes and accept a kill only when a named test actually fails.

Commit portable reproducers, label historical receipts by their source, and regenerate current receipts with source and binary digests. Both readable report copies must agree. Build the docsite, run the narrative gate with a new TD user as reader, and verify the commentable report in narrow and wide viewports. One PR into development; no live installation, agent-management integration or release changes. The next online profile receives product and architecture planning only.


## Handoff dispositions

| Reviewed requirement | Disposition and evidence |
|---|---|
| Scan the mounted directory | Descriptor-relative traversal, final root identity comparison and statx mount identity. Pin/swap controls and a real same-device nested bind-mount probe refuse launch. |
| Define the selected root | Canonical non-dot descendant of OS account home; hidden contents inside that project are granted. Help, changelog and user guide agree. |
| Repair evidence provenance | Earlier mixed-source logs are historical with unknown executed commits retained as null. Current receipts bind explicit source maps, actual test commands and exact binary bytes. Module/application mutations run separately with baseline and restoration controls; routing/startup use native windows. |
| Guard development CI | Workflow runs on dev/** pushes. Required-check enforcement remains a repository setting for Parker, tracked in issue216; no rulesets were changed. |
| Show an access proposal | One labelled supervisor message in the run terminal; outside-only dismissal, no approval operation and no changed grants. Unit and real OS controls pass. |
| Explain the feature | Dedicated docsite page, navigation, CLI listing and feature entry. Site build and narrative gate with the required new-user reader are recorded. |
| Bind own-pane authority | Kernel peer PID is checked against the current pane shell ancestry. Genuine native caller/supervisor succeed; forged caller is downgraded. |
| Prove startup opt-in | Tested environment reader plus an ordinary native window refusing all six mutations. Removing startup opt-in fails the native oracle. |
| Reject internal config injection | Note/deliverable fields cannot deserialize from config JSON. Otherwise valid wire fixtures fail under each independently removed guard. |
| Protect tabs and queued input | Both PID/index tab edits preserve hidden tabs. Queued input waits while hidden and resumes after unmarking; unit/native controls cover these effects. |
| Preserve own documents privately | Genuine own hidden document open/read succeeds. Foreign hidden document targets and ctl document routes are refused. Hidden and absent targets answer alike. |
| Check effects before persistence | Accepted live Apply precedes persistence. A stale allowed snapshot followed by refusal leaves no card for the sweep. Effects recheck current master/Writes policy. |
| Check media ctl receipts | Private local media rig commita4b6e22 rejects ctl/RPC failures before recording; eight reply controls and a fake refusal pass. No live scene was staged. |
| Plan the next scope | Product and architecture drafts under docs/plans/provider-restricted only. Provider compatibility and constrained recipients remain undecided; no online/TUI/delegation implementation. |

The local preview-preserving candidate has its own source manifest and full qualification; its three-file application delta is not silently attributed to the public development source. The readable report identifies both. Installation remains deliberately held, with a checked future-launch selection and undo command for the operator.

Verification-environment follow-ups are separate from the boundary: the [long temporary-path media fixtures](https://github.com/parker-brown-family/terminal-delight/issues/1097) and [closed hidden-window profile cleanup](https://github.com/parker-brown-family/terminal-delight/issues/1099). A short TMPDIR qualifies the suites; this run removes only its own confirmed-dead profile. Both reports have falsification steps and project-board mirrors.
