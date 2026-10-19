# Local agent launches

The Workbench's LAUNCH AGENT panel offers Claude, Codex and local. TD writes the bundled decision-brief kit and one shared instruction packet before starting any of them. Claude receives an appended system prompt; Codex receives an initial context prompt and waits for a task. Existing global agent instructions are not edited or replaced.

For a local runner, create `local-agent.toml` in TD's configuration directory (`~/.config/terminal-delight` unless XDG_CONFIG_HOME says otherwise). The file contains an argument array, not a shell command string. For example, Codex's local provider can be configured as follows, replacing the model with one installed in the local provider:

```toml
command = ["codex", "--oss", "--local-provider", "ollama", "--model", "YOUR_INSTALLED_MODEL", "{context}"]
```

Codex's supported local providers are `ollama` and `lmstudio`. This example describes the CLI adapter; it does not install a provider or a model, and no real-model acceptance run is implied by the example. Other interactive agent programs can use their documented instruction argument:

```toml
command = ["/absolute/path/to/your-agent", "--your-instruction-file-flag", "{briefing}"]
```

The flag in this second example is a placeholder for that program's real flag. A bare chat client without filesystem access cannot read the kit or write the HTML, so it does not meet this workflow's contract.

Each placeholder occupies a whole argument:

| Placeholder | Argument passed to the runner |
|---|---|
| `{briefing}` | Absolute path of the shared TD instruction file |
| `{context}` | The instruction file's text plus a task, or a short request to wait for a task |
| `{prompt}` | The explicit user task, or an empty argument when there is none |

A command must include `{briefing}` or `{context}`. Its executable and arguments are shell-quoted separately. The launcher shows an error for missing or malformed configuration, embedded placeholders, or a packet that could not be prepared. Reopen the launcher after editing the configuration; its preview and launch use the same configuration snapshot. The local command owns its model, effort and permission settings, so TD hides those dials for local launches and journals those unknown fields as null.

The local command gets `TD_LOCAL_AGENT=1` scoped to that command. A new session host or window recognizes that declaration on its foreground program as LOCAL. The enclosing shell is still a shell after the program exits. A local command that is actually Codex or Claude retains that known program's identity. This is a display and notes-routing declaration, not authentication, and it changes no host protocol variant. An older running host needs to restart to learn the new declaration.

A brief's ↪ action places the anchored map in the Workbench composer, or pastes it into the program's terminal when that program has enabled bracketed paste. It never submits the prompt; the user presses Enter. Without bracketed paste, TD explains the refusal and offers copy map. Local transcripts remain unavailable to the FOCUS transcript reader until a reader for that program exists; it uses the screen path instead.

Verification: `cargo test --locked brief_launch` exercises literal packet/task arguments, actual kit preparation for every launcher branch, malformed configuration, a real PTY's local-to-shell identity, and note delivery through the composer and prompt. These prove transport and interaction mechanics; actual model behavior still needs the decision-task, comment-map and trivial-task acceptance run recorded in the real-agent briefing follow-up.

CLI sources checked 2026-10-14: local `codex --help` (PROMPT, --oss, --local-provider) and [OpenAI configuration documentation](https://learn.chatgpt.com/docs/config-file/config-reference).
