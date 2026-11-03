# Product: permission must survive scrutiny at the moment of use

The person should be able to move or remove an agent and trust that the previous organisation cannot receive new information through a request already waiting. Agent callers should see only the panes and operations granted to their verified identity. An unrelated request must continue working after another relationship is revoked.

Acceptance covers read visibility, writes, copy admission and answer collection, operator-only controls, forged identities, malformed inputs, and organisation transitions. Denial must leave the target unchanged. Positive controls prove the test did not merely disable everything.

The local process boundary is cooperative for unrestricted same-user processes; Restricted copies provide a stronger OS boundary. This battery will state which boundary each result establishes. It will not claim that a same-user unrestricted process cannot read the user's files independently of MCP.

Sources dismantled for relevant principles, without importing code: [OWASP authorization guidance](https://cheatsheetseries.owasp.org/cheatsheets/Authorization_Cheat_Sheet.html), default denial and validation on every request; [OWASP MCP guidance](https://cheatsheetseries.owasp.org/cheatsheets/MCP_Security_Cheat_Sheet.html), untrusted tool arguments and explicit transport boundaries. These principles map to the existing Rust gates and regression tests rather than a second permission framework.
