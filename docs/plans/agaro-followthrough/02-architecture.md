# Architecture

Keep the existing control-socket fence and MCP handler. Add a paired regression test through the real socket handler and MCP protocol handler; no second authorization framework.

Open-pane requests must enter the caller's existing one-request queue for real, missing and already-open targets alike. Silently dropping a missing target would expose its absence through the next request's pending status. The rail can show an unavailable target to the person; the agent sees the same acknowledgement. Other request types keep their current visibility checks.

Bind a pane request to the current UI entity when it is queued. A later Yes must not open a new pane that reused the requested process ID. Missing targets remain missing for that request. Request state is ephemeral and is not a stored permission.
