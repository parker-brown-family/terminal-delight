# Program Design

In mcp.rs, let validated open_pane proposals pass target visibility checks; keep caller binding, server enablement and argument validation. In access.rs, distinguish request validity from whether the target is presently actionable. Add an optional UI entity key to Ask, set only by the window.

In main.rs, resolve hidden targets for open_pane, preserve the single pending request limit, and capture the target entity. Apply a Yes only to that entity with the original shell PID. A missing target produces no permission change. The human's answer remains the only disclosure or authorization step.

Tests compare real-hidden and nonexistent targets, repeated requests, No and Yes, stale target identity, missing caller, and MCP versus ctl document refusals. Existing scripted-window tests remain. Document readOnlyHint, note.by and the narrower control-socket contract.

Least certain choice: showing an unavailable-target proposal adds a harmless rail row. It avoids an existence oracle without inventing a second hidden queue or rate limiter.
