# verify/ — checks that every order of events keeps a promise

Tests check the cases someone thought to write. These check every order of steps in a small model of the code, or every way a person opens a surface on the real window, and each one says in one sentence what it promises and what breaks for a person if it does not.

| Folder | Promise | Tool | Today |
|---|---|---|---|
| [`keyboard/`](keyboard/SKILL.md) | typing into a surface never reaches the shell behind it | the real window (`app/src/verify_keyboard.rs`) | holds (the language picker leaked; fixed) |
| [`rest-sleep/`](rest-sleep/SKILL.md) | rest never freezes a turn; sleep never loses pending work | TLA+ | 2 findings |
| [`worktree-cleanup/`](worktree-cleanup/SKILL.md) | removal never deletes work or a tree under a process | TLA+ | 2 findings; the quarantine fix holds |
| [`overlay-stack/`](overlay-stack/SKILL.md) | what is on top gets your keys and your Esc | TLA+ and replays (`app/src/verify_overlay.rs`) | 3 findings, replayed |
| [`flows/`](flows/README.md) | the INBOX and the Settings book, as user flows | TLA+ | hold |

`verify/check.sh SPEC.tla [CONFIG.cfg]` checks one model, each rule on its own, against the folder's `findings.txt`. `verify/mutate.sh` proves a rule can fail. Each folder's `view.ts` is the reader's words for the pstack `/statechart` skill, which draws the model and gives each break as a path. `scripts/link-verify-skills.sh` links each `SKILL.md` where Claude Code finds project skills.

The models were designed with pstack's `create-formal-verification-skill` and `statechart` skills (Lauren Tan, MIT, github.com/cursor/plugins/tree/main/pstack); no file of theirs is copied here.
