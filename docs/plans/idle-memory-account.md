# Idle memory account

An idle Terminal Delight window with one pane held about 166 MiB of proportional
set size and 182 MiB of GPU memory on Parker's RTX 3080 laptop, six times what
Foot holds, and nobody could say which part of that was Terminal Delight's own
choice (#953 — an idle window holds 166 MiB of its own). This page is the account
that issue asked for: every mapping of the idle window's memory and every GPU
allocation behind it, by name, with whether Terminal Delight chose it and what it
buys. It was measured on 2026-10-15 against `origin/main` at `22189ec`, and then
again after the cuts on branch `perf/idle-memory-account`.

The window is always the same one: `scripts/idle-memory.sh` launches it hidden on
a special workspace of its own, on its own `TD_SESSION`, idle for eight to ten
seconds. On this box Hyprland tiles it at 1574×948 logical pixels on the 1.6×
laptop panel, so every surface-sized GPU texture is 2518×1517 and weighs
14.57 MiB at four bytes a pixel. A second ruler stands beside Foot: an empty gpui
window holding one line of text (`app/examples/empty_window.rs`, measured with
`scripts/idle-memory.sh --empty-gpui`), the same size, so what it holds is what
any gpui application on this machine pays.

## What the issue's own test said

The issue would have been closed as explained if two things held. The first did:
of the 76 MiB of shared libraries, 98% is the GPU driver stack and its loaders,
and the rest is Wayland, X11, xkbcommon and libc — nothing there that only
Terminal Delight loads. The second did not, on either half. The window had 263
font files mapped, across dozens of families, where Foot maps one; and its
182 MiB of GPU memory was 25% over the empty gpui window's 145 MiB, outside the
20% the issue allowed. So the issue stood, and the two places it pointed at were
the fonts and the GPU.

## Where the window's memory goes

Proportional set size of the window process alone, one pane, from
`/proc/<pid>/smaps` after ten idle seconds (one probe each; the totals further down
come from repeated instrument runs). PSS of a shared library depends on what else maps it at
that moment, so RSS is given beside it.

| Part | Before PSS / RSS | After PSS / RSS | Empty gpui PSS | Who chose it | What it buys |
|---|---|---|---|---|---|
| NVIDIA driver libraries (14 files): `libnvidia-gpucomp` 26.2, `libnvidia-glcore` 9.6–11.7, `libnvidia-eglcore` 6.7–8.0, `libnvidia-glvkspirv` 2.9, `libGLX_nvidia` 1.0, the rest under 1 | 51.3 / 54.0 | 51.5 / 54.3 | 51.6 | paid: the Vulkan driver | Vulkan on this GPU. `eglcore` is there only for wgpu's GL backend (see the findings) |
| Mesa and LLVM (22 files): `libLLVM` 18.4, `libgallium` 3.1, `libEGL_mesa` 0.3, and their dependencies | 22.7 / 24.2 | 22.7 / 24.2 | 22.6 | paid: drivers installed on the machine | nothing here; this laptop has no AMD or Intel GPU (see the findings) |
| The binary, `terminal-delight` | 25.2 / 29.5 | 25.2 / 29.4 | 15.9 | Terminal Delight | its code and data: 29.4 of the 41.9 MiB it maps are resident, against the empty gpui binary's 15.9 PSS |
| `[heap]` | 24.5 / 24.5 | 20.3 / 20.3 | 14.2 | 14.2 paid (driver and gpui, as in the empty window); the rest Terminal Delight's | about 4.2 MiB of it was the parsed tables of ~250 fonts the font search had opened; what is left of Terminal Delight's 6 MiB — terminal grid and scrollback, session, theme, decoded images — is not split further: unknown |
| Font files | 28.4 / 29.4 (263 files) | 4.0 / 4.7 (13 files) | 0.3 (2 files) | Terminal Delight, both before and after | before, mostly nothing. After: the grid's JetBrainsMono Nerd Font (4 faces, because gpui loads a family whole), Adwaita Sans for the chrome (2), Noto Sans CJK Regular and Bold (the logo placeholder's plus), Noto Color Emoji, Adwaita Mono, Noto Sans, Noto Sans Mono, Noto Sans Symbols |
| GPU driver mappings, `/dev/nvidiactl` and `/dev/nvidia0` | 12.8 / 12.8 | 12.8 / 12.8 | 12.8 | paid: the driver | the driver's shared control memory |
| Anonymous mappings | 7.0 / 7.0 | 7.0 / 7.0 | 3.7 | 3.7 paid; 3.3 Terminal Delight's | Terminal Delight's 3.3 is not split further: unknown |
| Wayland, X11, xkbcommon (18 files) | 0.9 / 1.7 | 0.9 / 1.8 | 0.9 | paid | the window system |
| Vulkan and EGL loaders (3) | 0.8 / 0.9 | 0.8 / 0.9 | 0.8 | paid | |
| libc, libm, libstdc++, ld.so (9) | 0.2 / 4.5 | 0.2 / 4.5 | 0.2 | paid | |
| dbus, systemd, stacks, vdso | 0.3 / 0.8 | 0.2 / 0.8 | 0.3 | paid | |
| **Total** | **174.0 / 189.4** | **145.6 / 160.6** | **123.4 / 133.6** | | |

Every mapping is in a row. The two rows not attributed to a file or a driver are
the heap and the anonymous arenas, and of those the share the empty gpui window
also holds (17.9 MiB) is attributed by that measurement; what remains unsplit is
Terminal Delight's 9.4 MiB, 6.5% of the window.

## Where the GPU memory goes

From the wgpu allocations themselves: a private build of the same wgpu logged
every texture and buffer gpui creates and every block of device memory its
allocator takes, and the swapchain's image count, and `nvidia-smi` gave the
process total. The difference between the total and everything wgpu allocated is
the swapchain and the driver's own context.

| Part | Before | After | Who chose it | What it buys |
|---|---|---|---|---|
| `path_msaa`, 2518×1517 BGRA at 4× MSAA | 58.29 | — | paid: gpui creates it on the first frame | antialiased vector paths. Terminal Delight draws a path only for a sticky note's paper |
| `path_intermediate`, 2518×1517 BGRA | 14.57 | — | paid: gpui, same | the same |
| Swapchain, 3 images of 2518×1517 BGRA (Mailbox, frame latency 2; count and size logged, bytes computed at four a pixel) | 43.71 | 43.71 | paid: gpui's Wayland surface | putting frames on screen |
| Glyph and image atlases: subpixel text 4.00, polychrome 4.00, grayscale text 1.00 (each 1024×1024) | 9.00 | 9.00 | 4.00 paid (any gpui text); 5.00 Terminal Delight's | the polychrome atlas holds images and colour emoji (Noto Color Emoji is mapped, so the chrome draws at least one); which elements fill it, and which draw grayscale text: unknown |
| Instance buffer | 2.00 | 2.00 | paid: gpui | per-frame primitive data |
| Host-visible staging block | 4.00 | 4.00 | paid: wgpu | uploads into the atlases |
| Unused space inside the allocator's blocks | 36.14 | 13.00 | paid: wgpu's block policy (8 MiB blocks growing to 64) | nothing. Its size follows from the order and size of the allocations above, which is why it shrank |
| Driver context: the Vulkan device, and the EGL context of wgpu's GL backend (about 3 MiB of it) | 14.29 | 14.29 | paid | the device itself |
| **Total, `nvidia-smi`** | **182** | **86** | | |

The CRT pass allocates nothing here: the hidden window never ran it, and neither
its texture nor its parameter buffer was ever created. Why it does not run there
is unknown; a new session's CRT switch being off is the likely reason. A window
running it adds one surface-sized texture, 14.57 MiB at this size — computed from
`ensure_texture` in the CRT patch, not measured.

## What was cut

Three changes, each drawing the same pixels as before.

**The font search.** Two glyphs on the idle chrome have no shape in the UI font,
Adwaita Sans: the logo placeholder's fullwidth plus (U+FF0B) and the agent
counter's robot (U+F06A9, a Nerd Font icon). For each, cosmic-text went through
every installed font until one had it — 242 fonts for the plus, 233 for the robot
— and kept every one it tried mapped and parsed for the life of the process. A
private build of cosmic-text logged the walks and the face each one ended on. Both
glyphs now name a family: the robot comes from the grid's resolved Nerd Font,
which is where the search ended, and the plus from Noto Sans Mono CJK JP, whose
U+FF0B is the same glyph as the Noto Sans CJK JP face the search ended on (same
glyph id, advance and vertical metrics, and a byte-identical swash rasterisation
at 13 px times every scale from 1 to 2.5) but which lives in two files instead of
seven. `pane::chrome_glyph_family` holds the rule.

**The grid's fallback chain.** A grid font carrying the CJK and Devanagari
fallback chain makes gpui load every face of every family in it the moment the
font is resolved, and the per-frame cell measurement resolved it before any row
had drawn a glyph. The chain only ever changes a glyph the run's own font lacks,
so `pane::FallbackLatch` now withholds it until the first frame whose rows hold
such a glyph, puts it on every run of that frame, and from then on the grid
carries it as before. A pane that prints `中文` still gets it; that was checked in
a hidden window, where the seven CJK weights load the moment the pane prints.

**The path render targets.** gpui made its two path targets on the first frame
whether or not a path was ever drawn. `docs/patches/0006-lazy-path-targets.patch`
makes them on the first frame that draws one. The empty gpui ruler with one
triangle drawn creates both again and returns to 145 MiB, so a sticky note still
gets them. This is the seventh patch on the gpui pin; `scripts/prepare-gpui.sh`
applies it, so a local build only has it after that script is run again on the
`zed-upstream` checkout.

Totals from `scripts/idle-memory.sh --panes 4`, window plus session host plus
shells, in MiB:

| | Runs | 1 pane RSS | 1 pane PSS | 4 panes RSS | 4 panes PSS | GPU | Font files |
|---|---|---|---|---|---|---|---|
| Before, `22189ec` | 4 | 208–210 | 171–176 | 242–244 | 192–198 | 182 | 263 |
| Font cuts only (gpui as checked out locally) | 5 of 6 | 181–185 | 152–155 | 213–219 | 173–176 | 182 | 13 |
| Font cuts and the path patch (as CI builds it) | 5 | 182–183 | 153–154 | 215–216 | 174–175 | 86 | 13 |
| Empty gpui window, one line of text | 7 | 133 | 90 | | | 145, or 70 with the path patch | 2 |
| Foot 1.28.0 | 13 | 33 | 18 | | | | |

The font-cuts row leaves out one run that read 170 / 144 with one pane and
208 / 167 with four, eleven below every other run of the same binary; why is
unknown. The empty gpui window and Foot were the same in every run that measured
them.

## What was found and left alone

These would save memory too, and each one changes something a person might care
about, or reaches past Terminal Delight, so each is Parker's to decide.

- **wgpu's GL backend.** gpui creates its wgpu instance with both Vulkan and GL,
  so on a Vulkan machine it still opens EGL, loads every EGL vendor and holds a GL
  context. With the EGL vendors hidden from the process the window held 18.6 MiB
  less PSS and 3 MiB less GPU memory. Trying Vulkan alone first and GL only when
  no Vulkan adapter passes would keep the fallback, but it changes adapter
  selection on hybrid machines, and this has only been run on one NVIDIA box.
- **Mesa and LLVM on an NVIDIA-only laptop.** 22.7 MiB of PSS and about 26 of RSS
  come from drivers for hardware this machine does not have, loaded twice over:
  by the GL backend through Mesa's EGL vendor, and by the Vulkan loader through
  the installed radeon ICD. Restricting either one alone left `libLLVM` mapped;
  restricting both (`VK_DRIVER_FILES` and `__EGL_VENDOR_LIBRARY_FILENAMES`, set to
  NVIDIA's files) took the window from 144.8 to 122.0 MiB PSS. Terminal Delight
  setting those would break AMD and Intel machines; this is a packaging or
  desktop setting.
- **The plus glyph itself.** An ASCII `+` in the UI font instead of the fullwidth
  plus would drop the two CJK files, about 1.7 MiB, and it would look different.
- **The swapchain's third image.** Mailbox with three images is gpui's choice for
  Wayland; FIFO with two would save 14.6 MiB here and changes frame latency,
  which the latency work owns.
- **The CRT texture once the CRT is off.** The CRT patch makes its surface-sized
  texture the first time the pass runs and keeps it after the effect is switched
  off. Dropping it whenever the pass is idle would reallocate 14.6 MiB every time
  a hover card flattens the glass for a frame, so it needs a delay before it lets
  go.
- **Whole families.** gpui loads every face of a family it resolves, so the grid's
  italic and bold-italic faces and Adwaita Sans Italic are mapped though nothing
  idle draws them, about 0.6 MiB.
- **The allocator's block policy.** 13 MiB of the GPU figure is empty space inside
  wgpu's blocks; a fixed smaller block size would trade it for more allocations.

## What could not be measured

- How Terminal Delight's own 6 MiB of heap and 3.3 MiB of arenas divide between
  the grid, scrollback, session state and decoded images: unknown. No heap
  profiler is installed here.
- Which element draws into the grayscale glyph atlas: unknown.
- The split of the 14.3 MiB driver context between the Vulkan device and the EGL
  context, beyond the 3 MiB the EGL-hidden run moved: unknown.
- The CRT texture in a window that runs the pass: computed (14.57 MiB at this
  size), not measured, because the hidden window never ran it.
- Any machine other than this one: unknown. The account is of one RTX 3080 laptop
  on Hyprland at 1.6× scale.

## Reproduce

```bash
scripts/idle-memory.sh --panes 4 --empty-gpui
```

The ruler has to be built first, from `app/`:

```bash
cargo build --release --example empty_window
```

The GPU table and the font walks came from builds that are not in the tree:
private copies of wgpu, gpu-allocator and cosmic-text, patched in through
`[patch]` for the run and removed after it, each printing one line per texture,
buffer, memory block, swapchain or fallback font when `TD_EXP_GPULOG` or
`TD_EXP_WALKLOG` was set. Rebuilding them is the way to check this table again.
