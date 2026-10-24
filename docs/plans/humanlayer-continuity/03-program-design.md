# Program design: Evidence at ownership boundaries

Process changes land in vt/pty.rs, vt/pump.rs, host.rs, hostproto.rs, term.rs and the explicit close path in main.rs. A monitor borrows the PTY owner's descriptor without owning a duplicate. A close receipt distinguishes request delivery, hangup sent, observed exit and cleanup still pending. The owned Child pins process identity until signaling is finished; no stale numeric PID is used by host close.

Artifact indexing belongs in a focused artifact_archive module, using benchstore records, handover identity and the existing document-opening path. The archive's filesystem operations run on a worker. Capture and indexing failures are surfaced. Source history is never rewritten to fill missing past evidence. Tests cover same target/different content, repeated handover, unavailable files, unknown provenance, corrupt records, restart from disk and link/path escaping.

Markdown anchor evidence belongs beside md_notes and is wired through markdown_view and notes_ui. Matching compares normalized quotation and surrounding context; ties are explicit. Tests cover movement, punctuation/whitespace changes, edited opening words, removed quotations, duplicate insertion, equal evidence and format-1 migration. Shared HTML note fixtures must remain unchanged.

Least-confident choices: capture limits and retention, ambiguous-note presentation, and legacy anchors with no quote. Keep absence explicit and preserve input; prefer a visible unresolved note to an invented match.
