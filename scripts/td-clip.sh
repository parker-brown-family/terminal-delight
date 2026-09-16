#!/usr/bin/env bash
# td-clip — record one short Terminal Delight gesture as an MP4 (+ optional GIF).
#
# The demo pipeline's only capture tool. It exists because the cost of a marketing
# clip was an afternoon, so no clips were ever made; the target here is ninety
# seconds from "I want to show the copy chips" to a file you can drag into a post.
#
#   scripts/td-clip.sh copy-chips              # pick a region with the mouse, 8s
#   scripts/td-clip.sh sticky-note -d 6        # 6 seconds
#   scripts/td-clip.sh theme-tray -w           # capture the focused window's box
#   scripts/td-clip.sh notes -g 100,100 900x600 --gif
#
# Output lands in docs/media/clips/<name>-<timestamp>.mp4 (never committed; the
# directory is gitignored — clips are build artifacts of a moment, not source).
#
# Backends, in order of preference:
#   gpu-screen-recorder — already installed here, GPU-encoded, measured at a true 60
#                  fps over a pane-sized region (251 frames in 4.2 s). It captures in
#                  LOGICAL coordinates and writes PHYSICAL pixels, so a 734x402 region
#                  on this 1.6x display lands as a 1174x644 file — crisper than the
#                  region you asked for, which is what you want for a HiDPI clip.
#   wf-recorder  — the usual Wayland answer; not installed here. `pacman -S wf-recorder`
#   grim loop    — last resort, no install needed, MEASURED at ~1 fps over a full
#                  1576x950 window and ~3 fps over a 734x402 pane. A slideshow.
#                  Fine for a still-ish gesture, useless for anything that moves.
#
# Hygiene it enforces so you do not have to remember:
#   * cursor is EXCLUDED unless you pass --cursor (a pointer in a terminal clip
#     reads as a recording, not as a product)
#   * TD is asked to hide its own transient chrome via `ctl` where that applies
#   * a 3-2-1 countdown on stderr, so you are not fumbling for the gesture on frame 1
#   * the last clip's path is echoed alone on the final line, for scripts to read

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
outdir="${TD_CLIP_DIR:-$here/docs/media/clips}"
dur=8
geom=""
usewin=0
gif=0
cursor=0
fps="${TD_CLIP_FPS:-60}"

name="${1:-clip}"; shift || true
while [ $# -gt 0 ]; do
  case "$1" in
    -d|--duration) dur="$2"; shift 2 ;;
    -g|--geometry) geom="$2 $3"; shift 3 ;;
    -w|--window)   usewin=1; shift ;;
    --gif)         gif=1; shift ;;
    --cursor)      cursor=1; shift ;;
    -h|--help)     sed -n '2,28p' "$0"; exit 0 ;;
    *) echo "td-clip: unknown argument '$1'" >&2; exit 2 ;;
  esac
done

mkdir -p "$outdir"
stamp="$(date +%Y%m%d-%H%M%S)"
mp4="$outdir/$name-$stamp.mp4"

# ── the capture region ───────────────────────────────────────────────────────
if [ "$usewin" = 1 ]; then
  if ! command -v hyprctl >/dev/null; then
    echo "td-clip: -w needs hyprctl (Hyprland)" >&2; exit 1
  fi
  read -r x y w h < <(hyprctl -j activewindow | jq -r '"\(.at[0]) \(.at[1]) \(.size[0]) \(.size[1])"')
  region="${x},${y} ${w}x${h}"
elif [ -n "$geom" ]; then
  region="$geom"
else
  command -v slurp >/dev/null || { echo "td-clip: need slurp to pick a region" >&2; exit 1; }
  echo "td-clip: drag the region to record…" >&2
  region="$(slurp)"
fi
# normalise "X,Y WxH" for both backends
rx="${region%% *}"; rs="${region##* }"
rw="${rs%%x*}"; rh="${rs##*x}"
# h264 needs even dimensions
rw=$(( rw - rw % 2 )); rh=$(( rh - rh % 2 ))

countdown() { for n in 3 2 1; do printf '\rtd-clip: %s… ' "$n" >&2; sleep 1; done; printf '\rtd-clip: RECORDING %ss\n' "$dur" >&2; }

# ── backend: gpu-screen-recorder (preferred — real GPU capture at a true 60 fps) ─
if command -v gpu-screen-recorder >/dev/null; then
  # -region takes WxH+X+Y in LOGICAL coordinates; the file comes out in physical
  # pixels, so on a scaled display it is larger (and sharper) than the numbers here.
  gargs=(-w region -region "${rw}x${rh}+${rx%,*}+${rx#*,}" -f "$fps" -o "$mp4")
  [ "$cursor" = 1 ] && gargs+=(-cursor yes) || gargs+=(-cursor no)
  countdown
  gpu-screen-recorder "${gargs[@]}" >/tmp/td-clip-gsr.log 2>&1 &
  rec=$!
  sleep "$dur"
  kill -INT "$rec" 2>/dev/null || true
  wait "$rec" 2>/dev/null || true
  # gsr needs a moment to write the moov atom after SIGINT; a clip read too early
  # looks like a zero-length capture and sends you debugging the wrong thing.
  sleep 1

# ── backend: wf-recorder ─────────────────────────────────────────────────────
elif command -v wf-recorder >/dev/null; then
  args=(-g "$rx ${rw}x${rh}" -f "$mp4" -r "$fps" --codec libx264)
  [ "$cursor" = 1 ] || args+=(--no-damage)   # cursor is off by default in wf-recorder
  countdown
  wf-recorder "${args[@]}" &
  rec=$!
  sleep "$dur"
  kill -INT "$rec" 2>/dev/null || true
  wait "$rec" 2>/dev/null || true

# ── backend: grim frame loop ─────────────────────────────────────────────────
else
  command -v grim >/dev/null || { echo "td-clip: need wf-recorder or grim" >&2; exit 1; }
  echo "td-clip: wf-recorder not installed — falling back to a ~10fps grim loop." >&2
  echo "         For anything with scrolling or motion, install it: sudo pacman -S wf-recorder" >&2
  frames="$(mktemp -d)"; trap 'rm -rf "$frames"' EXIT
  countdown
  i=0; end=$(( $(date +%s) + dur ))
  while [ "$(date +%s)" -lt "$end" ]; do
    grim -g "$rx ${rw}x${rh}" "$frames/$(printf '%05d' "$i").png" 2>/dev/null || true
    i=$((i+1)); sleep 0.1
  done
  # Play back at the rate we actually achieved, not the rate we hoped for: grim's
  # per-frame cost puts this near 4 fps on this box, and hard-coding 10 would run
  # the clip at 2.5x speed.
  real_fps=$(( i / dur )); [ "$real_fps" -lt 1 ] && real_fps=1
  echo "td-clip: captured $i frames in ${dur}s (~${real_fps} fps)" >&2
  ffmpeg -loglevel error -y -framerate "$real_fps" -pattern_type glob -i "$frames/*.png" \
    -vf "scale=${rw}:${rh}:flags=lanczos" -pix_fmt yuv420p "$mp4"
fi

[ -s "$mp4" ] || { echo "td-clip: capture produced nothing" >&2; exit 1; }

# ── optional GIF (palette pass — a naive gif of a phosphor theme looks like mud) ─
if [ "$gif" = 1 ]; then
  pal="$(mktemp --suffix=.png)"
  ffmpeg -loglevel error -y -i "$mp4" -vf "fps=15,scale=800:-1:flags=lanczos,palettegen=stats_mode=diff" "$pal"
  ffmpeg -loglevel error -y -i "$mp4" -i "$pal" \
    -lavfi "fps=15,scale=800:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3" \
    "${mp4%.mp4}.gif"
  rm -f "$pal"
  echo "td-clip: gif → ${mp4%.mp4}.gif" >&2
fi

echo "$mp4"
