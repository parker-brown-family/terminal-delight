The prompt each of the six agents received, 2026-09-25. `<crate>` was `rio-vt` or `libghostty-vt`; `<who>` was "the Rio terminal's" or "the Ghostty terminal's"; `<dir>` was the agent's own workspace.

---

You are a Rust developer testing how pleasant an open-source terminal emulator library is for an AI coding agent to work with. The library is `<crate>` on crates.io: <who> emulator core (the part that parses program output into a grid of cells).

Workspace: `<dir>`
Work only inside that directory. Keep every build output and cache inside it too (for example set CARGO_TARGET_DIR to a directory inside it, and point any other tool's cache there).

This is a blind test. Do not read anything under /home/parker/Work, /home/parker/BROWN-FAMILY-SPORTS or /home/parker/.claude, and do not look for other people's harnesses or examples of feeding recordings to this library. The crate's own documentation, docs.rs, its source under ~/.cargo/registry, and its upstream repository are all fine to read.

Available: Rust stable with cargo, network access to crates.io and GitHub. If a dependency turns out to need the Zig compiler, Zig 0.15.2 is at /home/parker/.cache/td-core-research/zig-x86_64-linux-0.15.2/zig (add that directory to PATH yourself).

Task: create a Cargo binary named `probe` that
1. creates one terminal of 120 columns by 40 rows, with cells of 10 by 22 pixels, keeping 10,000 lines of scrollback history;
2. feeds it the bytes of icat.bin (in the workspace) in 4096-byte chunks. The file is the raw output of `kitten icat` drawing a PNG picture with the Kitty graphics protocol;
3. prints JSON with: every picture placement the terminal stored (image id, row, column, width in cells, height in cells), the cursor row and column, any bytes the terminal wanted to send back to the program (its replies to queries), and the non-empty visible lines of text;
4. then, in a fresh terminal with the same settings, feeds text.bin (in the workspace, about 18,000 lines of ordinary text) and prints how many rows, visible plus history, the terminal is holding afterwards.

Build it with `cargo run --release` and save the program's final JSON output as output.json in the workspace.

Also write notes.md in the workspace: every surprise, wrong turn, documentation mismatch or build problem you hit, in the order you hit it, one line each, and how long the first successful build took. Be candid. Zero surprises is a fine answer if it is true.

When you finish, reply with: whether each of the four steps works as asked, the crate version you used, and the contents of notes.md.
