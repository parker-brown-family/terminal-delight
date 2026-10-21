# Architecture — qualify the shared rendering path

Skin controls chrome/material; design and palette resolve the terminal theme; Grade accessors resolve focus lighting at drawing time. Keep lighting at the existing terminal grid and pane-bounded GPU image pass so skin recipes do not opt into it individually. Test the real wiring, original glyph colours, zero, focus, clock expiry and invalidation across those independent axes.

Use one feature branch and PR. Merge the selected development baseline and current approved main baseline into that branch, preserving the newer document-close key ownership. Do not absorb the draft agent-management implementation or change its routing. Record this approved appearance work as an explicit 0.5.0 inclusion.

Measure GPU command execution using timestamp queries in a private diagnostic build; measure CPU layout/paint separately. The diagnostic instrumentation stays outside the shipped patch stack. Compare matched workloads and actual framebuffer sizes, report the tested adapter and frame budget, and retain unknown coverage for other hardware. Keep existing qualification issue as the measurement home.

Install under an immutable build identity and hold the deployment lock through guarded GUI replacement or rollback. Compare the exact session host wire, save document-note preflight and pane PID/start-time/pane/conversation identities, preserve window placement and never send PTY input to existing work. The host itself stays running.
