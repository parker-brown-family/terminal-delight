# The agent-finished bell, and the sound credits

When a Claude Code or Codex agent in a pane finishes a turn, Terminal Delight
plays one short ping. That is the whole feature: there is no sound picker, no
volume control, no per-pane setting, and nothing to configure.

- **When it rings.** On an agent pane only: a terminal bell (BEL) while the
  agent was working, or the finish the pane detects on screen when the agent
  stops working. A bell from a shell, or from an agent that was not working,
  stays silent, because a false finish would clear the tab's badge and
  interrupt you about an agent that is still going.
- **What you see with it.** The tab's 🔔 badge, `● done` in the pane header,
  and a desktop notification when you were not already looking at that pane.
  Focusing the pane clears the badge and the status.
- **How it plays.** The clip (`app/assets/sounds/alert.mp3`, 2 KB, CC0) is
  compiled into the binary and written once per boot to
  `$XDG_RUNTIME_DIR/terminal-delight-ping.mp3`. It plays once through `ffplay`
  (from `ffmpeg`) at a fixed volume, or through PipeWire's `pw-play` when
  `ffplay` is absent. With neither installed it stays silent. A second ring
  while the first is sounding restarts it, so finishing agents never stack.

The source is `app/src/bell.rs`; the ring sites are the two calls to
`bell_player.play()` in `app/src/pane.rs`.

Versions before 0.3.0 had a per-pane sound picker, a trim scrubber, a loop
toggle and a `~/.config/terminal-delight/sounds/` directory. They were removed
because the configuration buried the point of the feature, which is knowing
when an agent finishes. A `sounds/` directory left by an older version is no
longer read.

## Sound credits

This file ships inside the AppImage as `SOUND-CREDITS.md`, because
`scripts/build-appimage.sh` still bundles every clip in `app/assets/sounds/`.
Only `alert.mp3` is played; the other four are bundled and unused.

| File | Piece | License |
|------|-------|---------|
| `alert.mp3` | synth two-tone chime (the ping) | original / public domain (generated) |
| `fur-elise.mp3` | Beethoven — Für Elise (full, 2:56) | **CC BY-SA 3.0** (Wikimedia, "Sebion7125") — attributed in THIRD-PARTY-LICENSES.md |
| `fate.mp3` | Beethoven — Symphony No. 5, i. Allegro con brio (first 5:00) | **PD** (Musopen recording, via Wikimedia Commons) |
| `moonlight.mp3` | Beethoven — Moonlight Sonata, i. Adagio sostenuto (full, 5:35) | **PD** (Musopen recording, via Wikimedia Commons) |
| `bald-mountain.mp3` | Mussorgsky — Night on Bald Mountain (first 5:00) | **PD** (Musopen recording, via Wikimedia Commons) |
