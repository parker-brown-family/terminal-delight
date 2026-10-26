# Program design: make stale authority observable

Begin with a GPUI regression that seeds an AskBook with identifiable synthetic answers, transfers either endpoint through the real War Room assignment method, and asserts that old answers and approval prompts disappear. Deliver a late completion and prove it cannot restore the answer. Keep an unrelated entry as a positive control. Inspect the public ask-answer gate for current identity and policy checks.

Reuse the existing fixture constructor and cleanup hooks. Record the pre-fix failure, exact test names and commands. Expand existing Rust tests where that gives a deterministic oracle; put any native fixture runner in the registered APES script store and preserve its source with the evidence.

Real MCP checks use a private session and mock agent children, distinctive canaries, explicit target PIDs, recorded requests/responses, and exact-PID cleanup. A refusal is verified against target state, not only a response string. Unknown measurement remains absent rather than becoming zero.
