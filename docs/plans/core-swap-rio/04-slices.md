# Gate 4 — Vertical slices

**State:** self-approved 2026-09-25. Each slice ends green on the CI gates
(`cargo fmt -- --check`, `cargo clippy --locked -- -D warnings`,
`cargo test --locked`, all from `app/`) and is its own commit, so any slice can
be reverted alone.

## Slice 1 — the tracer: TD's boundary and loop, still on alacritty

Behaviour does not change, and that is the point. `vt` exists with its types,
its pump, its PTY and the alacritty adapter; every file moves onto it;
`socketpty.rs` is deleted. The whole existing suite passes on
`--features core-alacritty`. The attach fence is now one lock.

*Proves:* the boundary is complete (nothing else names a core crate), the TD
pump is a faithful replacement for alacritty's `EventLoop` (the attached tests,
the host suite and the echo bench exercise it end to end), and the frozen
numbering holds.

## Slice 2 — rio-vt becomes the core

The rio adapter, and rio-vt becomes the default. The suite runs on it; each
failure is either fixed in the adapter or recorded as a deliberate difference
with its test updated to say so (in `evidence/core-differences.md`).

*Proves:* rio-vt carries everything TD reads — text, colour, attributes, wide
characters, marks, wrap, history, selection, modes, replies — and a snapshot
from one core replays on the other.

## Slice 3 — pictures, attentionally

The pane draws `Term::pictures()` over the grid, and forgets them when hidden.
A unit test places a picture through rio-vt and reads it back; a behaviour
test covers the forget.

*Proves:* Kitty pictures from `kitten icat` and friends appear, sized and
placed in cells, and vanish on tab switch.

## Slice 4 — in a real window

A new Terminal Delight window on its own session, launched with `setsid` and a
private `TD_SESSION`, never touching Parker's host. Shell, vim, htop, `less`,
`kitten icat`, `servo-reader` inline images, and an attach to a pane owned by a
fresh session host. Photographs of each. The echo bench before and after, and
the twenty-pane run.

## Slice 5 — the words

The docs site, README, glossary, licences, contributing guide and the two
feature pages name the new core. The five-part series is drafted in the
parker-dev repository (not deployed). Follow-ups filed as issues: removing the
alacritty fallback, and anything slice 2 recorded as a difference worth
revisiting.
