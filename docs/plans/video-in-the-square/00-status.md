# Video plays in the floating square, and the bench opens what its links hide

**Difficulty: 6/10.** Two halves of unequal weight. The video half is new
machinery: a native library loaded at runtime, two threads of its own, a frame
handed to gpui thirty times a second, and every one of those frames a texture
that has to be given back. A mistake there leaks GPU memory quietly or crashes
the window outright, and neither shows in a unit test. That keeps it out of the
fives. It sits behind the document view's existing one-backend-per-kind door,
touches nothing in the grid, the host or the wire, and unwinding it is deleting
one variant and two files, which keeps it out of the sevens. The bench half is
small: a Markdown link on a card draws only its words, so the run now carries
the target those words hide. One combined plan page, this one.

**Approval:** Parker, 2026-09-25, in the request itself: *"Alt click works for
image, html md NOW MP4 as well! --- Let's get to it --- touch bridge to gap, but
certainly can be done--- parity in workbench! both on artifacts AND main work
surface <3 get to!"* No gate was held for a second approval; the decisions
below are the ones he would otherwise have been asked, each with the call made.

## What he asked for

- **MP4 opens where images, HTML and Markdown already open**: Alt+click into
  the floating square, Ctrl+Alt+click beside.
- **Parity on the Workbench**, on the artifacts and on the main work surface.

What the survey found before any code: the square has floated over the bench
since `13b2253` ("The bench opens its documents in TD"), and an artifact, the
rail's deliverable row and `open_document` all ask one question —
`docopen::drawable_document` — before opening anything. So a video that
answers that question is on every one of those surfaces at once. The one hole
on the main work surface was different: a Markdown card draws
`[the clip](clip.mp4)` as "the clip", and the bench's Alt+click reads the text
it can see, so there was no path under the pointer to find. On the terminal the
same link arrives as an OSC 8 hyperlink and Alt+click already opens it.

## Decisions made without him

| Question | Call | Why |
|---|---|---|
| What plays the video? | **libmpv's software renderer**, frames drawn into TD's memory at the size shown and handed to gpui as pictures | mpv is the codecs, the clock, the sound and the scaler in one library, and its render API draws in the byte order gpui's textures use. The alternatives each missed one: an `ffmpeg` subprocess has no clock and no sound; GStreamer is a second media stack to learn; headless Chromium's `<video>` would come back as screenshots, which is the snapshot engine's speed, not a video's. |
| Linked or loaded? | **Loaded with `dlopen` when the first video opens**, through `libc`, which TD already depends on | Linking would make every TD build need libmpv just to start. Loaded, a machine without mpv runs TD as before, and a video there goes to the desktop with a sentence ("no libmpv · opened on the desktop"), the way an HTML page does with no Chromium. No new crate. |
| Plays how? | **Starts at once, with sound, and loops** | The person clicked it: that is the gesture a browser waits for before it allows sound. Looping matches a GIF in the same square, and the clips agents make are short loops. |
| Put back by a restart? | **Paused** | Nobody asked for it this time. A pane that starts playing sound by itself after a restart is the one surprise worth designing out. |
| Keys? | **Space, ← →, M on the Document face only** | Over the terminal the square gives every key but Escape to the prompt beside it — a design rule with a test (`escape_closes_a_floating_document_before_it_reaches_the_terminal`). Space cannot become the square's without breaking typing at the prompt. The bar and a click on the picture do everything in the square. |
| Frame size? | **The size shown, in device pixels, never above the file's own** | A 4K file in a 300-pixel square costs a 300-pixel frame. A small clip in a large pane is stretched by the GPU, which is free. |
| Which files? | `.mp4 .m4v .mov .webm .mkv .ogv .avi`, and only when the first bytes carry that container's signature. **Never by bytes alone.** | A log named `.mp4` would open a square that never draws. An HEIC photo opens with the same `ftyp` box an MP4 does, so an extensionless file is never guessed to be a video. |
| Markdown links on bench cards: draw them differently? | **No — same accent, no underline.** They became Alt/Ctrl+Alt/Ctrl-clickable, and a held Alt outlines the words that will open. | A plain press on a card still arms the composer. Underlining every link on every card is a visual change to the whole bench that the request did not ask for. |

## Slices

- [x] 1 · `DocKind::Video`, recognised by name and container signature (`docopen.rs`)
- [x] 2 · `docview/mpv.rs`: libmpv loaded at runtime; control and render threads; frames in B G R A with the alpha made opaque; every reported number an `Option`
- [x] 3 · `docview/video.rs`: the backend — two frames held, the oldest given back deferred; the bar (▶/❚❚, time, track, sound) laid out and hit-tested from one set of constants; keys on the Document face; `hold` for a restart
- [x] 4 · The router: `engine_refused` hands a video to the desktop when there is no libmpv, as `html_refused` did for HTML
- [x] 5 · The bench: a run carries the targets its Markdown link labels hide (`benchdraw::sel_linked`, `Atom::links`); `bench_link_under` reads them first and outlines the label alone
- [x] 6 · Docs: panes-and-tabs (video), workbench (Alt / Ctrl+Alt / Ctrl on the bench), TDSP artifact section, `open_document`'s description
- [x] 7 · Verified in a hidden window. `scripts/video-check.sh` drives an agent pane's bench through the control socket and passes all eight of its checks: the film artifact's row floats the film over the bench; its player's threads run; the window's GPU memory held at 186 MB across eight seconds of playback; closing ends the threads; Alt over a Markdown label outlines the words alone; Alt+click floats the clip they hide; Ctrl+Alt+click plays it in a pane beside; nothing reaches the desktop. A second rig photographed the square and the Document face with the 1080p film and a 640×360 clip, a corrupt `.mp4` ("unrecognized file format") and a clip with no audio device ("audio output initialization failed"), and measured about 0.6 of a core for the 1080p film in the square, back to idle on close.
- [x] 7b · The bench's Alt chip hangs from whichever end of its outline has room. It hung from the right end, which was only ever right because the outline used to be a whole line; a label near a card's left edge pushed it off the bench ("lt+click").
- [ ] 8 · Land: PR, CI, merge, install

## Not done

- **Program-drawn video** (mpv `--vo=kitty` in a pane) is a different road, the
  program-pixels plan's, and nothing here changes it.
- **The rail's row label** says `open` for a video, as it does for a picture:
  only HTML and Markdown are named there today.
- **No libmpv path is not exercised by a test.** The router's refusal is the
  HTML engine's shape with a different reason, but nothing on this machine
  lacks libmpv to prove it; see the doubts in the PR.
