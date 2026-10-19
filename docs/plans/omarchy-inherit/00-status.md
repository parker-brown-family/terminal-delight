# Status: Inherit Omarchy colours

**Difficulty: 5/10** — a small toggle with live colour propagation and retained manual choices. One combined gate.
**Turned out to be:** 5/10 — live following is small; integration had to preserve the newer palette shelf and note controls.

- Combined plan: APPROVED 2026-10-17 — Parker: “Right - and INHERIT is sticky ... it sticks and WATCHES so that if OMARCHY changes themes, TD changes themes!” Persistent live following is required, not a one-time copy.
- [x] Tracer bullet: toggle reads active palette and colours outer plus following panes.
- [x] Live reload, persistence, unavailable state and manual-colour exit implemented; watcher tests pass.
- [ ] Finish final saved-state verification, CI, merge and installation.

Work lives in /home/parker/Work/td-omarchy-inherit on feat/omarchy-inherit, ported onto current main. The original usage-pace checkout retains only its pre-existing usage.rs edit. Installed Omarchy publishes under ~/.local/state/omarchy/current. Existing host environment test flake is tracked as #933 — the child environment can be read before exec. Coordinate shared build cache with the efficiency task before running more builds.
