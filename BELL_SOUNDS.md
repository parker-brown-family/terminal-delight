# Agent-finished bell — per-terminal sound system

Shipped in **0.2.0**. Default sounds are bundled in the AppImage and seeded on
first run; playback is via the host `ffplay` (install `ffmpeg` to hear it).

When a program rings the terminal bell (BEL) — which agents like Claude/Codex do
when they finish — the pane:

1. **plays this pane's configured sound** (trimmed clip, optional loop),
2. raises a **SNOOZE bar** across the top of that sub-terminal, and
3. keeps an **always-visible `♪` bell toggle** in the header to mute the pane in one click.

Every terminal configures its own sound independently.

## Controls
- **Header `♪`** — always-visible, one click mutes/unmutes this pane's bell (stops any current sound). Dim = muted.
- **Header `+`** — opens the BELL config tray.
- **SNOOZE bar** (top of the pane while ringing) — **SNOOZE** (silence + lower the bar) · **MUTE** (mute the pane).
- **Config tray** — pick a sound from the list (or *default alert*) · **START/END** steppers trim the clip (the two "scrubbers") · **↻ loop** toggle · **vol −/+** · **▶ preview** / **■ stop**.

## Sounds
User sounds live in `~/.config/terminal-delight/sounds/` — **drop any mp3/ogg/wav/flac there** and it appears in the picker. Bundled defaults are seeded on first run from `app/assets/sounds/`.

| File | Piece | License |
|------|-------|---------|
| `alert.mp3` | synth two-tone chime (gentle default) | original / public domain (generated) |
| `fur-elise.mp3` | Beethoven — Für Elise (0–10s) | **CC0** (Wikimedia "Gaodifan") |
| `fate.mp3` | Beethoven — Symphony No. 5 opening (0–9s) | **PD** (Musopen/Skidmore) — the dramatic "dun dun dun DUNNN" |
| `moonlight.mp3` | Beethoven — Moonlight Sonata 1st mvt (0–13s) | **PD** (Musopen/Pitman) |
| `bald-mountain.mp3` | Mussorgsky — Night on Bald Mountain (0–11s) | **PD** (Musopen) |
| `wild-eep.mp3` | classic Mac OS "Wild Eep" alert | **Apple-owned — NOT bundled.** Present only in your local sounds dir for personal use; never committed/redistributed. |

Notes from sourcing research:
- **Zarathustra (2001)** — no clean PD/CC0 *recording* exists (composition was under EU copyright to ~2020; archive.org copies are unlicensed commercial rips). **"Fate" stands in** as the dramatic opener. Record your own if you want the real fanfare.
- **Une larme (Mussorgsky)** — only recording online is CC BY-NC-SA + hotlink-blocked; not bundled.
- **Wild Eep** — System 7 era, raw vocal "eep" by Lora Wray, surfaced via Jim Reekes' sound team. Apple's property → personal use only.

## Engine
Playback shells out to **`ffplay`** (ffmpeg) — `-ss/-t` trim, `-loop 0`, `-volume`, `-nodisp -autoexit`, spawned in its own session and **hard-killed** (`Child::kill`) on SNOOZE/mute/drop. `ffprobe` gives clip durations for the trim track. No in-process audio deps. Verified on this machine (PipeWire/PulseAudio).

## Known follow-ups (not done tonight)
- **Persistence** — per-pane bell config is per-session (not yet saved to the state file). Easy follow-up: add it to the saved leaf.
- **Draggable scrubber handles** — trim is via START/END steppers (±0.5s) for now; drag-handles are a polish pass.
- **File browser** — picker lists `~/.config/terminal-delight/sounds/`; a zenity "Browse…" for arbitrary paths is a nice add.
