# War Room Orgs

**Difficulty: 4/10** — one UI surface, with a small saved grouping record and a migration that must preserve existing connections. One combined plan and one approval.
**Turned out to be:** pending.

- Combined plan: awaiting approval.
- Review page: `reports/2026-10-23-war-room-orgs.html`.
- No application implementation changes in this worktree.

## Slices

- [ ] Remove the expanded agent context panel; retain compact pane cards and selection.
- [ ] Add saved named orgs, create/rename controls, and membership selection.
- [ ] Preserve legacy seats and lines, verify global search and session restore, then build and install.

## Resume

Parker rejected the large selected-agent response card in his screenshot and asked to make and label orgs. The proposed model gives each pane one org. Existing seats and lines open together as an unnamed org whose label is unknown until a person names it. Explicit connections continue to control permissions. Search remains global and identifies the org containing each match. Read the plan and the page's note/concur map before implementation.

The search/card feature is in the parent commits and its open pull request targets dev/0.5.0. This org plan is a separate follow-on on `plan/war-room-orgs`; do not rewrite or drop the search work.
