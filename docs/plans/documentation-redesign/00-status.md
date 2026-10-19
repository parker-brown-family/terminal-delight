# Status: documentation redesign

**Difficulty: 4/10** — one public surface with a strong visual opinion; cheap to unwind before implementation → one gate
**Turned out to be:** 5/10 — the look took one gate as predicted; the port cost more than the score bought, because the live site's CRT bends a snapshot of the page, and three things the prototype never needed (theme colours a snapshot can read, glass the snapshot can draw, the wallpaper under the tube) only showed up in a CRT-on screenshot with the tube live

- Combined design gate: approved (Parker, 2026-09-25: "Colour me blown away", then "SEND IT")

## Slices

- [x] Workshop — five visual ancestors drawn against the same documentation home
- [x] Structure pass — three shapes (A quiet paper, B the room, C two faces), grey wireframes of Introduction and Install: `reports/2026-09-25-documentation-structure-workshop.html`
- [x] Structure picks — Parker marked wins and losses across A/B/C and the current site; combined into one design
- [x] Tracer bullet — a working two-page prototype on `docs/redesign-prototype`, `docsite/redesign/`, reviewed twice
- [x] Reading system and rollout — ported to all 29 pages and deployed 2026-09-25 (PR #815, merged as aa9efa0):
  - glass header, tools in Parker's order (theme · CRT · keys · light/dark · GitHub · Install), tabs Documentation / Reference / Languages / Keymapping, no kiosk among them
  - numbered chapters in the spine (weak chapter, strong page), numbered titles and Technical sections
  - eight Omarchy palettes with their wallpapers; the wall grows and pans down as you scroll and blocks roll into focus (the wellness-with-kate choreography, via the Omarchy kiosk)
  - the CRT tube still bends only the reading pane, now over the wallpaper (`window.TD_GLASS_WALL`)
  - command blocks with prompts, one-line `&&` copy, a throb
  - Introduction opens on Linux and Omarchy, with an On Omarchy section (Super+Alt+P) linking the kiosk
  - verify-site.mjs 668 checks, verify-kiosks 90, contrast ≥ 4.5:1 in all eight themes, glass and paper
- [x] Keymapping opens the app's own key sheet (PR #823, merged as 9740e10, deployed): the F1 SHORTCUTS sheet, read by the docs build from `app/src/main.rs` and `app/src/lang.rs`, with seven stale app rows given the code's version (`APP_KEY_FIXES` in `docsite/build.mjs`; the build refuses a fix whose row has gone). verify-site.mjs 673 checks
- [x] The key sheet's big button opens an example window (PR #831, merged as 7ca907d, deployed): the info kiosk's drawing of the app, read by the docs build from `info.html` between `td-window` markers, panes curved by `td-panes.js`, in the reader's theme. verify-site.mjs 676 checks

## Notes for a fresh session

The docs source is `docsite/` in this repository; the look is `assets/docs.css` and `assets/docs.js` over the shared `td-shell.js` and `td-glass.js`. Deploy with `docsite/deploy.sh` from a tree identical to main. The tube's snapshot limits are in the memory note "The tube snapshot sees stylesheets, not the glass".
