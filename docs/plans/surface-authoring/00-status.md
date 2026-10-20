# Custom surface authoring

**Difficulty: 6/10.** The existing skin/material seam can be strengthened, but the authoring contract has to prevent the omissions and font/layout surprises exposed by Chalk.

- Inventory audit: complete against the tactical workshop's 46 rows and main 0dcdb741. Source corrections and an unapplied patch are retained in `evidence/`.
- Combined architecture and slice plan: ready for review; awaiting one approval before implementation.
- Implementation and skill: not started.

The inventory under review lives on `design/tactical-skin-workshop` at `docs/spec/skin-surface-inventory.md`, introduced by 96316efd. Preserve its ownership and coordinate corrections with that pane. Source work is isolated in this worktree; the shared primary checkout contains other work.

## Verification

The self-contained review page passed browser checks at 968 and 390 pixels: no horizontal overflow, no external requests or JavaScript errors, all 46 inventory rows annotatable, notes added/exported/saved, and concurrence and clear exercised. See `evidence/browser-qa.json` and retained captures. The proposed correction patch passes `git apply --check` against the sibling's inventory; it has not been applied to their branch.

This was a source audit, not a new native certification of every surface. No application code or installed skin changed. Performance was not measured. The narrative diagnostic classified the page as an argument; it did not establish a cold-reader journey. Application tests were not rerun for this documentation-only change.

## Next

Review `plan.md` through the commentable HTML page. On approval, start with the Subscriptions tracer and prove a custom file changes the real native controls without editing their caller. Keep all four slices and the eventual skill in this task's single implementation review.
