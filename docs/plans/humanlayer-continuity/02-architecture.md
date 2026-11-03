# Architecture: Extend TD's owners

## Process ownership
The PTY source owns the Child and performs explicit shutdown. The host and GUI request it through the pump, rather than signal saved PIDs. Monitoring must not keep a second master descriptor alive after close. Cleanup closes the master, sends hangup only while child identity is owned, uses a grace deadline, then escalates and observes the result. Late messages to a removed pane cannot target a replacement because pane IDs and pump channels are not reused. Existing attachment serials and kernel host locks remain.

## Artifacts
The conversation record remains the source of handed-over work. Derive an archive across those records on demand, off the UI render path. Persist unbound handovers with explicitly unknown conversation provenance so a pane closing cannot discard them. A stable digest of normalized target identity identifies a file; observed content has a separate SHA-256 revision. Capture bounded local regular files when handed over, preserve recoverable content in the existing local state tree, and leave web targets, legacy revisions and failed reads explicit. No remote fetch or arbitrary filesystem sweep.

## Notes
Extend only TD's Markdown note store; HTML briefs keep their existing shared note-island contract. Version the Markdown format so old writers refuse to overwrite stronger evidence. Keep original quotation, nearby block text and revision alongside each note anchor. Resolve against a new parsed document without rewriting that original evidence. Render resolved notes on their current block and preserve ambiguous, outdated and legacy notes in the exported map and notes controls.

## Integration
Target dev/0.5.0, milestone 0.5.0. No new dependency or service. New storage remains local. Retain the running build's additional daily features during installation; do not kill the terminal host merely to update its binary.
