# Floor canvas

**Difficulty: 4/10** — layout and chart hit geometry share a viewport transform; permissions and storage stay unchanged.
**Process:** Parker explicitly waived proposal gates and requested implementation, merge, rebuild and daily relaunch while AFK.

- [x] Align the pool with the composer; compact the form and pool cards.
- [x] Add pointer-anchored Ctrl+wheel zoom, background panning and reset.
- [x] Verify native geometry and relationship gestures, then run both core gates.
- [ ] Merge into the latest `dev/0.5.0`, rebuild and cut over the daily window with preservation receipts.

The viewport belongs only to the chart. Its controls and the pane pool retain their size. Card positions, text, connectors, labels and hit areas share the chart scale and offset. The existing live-window guard and session host remain the deployment path.

Local validation passed: 49 Floor checks; full suites 3,005 / 2,952 passed, with 17 existing ignored per core; formatting, both strict Clippy commands and optimized build. See `evidence/checks.json`. Native screenshot capture was unavailable while the desktop was locked; `evidence/layout-reconstruction.png` is explicitly a drawing, rendered and inspected in Chromium.

**Post-hoc difficulty: 4/10.** Sharing the chart transform with hit geometry needed native event checks; the permission and storage boundaries were unchanged.
