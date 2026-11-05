# Skills Manager

**Difficulty: 7/10.** Attachment and truthful session status cross shell startup, two provider runtimes and native UI.
**Turned out to be: 7/10.** Native execution exposed WebSocket transport, provider-profile, process-lifetime and local-window identity boundaries that a picker alone would have missed.

Parker approved the assessed global-first, additive scope and delegated complete implementation and tests while AFK on 2026-11-05: “feel free to completely build the thing”. The design stages below are agent decisions under that delegation, not separate human approvals. No approval pauses are requested for these reversible implementation choices.

- Product: assessed scope accepted; recorded in 01-product.md.
- Architecture: recorded before implementation in 02-architecture.md.
- Program design: recorded before implementation in 03-program-design.md.
- Slices: recorded before implementation in 04-slices.md.

## Progress

- [x] Native Codex extra-root proof: isolated server, fixture skill enabled in skills/list; no model call, global daemon or config change.
- [x] Command bridge and additive native discovery.
- [x] Global menu, repository/folder/file sources and session HUD.
- [x] Meaningful integration tests, native acceptance and full application gates.
- [x] Commentable closing brief and source handoff.

Worktree: /home/parker/Work/td-skills-lab/terminal-delight, branch feat/skills-manager. The installed preview was based on f740cb68, retaining the concurrent floating-document update. Following Parker's acceptance and cutover request, the source was committed and updated to the current dev/0.5.1 baseline for the 0.5.3 wave, including the newer glow-pane keyboard behavior. The sibling zed-upstream is an isolated checkout of TD's pin with its patch stack applied. The shared Zed checkout had patch drift and was left alone. The canonical TD checkout retains its pre-existing changes.

## Installed for manual testing

### Compact frame revision

Difficulty 3/10. Parker's next manual test found that enabling Skills widened the menu and stretched it toward screen height. The menu now uses one centred frame capped at 840 × 600 logical pixels, independent of the Skills switch and list length. Both sections scroll within it. Smaller windows clamp the frame and stack the sections. Off still hides the skills section. The existing UI regression compares complete frame bounds across off/on, twenty imports, the import form, Deco and TERMINIMALIST.

Release intent: **0.5.3**, landing on **dev/0.5.1**. Parker approved cutover and asked about this release on 2026-11-05. The live milestone and current integration-branch release context confirm that target; older version references in the historical installation record describe the preview's base. Local process snapshots, resume lines and screenshots remain under ignored evidence/ rather than being published with the source.

Integration gates on the current release baseline: **3,367 Rust tests passed**, 18 existing ignored; formatting and strict Clippy passed. All 25 bridge/importer tests, including four native provider checks, passed again. The source delta from the installed preview is the newer integration baseline; no skill behavior was changed during release integration. The fixture-only captures loadout.png and import.png show the revised native controls without publishing process or recovery data.

### Revised loadout preview — current

Parker's manual test exposed the missing bridge in shells created by the retained pre-feature host. The revised build installs a TD-owned initializer only when creating a fresh plain Bash terminal; existing panes and resumed agents are never injected. Personal shell overrides remain authoritative. A measured missing bridge now has a localized HUD explanation rather than an unexplained question mark.

Agents now keeps the master switch in the main column. While enabled, a second independently scrolling column holds the skill switches and import control; disabling folds it away while retaining selections. Names open sources. Import has one recognized-source input for files, folders, file URLs, repositories and GitHub tree/blob/raw links. Repository revisions travel after #; slash-containing refs are resolved against actual Git refs. A source can contain several individually selectable skills. Supporting files stay available.

The launcher and daily window now use /home/parker/.local/lib/terminal-delight/td-f740cb68-skills-loadout, SHA-256 9f573e2761acbc7c83cda74113bf4ad05eb11c1ad396b96bda29797fcdd60ca0. The guarded swap retained the original host and all 33 open live shell identities, plus layout and geometry. Seven already-closed live terminals held for Undo were cleared; the eight already-ended records were also released. No open pane was eligible. Available resume lines were saved in evidence/revision/closed-resume.json. Receipt: evidence/revision/live/activation.json. The previous preview and pre-feature executable remain available. Automatic wave-follow updates remain paused during preview testing.

Current validation: 3,358 default-core Rust tests passed, 18 existing ignored; strict Clippy, formatting and git diff --check passed. All 25 bridge/importer tests passed with TD_NATIVE_SKILL_TESTS=1, including four native provider checks. UI tests exercise twenty-row scrolling, hidden disabled column, source-link clicks without toggling, the single field, clipboard and cancellation. An isolated native display verified one file URL importing three skills and the exact installed executable starting a fresh bare codex on an old clean host: all 23 fixture skills confirmed through skills/list. No model turn was submitted. All fixture services were stopped. All 36 changed strings (four keys across nine locales) were checked. The reference-core suite was not rerun for this revision; its earlier result below is historical.

Difficulty predicted 6/10; observed 6/10. The old-host boundary required an execution test beyond the menu tests. Use a new terminal in the current daily window for the manual bare-command test; the separate skills-test window is no longer required. Historical first-cutover details follow.

### Initial preview — superseded

Parker requested cutover on 2026-11-05: “go hard - get us there to test it.” The installed launcher and daily window now use /home/parker/.local/lib/terminal-delight/td-f740cb68-skills-preview, SHA-256 fe02e74cf3ce77c6e542b848fc85eef35f7527a6a5a56193353abcbdecf0e632. The guard retained host PID 2108452/start tick 55894047, all 40 pane identities including all 32 live shells, 25 exposed panes, layout, communication settings and geometry. The eight unrepresented records were already ended with no process; no live pane was released by this cutover. Receipt: evidence/cutover/live/activation.json. The previous installed binary td-f740cb68-wave remains available.

The initial preflight found three live closed-tab holds and selected a separate test session. A concurrent glow-document cutover replaced the window and released those holds before our installation. Our expected-launcher assertion detected that change before any mutation. A fresh preflight against that new window allowed our guarded daily swap. The earlier decision remains in the ledger as the record of what was known then.

TD Skills is enabled globally. The fresh window titled “terminal-delight — skills-test” is open on workspace 2; its own host and shell carry both provider command functions and the correct receipt identity. Use that window to type claude or codex. The retained daily host predates the bridge, so even newly created panes in that old session do not inherit it. On an isolated display, an ordinary typed codex confirmed Briefing and Software Factory through skills/list, using the existing personal copies; both HUD details and Agents settings were captured. That fixture window, display and private provider processes were stopped before the live test window opened.

On the integrated source, formatting and strict Clippy passed, 3,356 Rust tests passed with 18 existing ignored, and all 22 provider tests passed again. The standalone executable was rebuilt and stripped of debug information before installation. The automatic td-wave-follow.timer is paused so a branch build cannot overwrite this unmerged preview during manual testing; wave-follow.json records preview-testing. Resume the timer after integration or rollback. Installation, native receipt, fresh-shell proof, screenshots and rollback target are under evidence/cutover/.

The assessment is /home/parker/Work/reports/2026-11-05-td-skills-manager.html. The original APES task is “Add global TD skill selections to ordinary agent starts and report session attachment”.

## Validation

- Default core: 3,355 Rust tests passed; 18 existing tests ignored.
- Alacritty reference core: 3,289 Rust tests passed; 18 existing tests ignored.
- Provider bridge: 22 tests passed with TD_NATIVE_SKILL_TESTS=1, including four native checks on Claude 2.1.296 and Codex 0.162.1. Claude used a local mock endpoint; Codex sent no turn. No paid model call or shared daemon was used.
- Clippy with warnings denied passed for both cores; cargo fmt and git diff --check passed. After the final legacy-briefing correction, all 31 launcher and 17 briefing-hook tests were rerun and passed, followed by a fresh standalone build.
- Native GUI on an isolated Xvfb: settings, real local import, ordinary Codex startup, confirmed header and changed-selection marker.
- Closing HTML: notes edited successfully in isolated browser storage, Copy map present, nine comment targets, no horizontal overflow at 968px or 390px. Briefing and Fancy Pants gates passed. Narrative-gate cold understanding improved from 0.27 to 0.78 in the final page; arc argument.

Commands and local logs: evidence/ under this folder. CI runs the provider fixtures without requiring installed native providers; native acceptance is opt-in.

## Handoff

Primary review: /home/parker/Work/reports/2026-11-05-td-skills-built.html.
Standalone local build: /home/parker/Work/td-skills-lab/bin/terminal-delight.

Remaining product scope is tracked publicly:

- [Named Codex profile attachment](https://github.com/parker-brown-family/terminal-delight/issues/1358).
- [Per-session selections over the global default](https://github.com/parker-brown-family/terminal-delight/issues/1359).

Named profiles preserve ordinary agent behavior with Unknown attachment. Current sessions use the global selection; per-session controls are a later phase. Existing shells need a new terminal to receive the bridge; the revised window initializes fresh Bash terminals even when their host predates the feature.

## Execution findings

- Codex's native Unix socket speaks WebSocket, not raw JSONL; extraRoots/set is server-local. Keep one private server per invocation, never start the shared daemon.
- Its current app-server rejects --profile and skills/list has no profile field. Named profiles pass through unchanged.
- Bash exported command functions survive PATH initialization; personal aliases/functions and absolute binaries can bypass them. Other shells get PATH shims with Unknown when bypassed.
- Window-owned PTYs use a separate skill-receipt identity; they must not borrow the TD pane that launched the window. Hosted panes override inherited skill identities.
- A conversation ending is not process-death evidence. Claude runtime directories remain available across clear/resume and are pruned only after their owner PID exits.
- Native launcher and typed-agent briefings carry TD context without the older Briefing skill pointer. Offering it in a seed prompt would bypass the native adapter's personal-exclusion check; only the generic local adapter retains that optional pointer.
- Codex teardown ignores repeat termination signals while reaping its own children. The native TUI test verifies the private child processes end with their invocation.
- Local-source revisions hash the supporting files present at startup. Repository imports are immutable by resolved commit; reimport preserves the person's enable/disable switch.
