# Launch from either pane face

Difficulty: 4/10. One shared launch path gains two entry points; incorrect focus could send the command to the wrong pane.

Parker's requested behavior is the combined plan and approval: “alt+a should launch agent IN TERMINAL using the launcher” and “EXACTLY MIRROR (reuse!!!! the workbench flow)”. Add a persistent ⌁ AGENT header button beside TERM/BENCH, and Alt+A as a pane chord. Both emit the existing OpenAgentLauncher event. The existing project/harness/model/effort/reach picker, briefing preparation, command construction and run_line/adopt_pane execution remain shared. A terminal launch retains TERM; the existing bench launch retains BENCH. No second prompt writer or composer is introduced.

The clicked pane must remain the launch target and regain focus when the picker closes. At a known shell prompt the command executes there; busy or unknown panes keep the existing new-tab landing. Nothing types the launcher chord into a running program. The least-confident detail is header mouse routing, so verify the real rendered button through GPUI, as well as keyboard handling on both faces and prompt transport with a real PTY.

Validation: targeted launcher/GPUI tests, Rust format and clippy, full default and fallback-core suites, and the keybindings sheet's existing checks. Deliver a short illustrated HTML outcome.
