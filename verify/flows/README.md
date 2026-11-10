# User flows, as checked models

The two flows a person lives in most, each a small TLA+ model with the code it stands for named in its header, and a `*-view.ts` that gives the pstack `/statechart` skill the reader's words.

- `Inbox.tla`: one agent through the INBOX shelves (`app/src/inbox.rs`, `shelve`, `Store::advance`). Holds: a question asked after you put a row away comes back to LOOK AT ME; a question is never on the quiet shelf.
- `SettingsBook.tla`: the Settings book (`app/src/settings_view.rs`). Holds: a panel shown in place never outlives the book; Esc goes back one level.

Run: `verify/check.sh verify/flows/Inbox.tla` and `verify/check.sh verify/flows/SettingsBook.tla`. Both rules of each were broken on purpose with `verify/mutate.sh` and caught.
