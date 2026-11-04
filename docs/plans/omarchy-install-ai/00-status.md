# Status: Terminal Delight on Omarchy's Install › AI

**Difficulty: 5/10** — the pieces are small, but the installer runs on strangers' machines and a bad one is hard to take back. One combined plan.
**Turned out to be:** 4/10. The first-launch theme work the plan counted did not exist: new windows have followed Omarchy since the Clean default (#1119), so no app change and no new release were needed.

- Plan: the Install › AI brief, approved 2026-11-03 ("let's get to work").
- [x] `install.sh` at the site root, served by GitHub Pages; `scripts/test-install.sh` in the Release policy job.
- [x] Proven against the published v0.5.0 on a second Omarchy 4.0.4 machine, under scratch homes.
- [ ] Run the row's real gesture on an unlocked Omarchy session, and press Super+C there.
- [ ] Omarchy pull request, modelled on omacom/omarchy#17132, plus TD's class on the terminal list.

## Decisions

- **The AppImage lives at `~/.local/bin/terminal-delight`**, the layout the install page already documents, so the updater (`Install::AppImage`) and the uninstaller need no change.
- **An existing install is never touched**, including a dangling launcher or a copy elsewhere on PATH. Updating is the app's job.
- **The launcher entry and icons come out of the checksum-verified AppImage**, not a second download. They go into the same `desktop` receipt `scripts/install-desktop.sh` writes.
- **No Remove row yet.** Herdr's pull request left it out too, and TD's uninstaller deliberately keeps an AppImage.
