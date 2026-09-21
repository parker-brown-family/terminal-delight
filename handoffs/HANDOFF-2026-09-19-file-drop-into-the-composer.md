# Handoff — file drop into the workbench composer (2026-09-19)

## Status

**Landed.** PR [#561](https://github.com/parker-brown-family/terminal-delight/pull/561)
merged into `main` as `0ed2b718` at 05:47Z by the overseer. Branch
`bench/file-drop`, built in worktree `~/Work/td-file-drop` (clean, synced, safe
to remove). Main has moved four pull requests past it since.

**Not installed.** Nobody has run the gesture yet — see *Not done / next*.

## What's done

Drag a file out of the file manager onto a pane's workbench and its path is
typed into the agent's line at the caret, mid-sentence, with the rest of the
sentence after it. The composer lights up in the person's own colour while the
file is overhead. Several files arrive as several words; a name with a space
arrives single-quoted. A drop on the TERMINAL face pastes the path instead
(bracketed if the far end asked for bracketed paste). A drop on a card, the
rail or the strip does nothing, on the same terms as a click there.

| Where | What | Verified by |
|---|---|---|
| `app/src/workbench.rs` | `paths_as_words` — one path is one word; quoted only where a shell or line editor would misread it; control characters flattened so a newline in a filename cannot submit the line | table test `a_dropped_path_arrives_as_one_word` (bare path, space, several files, embedded quote, four hostile names, newline, empty list) |
| `app/src/pane.rs` | `.on_drop::<gpui::ExternalPaths>` on the pane ROOT div; `paste_text` split out of `paste_clipboard` | source guard `the_pane_root_takes_a_file_drop`, **broken on purpose and watched to fail** |
| `app/src/pane/bench.rs` | `bench_drop`; the clipboard's file arm sharing the quoting; `wb_drop` set in `bench_hover`, cleared by window-level MouseUp and FileDropEvent listeners in `pointer_hook` | compiler + the existing bench tests; the clearing paths are reasoned, not observed |
| `app/src/benchdraw.rs` | the composer's `hot` state — border to full strength, panel blended toward `th.human`, a chip replacing the LIVE tag | the palette-contrast tests main added in #555 pass over it |

It also fixed a live bug found on the way: the clipboard's `ExternalPaths` arm
joined paths raw, so a copied `Screenshot 2026-09-18.png` pasted as two words.

## How to run / verify

Everything runs from `app/`, and **there is no Cargo.toml at the repo root** —
a job started one directory up dies instantly.

```
cargo fmt -- --check
cargo clippy --locked -- -D warnings
cargo test --locked
```

Last run on the merge commit: all clean, **1337 passed / 0 failed**.

To try the gesture — **merged is not installed**: build release from main,
install as `td-<sha>-file-drop`, repoint `~/.local/bin/terminal-delight`, then
**relaunch** (a new tile is not a new process). Then:

1. Workbench face, agent running, half a sentence typed so there is a caret mid-line.
2. Drag a file from Nautilus over the composer → border full strength, panel washed, chip reads `⤓ DROP TO INSERT THE PATH`.
3. Let go → the path lands **at the caret**, the tail of the sentence survives.
4. Repeat with a name containing a space → arrives single-quoted, one word.
5. Drag over, then out of the window without dropping → highlight goes out. Same for a drop on a **sibling** pane: the pane you left must not stay lit.
6. Terminal face → the path is pasted at the prompt.
7. Drop on a card → nothing.

**If step 2 shows no highlight and step 3 types nothing**, the drag never
reached the window: that is the compositor leg, and the place to look is
whether gpui's `wl_data_device` Enter arm fires. **If the path lands at the end
of the line instead of the caret**, the composer's text layout was unavailable
at drop time and `bench_click` returned early.

## Not done / next

- **The compositor leg is unverified and cannot be verified here.** No
  `ydotool`, `xdotool` or `dragon-drop` on this box; `wtype` types keys only;
  `app/src` has no gpui test harness to inject a synthetic `FileDropEvent`.
  One human drag settles it. The overseer (pane 91) holds the script above.
- **Issue [#563](https://github.com/parker-brown-family/terminal-delight/issues/563)** —
  an image dragged off a web page does nothing, because gpui destroys any drag
  whose URIs are not local files (`client.rs:2320`) before the app is told.
  Serving it means a second patch on the pinned Zed checkout; scored 7/10 and
  not recommended. APES mirror: `decide-whether-a-drag-carrying-a-remote-uri-can-ever-reach-the-composer-github-5-mu81lwde`.
- **Left out on purpose:** a file count on the drop chip (the hover cannot know
  one), and the localised help modal (four translations for a gesture the lit
  box teaches at the moment it matters).

## Watch out

- **The shared checkout is contested.** It changed branch twice during this
  session (`chrome/phosphor-legibility` → `bench/defaults-and-dials`) with
  other agents' files modified in it. Build in your own worktree; a fresh one
  is a **cold gpui build**, about 11 minutes to `cargo check` and 12 more for
  the test binaries.
- **`ctx_read` refuses a sibling worktree** (the root jail follows cwd, not the
  repo), so every edit there costs a native `Read`. Read wide once and `grep -n`
  for anchors afterwards rather than paging back for shifted line numbers.
- **Never `on_drop` a bench element.** The bench is drawn bent and gpui
  hit-tests the flat tree; every bench gesture goes through the root listener
  and `bench_hit_at`.
- The notes UI in a decision brief has a `confirm()` behind its clear button —
  script-clicking it freezes the tab.

## Where it's recorded

- Episode: `apes/projects/terminal-delight/episodes/2026-09-19-a-file-dropped-on-the-bench.md`
- Harvest: `handoffs/2026-09-19-file-drop-into-the-composer.cdx` (253K, 6 facts, redacted)
- Reading, shipped in the repo: `reports/2026-09-18-file-drop-into-the-composer.html` (the assessment) and `reports/2026-09-19-the-drop-as-built.html` (what shipped)
- Memory: `finish-the-loop-pr-then-notify`, `a-fresh-worktree-is-a-cold-gpui-build`, `a-grep-gate-must-be-cut-before-its-own-test`
- lean-ctx: session decision recorded (`ctx_session`); `ctx_knowledge` was not bound this session
- APES kanban: the build ticket closed with its deliverable; the follow-up ticket opened and cross-linked to #563
