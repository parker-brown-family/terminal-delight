# Program Design

## Interfaces

```rust
// pane/input_edit.rs
fn selection_keys(term: &crate::vt::Term) -> Option<Vec<u8>>;
fn line_keys(term: &crate::vt::Term) -> Option<Vec<u8>>;

// workbench.rs
enum Edit { /* existing */ KillLine, ClearInput }

// styling::Memo: live rows plus rows without TD cursor/selection colour changes.
// GridPaint::Snapshot receives both at identical painted coordinates.
// Ghost retains whether this update erased text, to select its fade curve.
```

Keep the current plain Backspace/Delete and Ctrl+X write path, widening only its selection range. Combine new chord output with that existing send call so terminal writes still use the same audited primitive. Restrict compound edits to shell/agent input and preserve the non-agent alternate-screen guard. Readline-compatible clear-input uses Ctrl+A then Ctrl+K, verified on a literal-newline buffer; it never sends Enter or Ctrl+C.

## Verification

First reproduce a mouse selection crossing a soft wrap through the GPUI/Bash harness. Cover selection with the caret before, inside and after it, a visible multiline selection, history-only selection rejection, wide cells and a large passage. Assert shell input, prompt and unaffected suffix, rather than only the emitted bytes.

Exercise every shortcut through key routing, with a note present and without one, on terminal and workbench composer. Check line-versus-area deletion on multiline buffers and undo in the local composer. Check all nine translated descriptions and native Help layout.

Compare clean ghost colours before/after selection and cursor overlays, preserve application inversion, verify erased opacity still positive at 500ms and expiry at 650ms, keep ordinary replacement at 220ms and zero exact. Native proof uses an isolated fixture, compares a still neighbor and retains original live shell identities during GUI refresh.

The least-confident boundaries are explicit-newline selections, per-program line-editor key compatibility and the dual snapshot's coordinate transforms. Test them directly and state the native coverage; arbitrary full-screen editors keep their own keys.
