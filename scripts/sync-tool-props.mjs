#!/usr/bin/env node
// Bake agent-playhouse's cast into assets this binary can embed.
//
// The vocabulary — which tool is which drawing, in which colour, doing what —
// is the playhouse's, decided by counting 9,320 real tool calls rather than by
// taste. Terminal Delight is the second consumer of it, and a second
// HAND-MAINTAINED copy of a fifty-row table is a fork waiting to drift. So it
// is generated. This script reads the playhouse's own files and writes:
//
//   app/assets/tool-props.json       the props table, verbatim + provenance
//   app/assets/img/props/<art>.png   the prop alone, 128² — for the pane header
//   app/assets/img/scenes/<art>.png  the ROBOT holding it, 256² — for the wall
//
// TWO PIPELINES, because the two assets are different kinds of thing:
//
//   • A prop is a `<g>` in `web/art/props.svg` plus the `.pa-*` rules — static
//     art we can rasterise directly with rsvg-convert.
//   • The robot is not a file anywhere. He is composed at run time from an SVG
//     rig, a stylesheet that poses him by `data-state` / `data-face`, and three
//     custom properties. Re-implementing that here would be exactly the fork
//     this pipeline exists to prevent, so instead we PHOTOGRAPH the playhouse's
//     own contact sheet, which already renders every frame from the real
//     manifest with the real stylesheet. Change a pose there, re-run this, and
//     the terminal's faces change with it.
//
// Baked rather than referenced because gpui renders images from paths and does
// not read SVG or CSS custom properties, and because a deployed `td-<sha>` has
// to carry its own art the way it already carries its own mascot.
//
//   node scripts/sync-tool-props.mjs [path-to-agent-playhouse]
//   node scripts/sync-tool-props.mjs --check     # drift check, no writes
//
// Needs `rsvg-convert`, `magick` and a `chromium` on PATH — all three are
// already here, and none of it is an npm install.

import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { shoot } from "./lib/shoot-sheet.mjs";

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const argv = process.argv.slice(2);
const CHECK = argv.includes("--check");
const PLAYHOUSE = resolve(
  argv.find((a) => !a.startsWith("--")) ?? process.env.PLAYHOUSE ?? join(REPO, "..", "agent-playhouse"),
);

/** The box every prop is drawn in, and the sizes we rasterise to. */
const BOX = 56;
const PROP_PX = 128; // the header square draws these ~18px; 128 is ample.
const SCENE_PX = 256; // the wall card draws these ~48-100px.
const SUPER = 512; // rasterise props here, trim and centre, then land on PROP_PX.
const INK = 0.72; // how much of the tile the drawing fills once centred.
/** How much of the shot scene the rig occupies BEFORE cropping. Deliberately
 *  modest: the crop below frames on the drawn content, so this only has to
 *  leave every prop room to reach without touching the scene's edge. */
const BOT_FILL = 0.58;
/** How far a re-shot scene may drift before we call it a change, 0..1 RMSE.
 *  Screenshots are not byte-reproducible across chromium builds and font
 *  rendering; a pose change is worth ~an order of magnitude more than that. */
const SCENE_TOLERANCE = 0.04;

// ---------------------------------------------------------------- read source

function readPlayhouse(rel) {
  try {
    return readFileSync(join(PLAYHOUSE, rel), "utf8");
  } catch {
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

// -------------------------------------------------------- pipeline 1: props

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
function bakeProp(art, tint, outDir) {
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
    "-filter", "Lanczos", "-resize", `${PROP_PX}x${PROP_PX}`,
    "-strip", "-define", "png:compression-level=9",
    join(outDir, `${art}.png`),
  ]);
  for (const f of [ink, bg, svg]) rmSync(f);
}

// ------------------------------------------------------- pipeline 2: scenes

/**
 * Photograph the robot holding each prop, out of the playhouse's own contact
 * sheet — see `lib/shoot-sheet.mjs` for why a camera beats a re-implementation.
 *
 * The picker runs IN THE PAGE. It finds the `props` section by its heading and
 * returns one entry per cell, named by the caption the sheet itself writes,
 * which for that section is the art name.
 */
async function shootScenes(outDir) {
  const size = SCENE_PX + 64; // shoot larger than we ship, then downsample.
  const cells = await shoot({
    webDir: join(PLAYHOUSE, "web"),
    path: "/sheet.html",
    size,
    scale: 2,
    // The sheet draws him 168px wide in a 192px cell, sized for a human
    // scanning thirty-one frames at once. A pane logo is one frame at 48px, so
    // he has to fill it: scale the rig with the box and drop the floor's empty
    // headroom. Geometry only — no pose, colour or prop is touched.
    css:
      `.cell #bot{width:${Math.round(size * BOT_FILL)}px!important}` +
      `.cell .scene{display:flex!important;align-items:flex-end!important;justify-content:center!important}` +
      `.cell .rig{margin-bottom:${Math.round(size * 0.04)}px!important}`,
    // Frame each cell on what is actually DRAWN in it — the union of the rig
    // and whatever is in his hands — rather than on the scene box. A prop
    // reaches a different distance in every drawing (the dish and the horn
    // reach furthest, the ledger barely at all), so one fixed crop either cuts
    // the far ones off or leaves the near ones swimming. Squared around that
    // union and clamped to the scene, so a plate is never part of its neighbour.
    pick: () => {
      const section = [...document.querySelectorAll("section")].find(
        (s) => s.querySelector("h2")?.textContent.trim() === "props",
      );
      if (!section) return [];
      const PAD = 0.1;
      return [...section.querySelectorAll(".cell")].map((cell) => {
        const scene = cell.querySelector(".scene");
        const box = scene.getBoundingClientRect();
        const parts = ["#bot", "#prop-slot", "#prop-slot-l"]
          .map((s) => scene.querySelector(s))
          .filter(Boolean)
          .map((el) => el.getBoundingClientRect())
          .filter((r) => r.width > 0 && r.height > 0);

        const x0 = Math.min(...parts.map((r) => r.left));
        const x1 = Math.max(...parts.map((r) => r.right));
        const y0 = Math.min(...parts.map((r) => r.top));
        const y1 = Math.max(...parts.map((r) => r.bottom));
        const cx = (x0 + x1) / 2;
        const cy = (y0 + y1) / 2;
        let side = Math.max(x1 - x0, y1 - y0) * (1 + PAD * 2);
        side = Math.min(side, box.width, box.height);

        // Keep the square inside the scene without letting it drift off the
        // subject: clamp the centre, never the size.
        const x = Math.min(Math.max(cx - side / 2, box.left), box.right - side);
        const y = Math.min(Math.max(cy - side / 2, box.top), box.bottom - side);
        return {
          name: cell.querySelector(".cap b")?.textContent.trim(),
          rect: { x: x + window.scrollX, y: y + window.scrollY, width: side, height: side },
        };
      });
    },
  });

  for (const [art, png] of cells) {
    const raw = join(outDir, `${art}.raw.png`);
    writeFileSync(raw, png);
    execFileSync("magick", [
      raw,
      "-filter", "Lanczos",
      "-resize", `${SCENE_PX}x${SCENE_PX}^`,
      "-gravity", "center", "-extent", `${SCENE_PX}x${SCENE_PX}`,
      "-strip", "-define", "png:compression-level=9",
      join(outDir, `${art}.png`),
    ]);
    rmSync(raw);
  }
  return [...cells.keys()];
}

// ------------------------------------------------------------------- run it

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

const stage = CHECK ? mkdtempSync(join(tmpdir(), "td-props-")) : REPO;
const propDir = CHECK ? join(stage, "props") : join(REPO, "app/assets/img/props");
const sceneDir = CHECK ? join(stage, "scenes") : join(REPO, "app/assets/img/scenes");
const manifestPath = CHECK ? join(stage, "tool-props.json") : join(REPO, "app/assets/tool-props.json");
mkdirSync(propDir, { recursive: true });
mkdirSync(sceneDir, { recursive: true });

for (const [art, tint] of [...arts].sort()) bakeProp(art, tint, propDir);
const shotArts = await shootScenes(sceneDir);
writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + "\n");

// A drawing with no photograph would degrade to a blank card, silently. Say so.
const missing = [...arts.keys()].filter((a) => !shotArts.includes(a));
if (missing.length) {
  console.error(`the sheet rendered no cell for: ${missing.join(", ")}`);
  process.exit(2);
}

if (!CHECK) {
  console.log(
    `${arts.size} props and ${shotArts.length} scenes baked, ${Object.keys(props).length} tool rows written`,
  );
  process.exit(0);
}

// --check ------------------------------------------------------------------
// Props and the manifest are byte-compared: rsvg is deterministic, so any
// difference is a real one. Scenes are compared PERCEPTUALLY, because a
// screenshot is not reproducible to the byte across chromium builds — a pose
// change moves RMSE by an order of magnitude more than rendering noise does.
const drift = [];

for (const f of readdirSync(propDir).filter((f) => f.endsWith(".png"))) {
  const a = readFileSync(join(propDir, f));
  let b;
  try {
    b = readFileSync(join(REPO, "app/assets/img/props", f));
  } catch {}
  if (!b || !a.equals(b)) drift.push(`app/assets/img/props/${f}`);
}

for (const f of readdirSync(sceneDir).filter((f) => f.endsWith(".png"))) {
  const live = join(REPO, "app/assets/img/scenes", f);
  let rmse = 1;
  try {
    // `compare` exits non-zero when images differ, so read the metric off
    // stderr rather than trusting the exit code.
    execFileSync("magick", ["compare", "-metric", "RMSE", join(sceneDir, f), live, "null:"], {
      stdio: ["ignore", "ignore", "pipe"],
    });
    rmse = 0;
  } catch (e) {
    const m = /\(([\d.]+)\)/.exec(String(e.stderr ?? ""));
    rmse = m ? Number(m[1]) : 1;
  }
  if (rmse > SCENE_TOLERANCE) drift.push(`app/assets/img/scenes/${f}  (RMSE ${rmse.toFixed(3)})`);
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
