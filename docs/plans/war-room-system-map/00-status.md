# Threshing Floor cross-org relationships + System Map — status / ExecPlan

Difficulty: **8/10**. The data model carries authority semantics (an agent gaining
reach it should not is a security surface), persistence must stay back-compatible,
and there is a new higher-level view. Being wrong is costly to unwind. Gates were
run as the decision-brief `reports/2026-10-25-war-room-relationship-ux.html`, which
Parker annotated and approved on 2026-10-26 — so this is the implementation (Gate 4
slices). Parker's caveats: test thoroughly + manually; nothing goes dormant on org
delete (erase, "100% on them"); org title colour varies by theme.

APES ticket: `war-room-cross-org-relationships-...-mvp6vxo0`.
Branch: `feat/war-room-system-map` off `origin/dev/0.5.0` (3328924a).
Worktree: `/home/parker/Work/td-authority-lab/war-room-system-map`.
Target dir: `/home/parker/.cache/td-targets/war-room-system-map` (reflink-seed from
the warm `war-room-relate-ux` target once the PR-1 gate frees it).

## Approved decisions (from Parker's brief notes 2026-10-26)
- **Data-model change: APPROVED.** Endpoint = Seat | Org. Cross-org allowed; org as
  target fans out to every member (resolved live); org→org allowed.
- **Max-wins precedence, fail-closed.** Strongest matching line wins; an unreadable
  rung word grants nothing.
- **Org deletion: APPROVED + NEW.** Confirm dialog states the count of relationships
  that will be erased. Nothing dormant — erase.
- **Drag opens a popup at the drop: APPROVED.** Dropping a card onto a *separate org*
  offers MOVE agent vs BUILD relationship (with the org, or with the individual
  agent dropped onto). [Tier 2]
- **Labelled wire: APPROVED** ("may prompt →", in the rung's colour).
- **Counter rename Asks→Prompts: APPROVED** (shipped in PR 1, feat/war-room-relate-ux).
- **Org title colour: APPROVED, varies by theme** (shipped in PR 1 as th.ansi[6]).

## Design — data model (app/src/warroom.rs)
- `enum End { Seat(SeatId), Org { org: OrgId } }`, `#[serde(untagged)]`, Seat FIRST so
  a bare integer (legacy seat) deserializes to Seat; `{org=N}` to Org.
- `struct OrgLine { from: End, to: End, rung: RungWord }` — never a bypass.
- `Chart { lines: Vec<Line>, org_lines: Vec<OrgLine> }`. New methods: `from_saved(lines,
  org_lines)`, `org_lines()`, `draw_org`, `erase_org`, `forget_org(OrgId)`,
  `org_line_count_touching(OrgId)` (for the delete confirm); `forget_seat` also drops
  org_lines referencing that seat.
- `End::covers(seat, orgs)` — seat end names it; org end names it when orgs place the
  seat in that org (direct containment, not `owner` which is None on ambiguity).
- `Chart::org_rung(from_seat, to_seat, orgs) -> Option<Rung>` = max over org_lines
  whose from.covers(caller) && to.covers(target), of known rungs.
- `reach(cfg, chart, orgs, caller, target, own)` — add `orgs`. Effective rung = max of
  direct seat line and org_rung; then `.min(ceiling(access))`; None → Hidden.
- `decide_ask`, `may_write_live` — add `orgs`; `anonymous_reach` keeps its signature,
  passes `&Orgs::default()` (no caller seat → org fan-out cannot apply).

## Persistence (app/src/main.rs)
- `StateFile.org_lines: Vec<threshing_floor::OrgLine>` `#[serde(default, skip_serializing_if =
  "Vec::is_empty")]`; Default `vec![]`. Load at ~6283 (Chart::from_saved), save at
  ~7948.

## Call sites to thread `orgs` through
main.rs:8196, 8513; mcp.rs:824 (snapshot needs orgs); threshing_floor_view.rs:3119 (test);
threshing_floor.rs internal 684/846 + tests. anonymous_reach callers unchanged.

## UI (app/src/threshing_floor_view.rs)
- Relax same-org locks: `RelationDraft.ready`, `threshing_floor_begin_relation`,
  `threshing_floor_choose_endpoint`, `threshing_floor_save_relation`.
- Builder can target a whole org / a seat in another org. [enables data model]
- Org delete button + counted confirm modal (reuse floor_move modal pattern).
- Labelled wire: `wire_route` already returns a `label_at`; render "may <rung> →".
- System Map (all orgs, cross-org lines, org-as-target) + drag-to-popup editor. [Tier 2]

## Progress
- [x] PR 1 (feat/war-room-relate-ux) = **PR #1149** open against dev/0.5.0, ticket mvou7v4a closed.
      prompt rename + filter + org title + counter. fmt/clippy/24 tests green.
- [x] Data model: End/OrgLine/Chart ops, resolution (max-wins fail-closed), persistence
      (StateFile.org_lines), threading orgs through reach/decide_ask/may_write_live + all
      callers (main/mcp/mcp_transport/view). 16 adversarial threshing_floor::tests pass.
- [x] Org deletion + counted confirm (floor_delete_org, threshing_floor_delete_org,
      threshing_floor_org_relationship_count, Orgs::delete). Members become loose; nothing dormant.
- [x] System Map view (floor_map / SystemMap): toggle, cross-org + whole-org relationship
      builder (draw/draw_org), relationship list (erase/erase_org), org reference boxes.
- [x] Labelled wire (ask-3): wire chip reads "may {rung} →" via wr_rel_wire, inked by rung
      (ask=amber). Cross-org seat→seat handled by the System Map (per-org builder stays
      same-org by design).
- [ ] Build green (in progress), view-layer tests, clippy -D gate, manual test, PR.
- [ ] DEFERRED to follow-up: drag-to-popup editor (ask-2 drop-at-popup + move-vs-relate on
      a foreign org) — the fiddliest gpui; a refinement over the working System Map.

## Branch state (PR 2)
feat/war-room-system-map off dev tip 3328924a. Files touched: threshing_floor.rs, main.rs, mcp.rs,
mcp_transport.rs, threshing_floor_view.rs, lang/chrome.rs. Target: ~/.cache/td-targets/war-room-system-map.

## Surprises & discoveries
- Cross-org SEAT→SEAT needs only relaxing `ready()`/builder locks — no new type. Only
  ORG ENDPOINTS need the data-model change. Keeps the landed, security-reviewed
  seat→seat path, bypass logic, layout, role_of untouched.
- astra (war-room agent) is not reachable on the MCP bus from this fenced pane; built
  solo per Parker's approved brief + heavy testing. Handoff file if a second opinion helps.
- Live window keeps being swapped by other agents (now td-047661b5-0.5-agaro-permissions);
  landing is a coordinated single swap on Parker's word.
