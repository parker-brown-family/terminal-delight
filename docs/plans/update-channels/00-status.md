# Status: Update Terminal Delight from inside TD

**Difficulty: 8/10** — replaces the running binary on a live install, must stay compatible with the persistent session host that holds every terminal and agent, and a wrong call stays invisible until the day an update bricks a window → four gates.
**Turned out to be:** 7/10. The host problem was as hard as scored, but almost all of it had already been solved by the session-host split. An update only restarts the window, so the new work was mostly honest bookkeeping (which build, which wire, which install) plus one new public artifact (the rolling dev build). The 8 bought one useful gate. The rest were collapsed at Parker's word.

- Gate 1 — Product: revised 2026-11-02 after Parker's note on the page. Q1 answered "100% yes" (ships in 0.5.0 if it qualifies Nov 9–12). His cutover concern added D9 (back up settings and small records before every install), D10 (name losses before the press) and D11 (a full restart resumes agents).
- Gates 2–4: **collapsed by Parker**, 2026-11-02: *"no more gates - jsut go all the way and finish this piece and get the PR merged in I am afk"*. The architecture and program decisions are recorded in the module docs of `app/src/update.rs`, `app/src/update_view.rs` and `app/build.rs`, and in `update-channels-built.html` beside this file.

## Slices (built in one pull request)
- [x] Build identity: `build.rs` embeds the commit, date, dirty flag and window↔host wire fingerprint; `--version` names the commit; `--build-info` prints JSON.
- [x] Engine (`app/src/update.rs`): channels, discovery via GitHub's public API, checksum, extract and smoke check, settings backup, AppImage and launcher installs, rollback, staging, cold-start apply.
- [x] `terminal-delight update` CLI.
- [x] ⚙ CONFIG › UPDATES panel (`app/src/update_view.rs`), in nine languages.
- [x] CI `dev-build` job: the rolling public dev build with `build.json`.
- [x] End-to-end scripts `scripts/update-e2e-appimage.sh` and `scripts/update-e2e-launcher-host.sh`.
- [ ] Qualification run (ten real updates, zero terminals lost) in the November 9–12 window, including Restart window and Restart everything driven from the panel on a real display.

## Notes for a fresh session
- Source request: Parker's feature-request email of 2026-11-02 00:25 ("Update Terminal Delight from inside TD"), quoted in `01-product.md`.
- Built in `/home/parker/Work/td-update-lab/td` (branch `feat/update-channels`, cut from `origin/dev/0.5.0` at 3e2f69bb), against a private patched `zed-upstream` beside it. Moved there from `~/Work/td-update-channels`, which no longer exists.
- The Gate 1 page is a commentable brief (id `update-channels-gate-1`). Edit `mockups/settings-updates.src.html`, never the built page, and rebuild with `python3 ~/.claude/skills/briefing/scripts/brief.py html settings-updates.src.html settings-updates.html`. Keep its comment ids stable.
- What is not proven yet is listed in `update-channels-built.html`, under "What is not proven yet".
- APES ticket: `update-terminal-delight-from-inside-td-stable-dev-channels-safe-swap-rollback-fo-mvzirwc7`.
