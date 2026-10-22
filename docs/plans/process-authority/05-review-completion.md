# Close the reviewed boundary

Difficulty 8/10. Parker delegated the remaining decisions and completion while AFK. This extends the selected product, architecture, program design and slice plan; it does not record unread documents as approved.

## Product and architecture decisions

The offline profile accepts a canonical directory strictly below the OS account home, with no dot-directory in the selected path. External volumes and system directories need a copy into a dedicated project directory. Hidden files already inside that project remain explicitly granted content. This is a useful, predictable first scope rather than a growing credential-directory blacklist.

Directory validation walks open descriptors with openat and fdopendir, never a reconstructed child path. Every entry must retain the root descriptor's device and statx mount ID. Symlinks are inspected without following them. A replacement of the selected path after pinning refuses the run. The existing limits on special files, hard links and scan size remain. A malicious Standard host can still mutate shared files; isolation protects in the opposite direction.

The ctl connection captures the peer PID from SO_PEERCRED. Against the current UI snapshot, a claimed pane survives only if the peer descends from that pane's shell. Unknown credentials or ancestry remove own-pane authority. A genuine relay and the Restricted supervisor remain descendants. Session names still prevent misrouting; they do not authorize a pane.

Validated note/deliverable operations alone may populate their internal config fields. A document marked off-limits is available only to its own authenticated caller, including its own floating square. Other callers cannot read a hidden split's notes. Scripted tab operations cannot edit a tab containing an off-limits pane. Queued bench bytes do not drain while the pane is off-limits. Hidden and absent targets answer alike.

One access proposal per run is printed by the supervisor in the run's terminal, naming the current resource and the workload-supplied request. It grants nothing and is inspectable through status. Only the outside operator command can dismiss it; dismissal never resets the lifetime proposal budget. The attention-rail integration and a structured scope-grant UI remain unavailable, and changing scope requires a new explicit launch.

## Verification and integration

Test pin/scan swaps, same-device mount identity, allowed projects and hidden/external roots, forged and genuine peer ancestry, internal-field injection, own/foreign document permissions, queued input and hidden-target receipts. Run real Linux probes and scratch native windows against both scripted and ordinary startup. Add development-push CI. Apply each mutation separately, restore exact bytes and accept a kill only when a named test actually fails.

Commit portable reproducers, label historical receipts by their source, and regenerate current receipts with source and binary digests. Both readable report copies must agree. Build the docsite, run the narrative gate with a new TD user as reader, and verify the commentable report in narrow and wide viewports. One PR into development; no live installation, agent-management integration or release changes. The next online profile receives product and architecture planning only.
