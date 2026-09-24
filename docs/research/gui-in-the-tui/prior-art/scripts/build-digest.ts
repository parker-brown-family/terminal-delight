/**
 * Build digest.md from the hand-written digest.src.md plus two sections
 * generated from the run itself, so the comparison table and the per-subject
 * notes can never drift from the sourced data:
 *
 *   bun run scripts/build-digest.ts
 *
 * It replays the round results through research-delight's own merge
 * (applyUpdates — the same non-destructive merge the harness used), applies the
 * plan's derived fields, and scores with scoreRun. Markers in digest.src.md:
 *   <!-- TABLE -->  the comparison table (short cells below are hand-written,
 *                   one line per subject; licence, closeness and DONE come
 *                   from the run)
 *   <!-- NOTES -->  per-subject notes: what / how / where / steal / trap /
 *                   sources, each labelled from its provenance
 */
import { readdirSync } from "node:fs";
import { applyUpdates } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/merge.ts";
import { scoreRun } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/rubric.ts";
import { classify } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/provenance.ts";
import { type Dossier, type Field } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/types.ts";
import plan, { clusters, subjects } from "../gui-in-the-tui.plan.ts";

const HERE = `${import.meta.dir}/..`;
const RUN = `${HERE}/run`;

// ── Replay the run ─────────────────────────────────────────────────────────
let ds: Dossier[] = subjects.map((s) => ({ id: s.id, subjectType: plan.rubric.subjectType, label: s.label, group: s.group, fields: {}, insights: [] }));
const rounds = readdirSync(RUN).filter((f) => /^round-\d+\.result\.json$/.test(f)).sort();
for (const f of rounds) {
  const res = JSON.parse(await Bun.file(`${RUN}/${f}`).text());
  ds = applyUpdates(ds, res.updates ?? [], plan.rubric.subjectType);
}
ds = plan.applyDerived!(ds);
const score = scoreRun(ds, plan.rubric);
const done = new Map(score.scores.map((s) => [s.id, s.complete]));

// ── Short cells, hand-written from the run's fields (mechanism · where) ─────
const SHORT: Record<string, [string, string]> = {
  waveterm: ["Electron DOM: react-markdown, image, PDF and video preview blocks; `<webview>` web blocks; terminal has no image protocol", "tiled blocks beside the terminal; magnify fills the tab"],
  warp: ["own Rust GPU UI; Alacritty-derived model with Kitty + iTerm2 images; built-in Markdown viewer", "images inline, row-anchored; Markdown in a viewer pane"],
  "zed-terminal": ["gpui + alacritty_terminal; no image protocol; a click opens a gpui ImageView, or an editor tab with Markdown preview on request", "split pane / tab (workspace item)"],
  extraterm: ["Qt widgets as blocks; `show` sends a MIME-typed bulk file", "inline frames that pop out to a tab"],
  domterm: ["HTML fragments in a browser-engine DOM (OSC 72)", "inline in scrollback; dockable panes"],
  hyper: ["xterm.js image addon (Sixel, iTerm2)", "inline; links leave the app"],
  terminology: ["EFL canvas; private `ESC }` escapes carry file paths; no Sixel or Kitty", "floating popup over the terminal, or a reserved cell area"],
  "vscode-terminal": ["xterm.js image addon (Sixel, iTerm2)", "inline; clicked files open as editor tabs"],
  "emacs-eaf": ["Qt apps (QtWebEngine, PDF) in a separate Python process", "separate OS window laid over an Emacs window"],
  cmux: ["libghostty terminal + WKWebView browser + webview Markdown viewer", "split pane beside the source terminal"],
  kitty: ["defines Kitty graphics (APC G; direct, file, temp-file, shm; Unicode placeholders)", "inline, cell-anchored; overlay windows; OS panels"],
  wezterm: ["iTerm2, Sixel and Kitty decoded into one cell image model", "inline, cell-anchored"],
  ghostty: ["Kitty graphics only, with placeholders and animation", "inline, cell-anchored"],
  rio: ["Sixel, iTerm2, Kitty with placeholders (rio-vt); wgpu", "inline, cell-anchored"],
  contour: ["Sixel, ReGIS, Kitty, iTerm2, Good Image Protocol", "inline; GIP layers"],
  foot: ["Sixel only", "inline"],
  iterm2: ["OSC 1337 inline images (it defines them)", "inline"],
  "alacritty-upstream": ["none: vte drops APC, DCS hooks are stubs", "text only"],
  "alacritty-graphics-fork": ["Sixel inside alacritty_terminal via `vte-graphics`", "inline, cell-anchored"],
  "xtermjs-addon-image": ["Sixel + iTerm2 IIP; Kitty marked WIP", "inline, per-cell tiles"],
  "cosmic-term": ["none upstream; a fork vendors ayosec's Sixel plus a kitty.rs", "(fork) inline"],
  dmux: ["none: text cells; Ink dialogs in `tmux display-popup`", "tmux splits + popups, 40-column sidebar"],
  tmux: ["no pixels; stores Sixel server-side and re-sends per client; passthrough", "tiled panes, display-popup, floating panes (3.7+)"],
  zellij: ["no pixels; re-encodes Sixel and (since 2026-07) Kitty for the host", "tiled, stacked, floating layer, in-place"],
  "arcan-cat9-durden": ["display-server compositing; cat9 `open` routes by MIME to decoders", "split / tab / swallow / embed hints to the WM"],
  herdr: ["libghostty-vt per pane; re-emits Kitty graphics to the host", "splits, tabs, popup, overlay"],
  "dmux-rs": ["host-terminal cells; Kitty graphics only for its own chrome", "composited tmux panes + native overlays"],
  "claude-squad": ["none: `tmux capture-pane` in a lipgloss box", "list + preview inside its TUI"],
  twin: ["none: text windows", "overlapping floating windows"],
  ueberzugpp: ["overlay OS window (X11/Wayland) or Kitty / Sixel / iTerm2 / chafa", "overlay window positioned over cells"],
  awrit: ["Electron offscreen paint → Kitty graphics over shm", "fills the terminal"],
  browsh: ["headless Firefox → half-blocks + real text", "fullscreen"],
  presenterm: ["Kitty (file or direct), iTerm2, Sixel, half-blocks", "fullscreen slides"],
  yazi: ["Kitty placeholders, old Kitty, iTerm2, Sixel, ueberzug++, chafa", "preview pane in a fullscreen TUI"],
  euporie: ["Kitty (placements or placeholders), iTerm2, Sixel, blocks", "fullscreen notebook"],
  mcat: ["Kitty (shm, placeholders), iTerm2, Sixel, ASCII; HTML/Markdown → image via headless Chromium", "inline in scrollback"],
  tdf: ["PDF pages via Kitty (kittage, shm) or ratatui-image", "fullscreen"],
  w3m: ["w3mimgdisplay onto the X11 window, or Sixel / Kitty / iTerm2", "fullscreen pager"],
  glow: ["no graphics: ANSI + OSC 8 links for images; OSC 66 big headers on main", "inline or pager"],
  notcurses: ["Kitty if `a=q` answers, else Sixel if DA1 has 4, else blocks", "fullscreen planes"],
  "mpv-terminal-vo": ["`vo=kitty` (shm), `vo=sixel`, `vo=tct` half-blocks", "fullscreen"],
  "ratatui-image": ["queries, then Kitty placeholders / Sixel / iTerm2 / half-blocks", "inside a ratatui Rect"],
  chafa: ["env term-DB first; symbols, Sixel, Kitty, iTerm2", "inline"],
  timg: ["env + XTVERSION; Kitty, iTerm2, Sixel, blocks", "inline"],
  "icy-sixel": ["Rust Sixel encoder + streaming decoder", "codec only"],
  "rio-vt": ["Rust VT core that parses Kitty, Sixel, iTerm2", "the embedder renders"],
  viuer: ["Sixel by DA1, iTerm2 by env, Kitty by query, else blocks", "inline"],
  carbonyl: ["Chromium → half-block pixels; glyphs printed as text", "fullscreen"],
  rtv: ["malware lure: copy of Santitub/rtv + a zip download", "n/a"],
  "rtv-santitub": ["Kitty (shm, zlib), iTerm2, Sixel, blocks, ASCII", "fullscreen alt screen"],
  "hyperion-tui": ["unknown: artwork, no software found", "depicted: a GUI dialog as a tile in a TUI"],
  "boyd-jev-pokemon": ["Kitty graphics, 4× nearest-neighbour upscale; half-block fallback", "cell-anchored over the streamed dashboard"],
};

// ── Helpers ────────────────────────────────────────────────────────────────
const label = (f: Field | undefined): string => {
  if (!f) return "not gathered";
  const s = classify(f);
  if (s === "flagged") return "unverified";
  if (s === "unverified") return "unsourced";
  const c = f.provenance?.confidence;
  return c === "high" ? "measured" : c === "low" ? "hunch" : "inferred";
};
const val = (f: Field | undefined): string => (f ? String(f.value).replace(/\s+/g, " ").trim() : "—");
const cell = (s: string): string => s.replace(/\|/g, "\\|");
const shortLicense = (f: Field | undefined): string => {
  const v = val(f);
  const m = v.match(/^(proprietary|none|unknown|Mixed|NOASSERTION|[A-Za-z0-9.\-+]+(?: (?:OR|AND) [A-Za-z0-9.\-+]+)?)/);
  return m ? m[1] : v.slice(0, 24);
};
const byGroup = (g: string) => ds.filter((d) => d.group === g);

// ── Comparison table ───────────────────────────────────────────────────────
const table: string[] = [];
table.push("| Subject | How the pixels or content get there | Where it draws | Licence | Closeness to TD | DONE |");
table.push("|---|---|---|---|---|---|");
for (const g of clusters) {
  table.push(`| **${g}** | | | | | |`);
  for (const d of byGroup(g)) {
    const [mech, where] = SHORT[d.id] ?? ["(no short cell)", ""];
    const close = val(d.fields["closeness"]);
    table.push(`| ${cell(d.label)} | ${cell(mech)} | ${cell(where)} | ${cell(shortLicense(d.fields["license"]))} | ${cell(close)} | ${done.get(d.id) ? "yes" : "no"} |`);
  }
}
const missingShort = ds.filter((d) => !SHORT[d.id]).map((d) => d.id);
if (missingShort.length) console.warn(`no short cell for: ${missingShort.join(", ")}`);

// ── Per-subject notes ──────────────────────────────────────────────────────
const notes: string[] = [];
const line = (name: string, f: Field | undefined) => `- **${name}** (${label(f)}) — ${val(f)}`;
for (const g of clusters) {
  notes.push(`### ${g}\n`);
  for (const d of byGroup(g)) {
    notes.push(`#### ${d.label}\n`);
    // Where-it-draws, input model, licence and activity are in the comparison
    // table and the harness's HTML/CSV; the notes carry the how and the why.
    notes.push(line("What", d.fields["what_it_is"]));
    notes.push(line("How", d.fields["graphics_mechanism"]));
    notes.push(line("Take for TD", d.fields["steal_for_td"]));
    notes.push(line("Avoid", d.fields["trap"]));
    notes.push(`- **Sources** — ${val(d.fields["primary_sources"])}`);
    notes.push("");
  }
}

// ── Assemble ───────────────────────────────────────────────────────────────
const src = await Bun.file(`${HERE}/digest.src.md`).text();
const out = src
  .replace("<!-- TABLE -->", table.join("\n"))
  .replace("<!-- NOTES -->", notes.join("\n"))
  .replace("<!-- SCORE -->", `${score.completeCount} of ${score.total} subjects met the Definition of Done; mean completeness ${(score.meanScore * 100).toFixed(0)}%`);
await Bun.write(`${HERE}/digest.md`, out);
console.log(`digest.md: ${ds.length} subjects, ${score.completeCount}/${score.total} DONE, ${out.length} chars`);
