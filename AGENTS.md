# Terminal Delight: Project Instructions

Machine-wide agent rules continue to apply. This file contains project-specific release coordination.

Before substantive work, read the current release section of [RELEASING.md](RELEASING.md), follow its release-plan link, and check the live GitHub milestone and the PR’s actual base branch. Older checkouts can have older release notes; the live milestone and published development branch resolve that mismatch.

For each PR:

- Record its intended release with GitHub’s milestone field, or explicitly say `unassigned` when no release has been selected. Missing metadata does not mean approved, deferred or irrelevant.
- Set the base branch to the selected integration branch. Release intent and merge destination are separate facts. Do not retarget another owner’s work merely because it looks related.
- Use area labels to describe the work. Release membership belongs in the milestone rather than a second version label.
- State why the change fits the release and preserve its validation evidence. A milestone is a plan; passing review and tests, integration and the tagged candidate determine what actually ships.

For release management, follow the PR-survey procedure in RELEASING.md. Survey unassigned work, readiness, dependencies and changes since the previous candidate. Record inclusion or deferral decisions; do not include every open PR automatically.
