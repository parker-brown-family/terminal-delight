# Warroom Search

Warroom now has a search bar and a card for every pane. Type fuzzy text across the project, tab name and pane title, or use colons to search those fields separately.

| Search | Finds |
|---|---|
| `permissions` | A matching project, tab or pane title |
| `server:auth:refresh` | The refresh pane in the auth tab under server |
| `::permissions` | A matching pane title in any project or tab |

Escape clears the search; press it again to close Warroom. The × button also clears it. Editing the search clears a half-drawn connection.

Every card shows **AGE** and **TOKENS**. Age runs from the conversation's first recorded timestamp, including time before a resume. Tokens means the current context size reported in the transcript. Missing measurements say **unknown**, including a plain terminal with no agent session.

Captures: [all seven pane cards](2026-10-23-war-room-search-evidence/all-panes.png), [project:tab:pane search](2026-10-23-war-room-search-evidence/fields.png), [no matches](2026-10-23-war-room-search-evidence/no-matches.png), [Escape restores the list](2026-10-23-war-room-search-evidence/cleared.png).

These are captures of the native application in a separate hidden test window. Six stand-in agents provide known transcript timestamps and token counts; the seventh pane is a plain terminal. The test window was closed afterward.

The new tests cover fuzzy words, positional fields, Unicode, missing and future timestamps, and resumed Claude and Codex conversations. The final serial full suite passes **2,795 tests**, with 17 ignored. Clippy and formatting pass. Two loaded runs hit unrelated tests: the [host-session environment assertion](https://github.com/parker-brown-family/terminal-delight/issues/933#issuecomment-6366176044) and the [erased-text linger assertion](https://github.com/parker-brown-family/terminal-delight/issues/1126). Each passed alone and in the final suite; both occurrences are recorded.

The change is based on the latest 0.5 development tree, preserving the Clean shipping defaults. The optimized development binary is installed as `td-73306bca-0.5-war-room-search` and selected by the launcher. **Restart the window to see the change.** The running window has not been reloaded. The [installation receipt](2026-10-23-war-room-search-evidence/install.json) records the binary's checksum and previous launcher.

Source: [Warroom view](../app/src/war_room_view.rs), [session measurements](../app/src/vitals.rs), [keyboard capture](../app/src/main.rs), [translated strings](../app/src/lang/chrome.rs).
