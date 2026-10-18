# Terminal State

**Difficulty: 5/10.** One combined gate, APPROVED 2026-10-17 by two explicit concurs.

Rename AGENT THEME to TERMINAL STATE. Keep a single persistent switch: FOLLOWS OUTER when off, USES PRESETS when on. Add a hover explaining SSH, agent provider, editor, system monitor and console coverage, outer fallback and manual-theme precedence.

Use the existing measured PaneMode and maybe_mode_theme seam. Keep agent_tint as the saved key for compatibility. Extract a visual-only catalog in app/src/terminal_state.rs; it maps a known mode or exact observed program name to a palette. No changes to is_agent, prompt safety, conversation readers, host protocol or terminal input. The old host can continue serving current terminals. Directly recognized executable names receive presets; ambiguous wrappers keep the observed executable’s preset (or outer if unrecognized), without guessing their provider. No screen-content inference.

Curated catalog: Claude amber; Codex ice; declared Local rose; additional directly observed agent programs (gemini, opencode, aider, copilot, crush, goose) share mint; remote (ssh, mosh-client, et, telnet) retains violet; editors (vim, nvim, vi, hx, helix, nano, emacs) use parchment; monitors (top, htop, btop, nvtop, glances) use green; consoles (python, python3, ipython, node, irb, julia, R) use blue. A console means an observed console executable, not a claim that it is at an interactive prompt. Shell, tmux/screen, unknown and other programs retain outer. This is a small context catalog, not a list of every CLI.

Precedence: manually painted pane wins; otherwise off uses outer; otherwise recognized state uses its preset; otherwise outer. Existing sticky agent ownership remains authoritative during agent subprocesses. SSH recognition means the local remote client is active, not that remote authentication or connection success was measured. Switching off restores the current outer/Omarchy theme. App-controlled ANSI output and display grade retain existing behavior.

Least confident decisions: include editors/monitors/consoles initially; additional providers share one mint look instead of one colour per brand. Colours and coverage are proposals. Direct executable recognition gives Node/Python wrappers the console/runtime look rather than guessing their provider.

Slices: (1) Rename, hover and remote/agent tracer through the existing seam. (2) Add context families and prove aliases, exact matching, shell/unknown fallback, independent themes and transitions. (3) Formatting, CI Clippy, full tests, release/native smoke; merge one PR and safely refresh GUI without restarting its host.

Acceptance: supported states change only following panes when enabled; manual panes remain unchanged; exiting to shell returns outer; an old saved agent_tint preference retains its meaning. Tests must reject a substring such as Vim opening a file named gemini, not only prove positive matches.
