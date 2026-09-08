#!/usr/bin/env node
/**
 * td-shorts — the local half of the Terminal Delight shorts pipeline.
 *
 * The shot board (docs/media/shot-list.html, published as an artifact) is where
 * a clip is judged and where posting is tracked. It cannot record a voice, and it
 * should not try: a microphone take has to land on this disk, aligned, next to the
 * master it belongs to. So the board stays the ledger and this is the workshop.
 *
 *   node scripts/td-shorts.mjs          # http://127.0.0.1:8901
 *
 * The loop it serves:
 *
 *   clips/<slug>.mp4        the master, from scripts/stage-clip.sh
 *   scripts/<slug>.md       what to say, one line per card (authored by hand)
 *      ↓  say.html — plays the clip, records you saying it, aligned
 *   voice/<slug>.webm       the take, head-trimmed to frame zero
 *      ↓  mix
 *   to-upload/<slug>.mp4    voice over picture, padded to 1080x1920
 *      ↓  post, then light the platform up on the board
 *
 * ALIGNMENT is the only part that needs care, and it is lifted from the say-along
 * spec in agent-playhouse (film/2026-09-03-say-along-spec.md), which was written
 * after three terminal-teleprompter takes drifted 1.74s. The rule: never assume
 * recorder and video start together. Arm the recorder, wait for its `start`, then
 * play; measure the gap against the video's own clock and trim exactly that much.
 * The page measures it because the page owns both clocks.
 *
 * Loopback only. It writes into docs/media/ and nowhere else, and every path it
 * writes is built from a slug it already knows — never from anything in a request.
 */
import { createServer } from "node:http";
import { readFile, writeFile, readdir, mkdir, stat, unlink } from "node:fs/promises";
import { createReadStream } from "node:fs";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";
import path from "node:path";

const run = promisify(execFile);
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const MEDIA = path.join(ROOT, "docs", "media");
const DIRS = {
  clips: path.join(MEDIA, "clips"),
  scripts: path.join(MEDIA, "scripts"),
  voice: path.join(MEDIA, "voice"),
  upload: path.join(MEDIA, "to-upload"),
  posted: path.join(MEDIA, "posted"),
};
const PORT = Number(process.env.TD_SHORTS_PORT || 8901);
const MAX_TAKE = 40 * 1024 * 1024;          // a 15s take is ~200KB; this is a wall

const json = (res, code, body) => {
  res.writeHead(code, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
};
const bad = (res, code, error) => json(res, code, { error });

/** A slug is ours or it does not exist. No path ever comes from a request. */
const SLUG = /^[a-z0-9][a-z0-9-]{0,60}$/;

/** Newest master per slug: agent-wall-20260907-203925.mp4 → "agent-wall". */
async function shots() {
  const files = (await readdir(DIRS.clips).catch(() => []))
    .filter((f) => f.endsWith(".mp4"));
  const bySlug = new Map();
  for (const f of files) {
    const slug = f.replace(/-\d{8}-\d{6}\.mp4$/, "").replace(/\.mp4$/, "");
    if (!SLUG.test(slug)) continue;
    const prev = bySlug.get(slug);
    if (!prev || f > prev.file) bySlug.set(slug, { slug, file: f });
  }
  const out = [];
  for (const s of bySlug.values()) {
    const master = path.join(DIRS.clips, s.file);
    const [meta, script, voice, staged] = await Promise.all([
      probe(master),
      readFile(path.join(DIRS.scripts, `${s.slug}.md`), "utf8").catch(() => null),
      stat(path.join(DIRS.voice, `${s.slug}.webm`)).catch(() => null),
      stat(path.join(DIRS.upload, `${s.slug}.mp4`)).catch(() => null),
    ]);
    out.push({
      ...s,
      seconds: meta.seconds,
      width: meta.width,
      height: meta.height,
      cards: script ? cards(script) : [],
      hasScript: !!script,
      hasVoice: !!voice,
      staged: !!staged,
    });
  }
  return out.sort((a, b) => a.slug.localeCompare(b.slug));
}

/** Cards are the lines you say — one per non-empty, non-heading line. */
const cards = (md) =>
  md.split("\n").map((l) => l.trim())
    .filter((l) => l && !l.startsWith("#") && !l.startsWith(">"))
    .map((l) => l.replace(/^[-*]\s+/, ""));

async function probe(file) {
  try {
    const { stdout } = await run("ffprobe", [
      "-v", "error", "-show_entries", "format=duration:stream=width,height",
      "-of", "default=nw=1", file,
    ]);
    const g = (k) => Number((stdout.match(new RegExp(`^${k}=(.+)$`, "m")) || [])[1]);
    return { seconds: g("duration") || 0, width: g("width") || 0, height: g("height") || 0 };
  } catch { return { seconds: 0, width: 0, height: 0 }; }
}

/** Stream a file with range support, so <video> can seek and scrub. */
async function serveFile(req, res, file, type) {
  const info = await stat(file).catch(() => null);
  if (!info) return bad(res, 404, "no such file");
  const range = req.headers.range;
  if (range) {
    const m = /bytes=(\d*)-(\d*)/.exec(range);
    const start = m[1] ? Number(m[1]) : 0;
    const end = m[2] ? Number(m[2]) : info.size - 1;
    res.writeHead(206, {
      "content-type": type,
      "content-range": `bytes ${start}-${end}/${info.size}`,
      "accept-ranges": "bytes",
      "content-length": end - start + 1,
    });
    return createReadStream(file, { start, end }).pipe(res);
  }
  res.writeHead(200, { "content-type": type, "content-length": info.size, "accept-ranges": "bytes" });
  createReadStream(file).pipe(res);
}

async function body(req, limit) {
  const chunks = [];
  let size = 0;
  for await (const c of req) {
    size += c.length;
    if (size > limit) throw new Error("too large");
    chunks.push(c);
  }
  return Buffer.concat(chunks);
}

/**
 * Write a take. `offsetMs` is how far the recording started BEFORE frame zero of
 * the picture, measured in the page against the video's own clock — trimming it
 * here is what makes the voice line up with what it is describing.
 */
async function saveVoice(req, res, url) {
  const slug = url.searchParams.get("slug") || "";
  const offsetMs = Math.max(0, Math.min(60000, Number(url.searchParams.get("offsetMs") || 0)));
  if (!SLUG.test(slug)) return bad(res, 400, "bad slug");
  const known = (await shots()).find((s) => s.slug === slug);
  if (!known) return bad(res, 404, `no master for ${slug} — film it first`);

  let buf;
  try { buf = await body(req, MAX_TAKE); }
  catch { return bad(res, 413, "take too large"); }
  if (!buf.length) return bad(res, 400, "empty take — the microphone produced no audio");

  await mkdir(DIRS.voice, { recursive: true });
  const raw = path.join(DIRS.voice, `${slug}.raw.webm`);
  const out = path.join(DIRS.voice, `${slug}.webm`);
  await writeFile(raw, buf);
  try {
    // Trim the head, and only the head — re-encode to Opus so the cut is exact
    // rather than landing on the previous keyframe.
    await run("ffmpeg", ["-v", "error", "-y", "-i", raw, "-ss", (offsetMs / 1000).toFixed(3),
                         "-c:a", "libopus", "-b:a", "96k", out]);
  } catch (e) {
    return bad(res, 500, `ffmpeg could not trim the take: ${e.message}`);
  }
  await unlink(raw).catch(() => {});
  const info = await stat(out);
  const dur = (await probe(out)).seconds;
  json(res, 200, {
    file: path.relative(ROOT, out),
    bytes: info.size,
    seconds: dur,
    trimmed: offsetMs / 1000,
  });
}

/**
 * Mix: the master picture, the take over it, padded to a 1080x1920 short.
 *
 * The clip's own audio is silent (the capture rig records no sound), so there is
 * nothing to duck — the voice is the whole track. Loudness is normalised to
 * -16 LUFS, which is what every platform re-encodes toward anyway.
 */
async function mix(req, res, url) {
  const slug = url.searchParams.get("slug") || "";
  if (!SLUG.test(slug)) return bad(res, 400, "bad slug");
  const s = (await shots()).find((x) => x.slug === slug);
  if (!s) return bad(res, 404, "no master");
  const voice = path.join(DIRS.voice, `${slug}.webm`);
  if (!(await stat(voice).catch(() => null))) return bad(res, 404, "no take recorded yet");

  await mkdir(DIRS.upload, { recursive: true });
  const out = path.join(DIRS.upload, `${slug}.mp4`);
  const vf = [
    "scale=1080:-2:flags=lanczos",
    "pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=0x17130E",   // the board's own ground
    "setsar=1",
  ].join(",");
  try {
    await run("ffmpeg", ["-v", "error", "-y",
      "-i", path.join(DIRS.clips, s.file), "-i", voice,
      "-filter_complex", `[0:v]${vf}[v];[1:a]loudnorm=I=-16:TP=-1.5:LRA=11[a]`,
      "-map", "[v]", "-map", "[a]",
      "-c:v", "libx264", "-preset", "slow", "-crf", "20", "-pix_fmt", "yuv420p",
      "-c:a", "aac", "-b:a", "160k", "-shortest", "-movflags", "+faststart", out,
    ], { maxBuffer: 1 << 24 });
  } catch (e) {
    return bad(res, 500, `mix failed: ${e.message}`);
  }
  const info = await stat(out);
  json(res, 200, { file: path.relative(ROOT, out), bytes: info.size, seconds: (await probe(out)).seconds });
}

const server = createServer(async (req, res) => {
  const url = new URL(req.url, `http://127.0.0.1:${PORT}`);
  try {
    if (url.pathname === "/" || url.pathname === "/say.html")
      return serveFile(req, res, path.join(MEDIA, "say.html"), "text/html; charset=utf-8");

    if (url.pathname === "/api/shots") return json(res, 200, { shots: await shots() });

    if (url.pathname === "/api/clip") {
      const slug = url.searchParams.get("slug") || "";
      if (!SLUG.test(slug)) return bad(res, 400, "bad slug");
      const s = (await shots()).find((x) => x.slug === slug);
      if (!s) return bad(res, 404, "no master");
      const staged = url.searchParams.get("mixed") === "1";
      const file = staged ? path.join(DIRS.upload, `${slug}.mp4`) : path.join(DIRS.clips, s.file);
      return serveFile(req, res, file, "video/mp4");
    }

    if (url.pathname === "/api/voice" && req.method === "POST") return saveVoice(req, res, url);
    if (url.pathname === "/api/mix" && req.method === "POST") return mix(req, res, url);

    bad(res, 404, "no such endpoint");
  } catch (e) {
    bad(res, 500, e.message);
  }
});

await Promise.all(Object.values(DIRS).map((d) => mkdir(d, { recursive: true })));
server.listen(PORT, "127.0.0.1", async () => {
  const list = await shots();
  console.log(`td-shorts · http://127.0.0.1:${PORT}`);
  for (const s of list) {
    console.log(`  ${s.slug.padEnd(20)} ${s.seconds.toFixed(1)}s  ` +
      `${s.hasScript ? `${s.cards.length} cards` : "NO SCRIPT"}` +
      `${s.hasVoice ? " · voiced" : ""}${s.staged ? " · staged" : ""}`);
  }
  if (!list.length) console.log("  (no clips yet — film one with scripts/stage-clip.sh)");
});
