# Plan: Shared launch briefing

**Difficulty 5/10** — adapter delivery and note targeting must agree without a machine-specific instruction file.

## What it is for

A new TD user launches an agent, receives an illustrated HTML decision brief for work that requires a decision, comments, and returns the anchored note map. TD already ships the kit; its launcher only hands it to Claude. Path A was selected in the First-launch briefs page and approved with "Let's do it!".

## The shape

Keep one surface::launch_briefing and one bundled kit. Claude uses its existing appended system prompt. Codex uses its documented initial PROMPT argument, prefixed with the packet; this preserves configured developer instructions and AGENTS.md. A context-only opener asks the program to wait for the user's task. A supplied task travels in the same positional argument.

Add Harness::Local and a user-owned local-agent.toml in TD's config directory. LocalAdapter owns command: Vec<String>. Arguments are individually POSIX-quoted; whole-argument placeholders are {briefing} (file path), {context} (packet text), {prompt} (user opener). At least one packet placeholder is required. Local configuration owns model, effort and permission flags; TD offers no misleading dials. Missing/malformed configuration is a visible launch error.

Interfaces: Recipe::command_line remains for builtin command construction; Recipe::command_line_with_adapter(briefing, adapter) -> Result<String, String> is shared by preview and actual launch. LocalAdapter::load() -> Result<LocalAdapter, String>. All selected launches receive the kit and packet, or remain in the launcher with the reason.

The local command gets TD_LOCAL_AGENT=1 as a command-scoped environment value. The host and window foreground readers recognize that exact declaration only on the program's foreground process, never on the enclosing shell. Transport it through the existing WireMode::Other string vocabulary; it is a display classification, not authentication. PaneMode::is_agent recognizes the reserved local value while reads_transcript remains false. Return uses the existing composer or bracketed-paste check, never auto-submits.

## Least confident decisions

1. Codex startup context is a user prompt rather than a developer-instructions override. This avoids replacing configured instructions; verify exact argv and document the context-only startup behavior.
2. Generic local programs must accept a configured prompt/file argument and bracketed paste, or use the bench composer. No model competence is implied by successful configuration.
3. The reserved Other vocabulary preserves the host wire but an old host cannot recognize the new process marker. Verify the new host/window behavior; report that an existing host needs restart to observe local agent identity.

## Slices

1. Tracer: inject the packet into Codex and test captured arguments with a temporary home, including quotes/newlines and an opener.
2. Local adapter: validated config, argv expansion, explicit foreground marker and note recipient classification.
3. Launcher integration: equal briefing path, fail visibly, honest dials and actual preview.
4. Run focused regressions then repo checks; report measured transport and unmeasured real-model behavior separately. No new LLM client, prompt routing model or global config modifications.
