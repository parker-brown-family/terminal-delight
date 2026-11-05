# Architecture

Keep settings in skills.json under TD's config directory, with optional booleans so an unchosen preference remains distinct. CONFIG → Agents owns the controls. A Rust module embeds and installs a standard-library Python bridge and the bundled skills into a versioned TD data directory.

The host prepends native-command shims to PATH and exports Bash command functions. The functions survive a user's PATH initialization; personal aliases or functions can still bypass them. Launcher commands name the same shim directly. The bridge reads the current global setting at every invocation and forwards an inactive command unchanged.

Claude receives an added .claude/skills directory and an additional SessionStart hook. Preserve existing CLI settings and hooks. Explicitly disabled names are omitted. The hook confirms this run's attachment context; the closing receipt describes the evidence method.

Codex receives a private app-server process for one invocation, never the shared daemon. Add extra roots over its native WebSocket-over-Unix API, query skills/list, then connect its normal TUI to that server. Preserve command flags and the effective working directory. Shut the owned server down with the invocation. Unsupported remote or noninteractive modes run unchanged and report unknown.

Receipts live under TD state, keyed by session and pane. Each names its provider, supervisor PID, process start tick, selection digest, available skills and failures. The HUD validates the process identity and foreground group, so a live background agent cannot paint a different foreground agent green. Read on a throttled effects tick, never in rendering.

Import a repository once into TD's source cache and pin its commit; launches do not fetch. Preserve the repository around the selected skill so references and scripts retain their paths. Local folders retain their original location. Reject repository paths escaping the checkout. No user skill directory or global provider config is rewritten.
