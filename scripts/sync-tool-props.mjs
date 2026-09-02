#!/usr/bin/env node
// Bake agent-playhouse's tool props into assets this binary can embed.
//
// The vocabulary — which tool is which drawing, in which colour — is the
// playhouse's, decided by counting 9,320 real tool calls rather than by taste.
// Terminal Delight is the second consumer of it, and a second HAND-MAINTAINED
// copy of a fifty-row table is a fork waiting to drift. So it is generated:
// this script reads the playhouse's own `web/skin.json`, `web/art/props.svg`
// and the `.pa-*` rules out of `web/playhouse.css`, and writes
//
//   app/assets/tool-props.json      the props table, verbatim + provenance
//   app/assets/img/props/<art>.png  one 128² plate per drawing, tint baked in
//
// Baked rather than referenced because gpui renders images from paths and does
// not read SVG or CSS custom properties, and because a deployed `td-<sha>`
// has to carry its own art the way it already carries its own mascot.
//
//   node scripts/sync-tool-props.mjs [path-to-agent-playhouse]
//   node scripts/sync-tool-props.mjs --check     # drift check, no writes
//
// --check regenerates into a temp dir and diffs. A non-zero exit means the
// playhouse moved and this repo did not; re-run without --check.

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const CHECK = argv.includes("--check");
const PLAYHOUSE = resolve(
  argv.find((a) => !a.startsWith("--")) ?? process.env.PLAYHOUSE ?? join(REPO, "..", "agent-playhouse"),
);

/** The box every prop is drawn in, and the size we rasterise to. */
const BOX = 56;
const PX = 128; // the wall card draws these ~48px; 128 keeps 2.6× headroom.
const SUPER = 512; // rasterise here, trim and centre, then land on PX.
const INK = 0.72; // how much of the tile the drawing fills once centred.

// ---------------------------------------------------------------- read source

function readPlayhouse(rel) {
  try {
    return readFileSync(join(PLAYHOUSE, rel), "utf8");
  } catch (e) {
    console.error(`cannot read ${rel} under ${PLAYHOUSE}`);
    console.error(`pass the playhouse checkout: node scripts/sync-tool-props.mjs /path/to/agent-playhouse`);
    process.exit(2);
  }
}

const skin = JSON.parse(readPlayhouse("web/skin.json"));
const propsSvg = readPlayhouse("web/art/props.svg");
const css = readPlayhouse("web/playhouse.css");

/** The `.pa-*` rules, which carry every colour and stroke width the art uses. */
function paRules() {
  const out = [];
  for (const m of css.matchAll(/^\s*(\.pa-[\w-]+)\s*\{([^}]*)\}/gm)) {
    out.push([m[1], m[2].trim()]);
  }
  if (out.length < 8) {
    console.error(`only ${out.length} .pa-* rules found — playhouse.css changed shape`);
    process.exit(2);
  }
  return out;
}

/**
 * The body of `<g id="prop-NAME">…</g>`, depth-tracked so a prop containing its
 * own inner `<g>` (the radar's sweep arm) is not cut short at the first close.
 */
function propBody(name) {
  const open = propsSvg.indexOf(`<g id="prop-${name}"`);
  if (open < 0) return null;
  let i = propsSvg.indexOf(">", open) + 1;
  const start = i;
  let depth = 1;
  while (depth > 0) {
    const nextOpen = propsSvg.indexOf("<g", i);
    const nextClose = propsSvg.indexOf("</g>", i);
    if (nextClose < 0) return null;
    if (nextOpen >= 0 && nextOpen < nextClose) {
      depth++;
      i = nextOpen + 2;
    } else {
      depth--;
      if (depth === 0) return propsSvg.slice(start, nextClose);
      i = nextClose + 4;
    }
  }
  return null;
}

// ------------------------------------------------------------------ bake one

/**
 * One prop alone on transparent ground: the playhouse's own rules, inlined with
 * `--tint` resolved to a literal. No plate and no centring — a prop is drawn so
 * that its GRIP lands at (27, 17), where the robot's hand is, which means it is
 * deliberately off-centre in its own box (`web/art/CONVENTIONS.md`). Centring
 * happens after rasterising, against the ink that actually got drawn.
 */
function inkOnly(body, tint) {
  const rules = paRules()
    .map(([sel, decls]) => `${sel}{${decls.replaceAll("var(--tint)", tint)}}`)
    .join("\n    ");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${BOX} ${BOX}" width="${SUPER}" height="${SUPER}">
  <style>
    ${rules.replaceAll("var(--mono)", "ui-monospace, monospace")}
  </style>
${body.trimEnd()}
</svg>
`;
}

/**
 * The ground every prop stands on: the dark tile the drawings were designed
 * against (they use `#151824` faces and vanish on anything light) lit by the
 * same tint glow the player gives them with `drop-shadow(0 0 11px …)`.
 */
function plate(tint) {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${BOX} ${BOX}" width="${SUPER}" height="${SUPER}">
  <defs>
    <radialGradient id="glow">
      <stop offset="0" stop-color="${tint}" stop-opacity=".34"/>
      <stop offset=".62" stop-color="${tint}" stop-opacity=".10"/>
      <stop offset="1" stop-color="${tint}" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="${BOX}" height="${BOX}" rx="11" fill="#0e1017"/>
  <circle cx="28" cy="28" r="27" fill="url(#glow)"/>
</svg>
`;
}

/**
 * Rasterise, trim to the ink, scale that to `INK` of the tile and centre it.
 *
 * Optical centring is the whole reason this goes through a raster step. Left as
 * drawn, a prop composed around its grip sits low and left in a square, and
 * nineteen of them in a column look like a rack that was hung badly. Trimming
 * to the drawn pixels and centring THAT is the only way to get a set that reads
 * as one set, without editing nineteen drawings that are correct as they are.
 */
function bake(art, tint, outDir) {
  const body = propBody(art);
  if (!body) {
    console.error(`props.svg has no <g id="prop-${art}">`);
    process.exit(2);
  }
  const ink = join(outDir, `${art}.ink.png`);
  const bg = join(outDir, `${art}.plate.png`);
  const svg = join(outDir, `${art}.svg`);

  writeFileSync(svg, inkOnly(body, tint));
  execFileSync("rsvg-convert", ["-w", String(SUPER), "-h", String(SUPER), "-o", ink, svg]);
  writeFileSync(svg, plate(tint));
  execFileSync("rsvg-convert", ["-w", String(SUPER), "-h", String(SUPER), "-o", bg, svg]);

  execFileSync("magick", [
    bg,
    "(", ink,
      "-fuzz", "1%", "-trim", "+repage",
      "-resize", `${Math.round(SUPER * INK)}x${Math.round(SUPER * INK)}`,
    ")",
    "-gravity", "center", "-composite",
    "-filter", "Lanczos", "-resize", `${PX}x${PX}`,
    "-strip", "-define", "png:compression-level=9",
    join(outDir, `${art}.png`),
  ]);
  for (const f of [ink, bg, svg]) rmSync(f);
}

// ----------------------------------------------------------------- run it

const props = skin.props ?? {};
const arts = new Map(); // art -> tint (1:1 today; last row wins if that changes)
for (const row of Object.values(props)) {
  if (row?.art) arts.set(row.art, row.tint ?? "#9aa3b8");
}
// The lettered block is the honest fallback the playhouse keeps for a tool
// nobody wrote a row for; ship it even though no row names it.
if (!arts.has("tile")) arts.set("tile", "#7f8aa8");

const manifest = {
  _source: "agent-playhouse web/skin.json — regenerate with scripts/sync-tool-props.mjs, never hand-edit",
  props,
};

const stage = CHECK ? mkdtempSync(join(tmpdir(), "td-props-")) : join(REPO, "app/assets/img/props");
const manifestPath = CHECK ? join(stage, "tool-props.json") : join(REPO, "app/assets/tool-props.json");
mkdirSync(stage, { recursive: true });

for (const [art, tint] of [...arts].sort()) bake(art, tint, stage);
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");

if (!CHECK) {
  console.log(`${arts.size} props baked into app/assets/img/props/, ${Object.keys(props).length} tool rows written`);
  process.exit(0);
}

// --check: compare byte-for-byte against what is committed.
const live = join(REPO, "app/assets/img/props");
const drift = [];
for (const f of readdirSync(stage).filter((f) => f.endsWith(".png"))) {
  let a, b;
  try {
    a = readFileSync(join(stage, f));
  } catch {}
  try {
    b = readFileSync(join(live, f));
  } catch {}
  if (!b || !a?.equals(b)) drift.push(`app/assets/img/props/${f}`);
}
const wantManifest = readFileSync(manifestPath, "utf8");
let haveManifest = "";
try {
  haveManifest = readFileSync(join(REPO, "app/assets/tool-props.json"), "utf8");
} catch {}
if (wantManifest !== haveManifest) drift.push("app/assets/tool-props.json");
rmSync(stage, { recursive: true, force: true });

if (drift.length) {
  console.error("tool props have drifted from the playhouse:");
  for (const d of drift) console.error(`  ${d}`);
  console.error("run: node scripts/sync-tool-props.mjs");
  process.exit(1);
}
console.log("tool props are in sync with the playhouse");
