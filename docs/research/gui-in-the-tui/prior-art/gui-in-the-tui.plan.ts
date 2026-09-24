/**
 * Run plan: GUI in the TUI — prior art for Terminal Delight's in-terminal viewer.
 *
 *   bun run /home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/bin/research-delight.ts \
 *     run docs/research/gui-in-the-tui/prior-art/gui-in-the-tui.plan.ts \
 *     --out docs/research/gui-in-the-tui/prior-art/run
 *
 * THE QUESTION
 * ------------
 * Which projects put graphical content (images, documents, web pages, video, GUI
 * widgets) inside or beside a terminal, or turn a terminal/TUI into the window
 * manager for mixed content? For each: how it does it, and the one idea Terminal
 * Delight should take from it.
 *
 * THE FEATURE BEING STUDIED
 * -------------------------
 * Terminal Delight (TD) is a Rust terminal on gpui (Zed's GPU UI framework) with
 * alacritty_terminal as its VT engine, tmux-style splits, tabs and agent panes, on
 * Omarchy (Arch + Hyprland). Ctrl+click a Markdown file, an HTML brief or an image
 * in terminal output and it opens inside TD — in a new split pane, or as a
 * floating draggable square in the current pane. Later: programs running inside
 * TD show real pixels (video, previews) via the Kitty graphics protocol or Sixel.
 *
 * GATHERER
 * --------
 * ManualGatherer: each round the harness writes run/round-N.brief.md and pauses;
 * an agent session executes it and writes run/round-N.result.json; re-running
 * merges, re-scores and either stops or emits round N+1 for the remaining gaps.
 * Tiering (docs/GATHERERS.md) is applied by hand inside a round: WebSearch /
 * WebFetch / `gh api` first, the Servo reader only for pages the cheap tier
 * could not read.
 *
 * RUBRIC CHOICES THAT DEPART FROM THE BRIEF'S SUGGESTION
 * ------------------------------------------------------
 * 1. `steal_for_td` and `trap` were suggested as `derived`. In this harness
 *    `derived` means "computed by code after convergence, never gathered and never
 *    gated". These two are an analyst's judgement written by the gatherer, and a
 *    subject without its one idea is not done — so `steal_for_td` is `required`
 *    and `trap` is `preferred`, both `verify: none` (they are inference, and their
 *    note carries the label `inferred`).
 * 2. Two genuinely derived fields are added and computed below by code:
 *    `closeness` (how many of TD's feature traits the subject shares, read off the
 *    `td_overlap` tags) and `evidence_grade` (verified / flagged counts).
 * 3. `td_overlap` is new: a tag list from a fixed vocabulary, so closeness is a
 *    count rather than a vibe.
 *
 * EVIDENCE LABELS. Every value's `provenance.confidence` maps to the label the
 * digest uses: high = measured (read on a primary source this session — the
 * project's repo, docs, or `gh api`), medium = inferred (from a search summary, a
 * secondary write-up, or reasoning over a primary), low = hunch. Notes say which.
 *
 * SECURITY. github.com/Laotian-telephonebell2559/rtv is judged from metadata only.
 * Nothing from it is cloned, downloaded, built or run.
 */
import { type Plan } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/plan.ts";
import { ManualGatherer } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/gather/manual.ts";
import { type Rubric } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/rubric.ts";
import { classify } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/provenance.ts";
import { isEmpty, type Dossier, type Field } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/types.ts";

const HERE = import.meta.dir;

/** The TD feature traits a subject can share. `closeness` counts these. */
export const TD_TRAITS = [
  "open-on-click", // click a path/link in terminal output and a viewer opens
  "pane-content", // non-terminal content (image, doc, web page) lives as a pane/block in the split layout
  "floating", // a floating / popup panel over the terminal
  "host-protocol", // the terminal itself implements Kitty graphics / Sixel / iTerm2 OSC 1337 for programs
  "tui-wm", // a terminal or TUI acting as the window manager for mixed content
  "agent-panes", // multiplexes AI agent sessions
  "same-stack", // Rust, gpui, or alacritty_terminal — TD's own stack
] as const;

export const rubric: Rubric = {
  name: "gui-in-the-tui-prior-art",
  subjectType: "terminal-graphics-project",
  bubbleUp: true,
  specSuggestThreshold: 4,
  done: { requiredAll: true, preferredMinRatio: 0.6 },
  fields: [
    {
      key: "what_it_is",
      requirement: "required",
      verify: "source-required",
      hint: "one or two sentences: what the project is and what graphical content it puts in or beside a terminal",
    },
    {
      key: "license",
      requirement: "required",
      verify: "source-required",
      hint: "SPDX id from the repo (gh api repos/O/R --jq .license.spdx_id) or 'proprietary' with the vendor page as source",
    },
    {
      key: "language",
      requirement: "required",
      verify: "source-required",
      hint: "primary implementation language(s), from the repo's language breakdown or docs",
    },
    {
      key: "graphics_mechanism",
      requirement: "required",
      verify: "source-required",
      hint: "HOW pixels or rich content get there: native GPU drawing, Kitty graphics protocol, Sixel, iTerm2 OSC 1337, Unicode half/quadrant/sextant blocks, separate overlay OS window, embedded webview/browser engine, DOM (Electron/xterm.js) … cite the doc or source file",
    },
    {
      key: "where_it_draws",
      requirement: "required",
      verify: "source-or-flag",
      hint: "split pane | floating panel | inline in scrollback | fullscreen | separate OS window | cell-anchored overlay — say which, and whether it scrolls with text",
    },
    {
      key: "input_model",
      requirement: "preferred",
      verify: "source-or-flag",
      hint: "how mouse and keys reach the embedded thing (focus model, mouse passthrough, keyboard capture, escape key)",
    },
    {
      key: "activity",
      requirement: "preferred",
      verify: "source-required",
      hint: "stars, last push / last release date — measured with `gh api repos/O/R` (source: 'measured 2026-09-24: gh api repos/O/R') or the vendor's release page",
    },
    {
      key: "td_overlap",
      requirement: "required",
      verify: "none",
      hint: `comma-separated tags from exactly this vocabulary: ${TD_TRAITS.join(", ")} — or 'none'. Drives the derived closeness score.`,
    },
    {
      key: "steal_for_td",
      requirement: "required",
      verify: "none",
      hint: "THE one idea Terminal Delight should take from it, concrete enough to act on (inferred — say so in the note)",
    },
    {
      key: "trap",
      requirement: "preferred",
      verify: "none",
      hint: "what TD should avoid, learned from this project's design or its issue tracker",
    },
    {
      key: "primary_sources",
      requirement: "required",
      verify: "source-required",
      hint: "space-separated repo/docs URLs actually read; mark blog posts or AI summaries as 'synthesis:'",
    },
    { key: "closeness", requirement: "derived", hint: "count of TD traits shared, 0–7 — computed from td_overlap" },
    { key: "evidence_grade", requirement: "derived", hint: "verified / flagged / unverified field counts — computed" },
  ],
};

export const clusters = [
  "A · Terminals that are also a workspace",
  "B · Terminals that draw pixels for programs",
  "C · Multiplexers and TUI-as-window-manager",
  "D · Programs that put pixels in a terminal",
  "E · The two posts Parker shared",
] as const;
const [A, B, C, D, E] = clusters;

export const subjects = [
  { id: "waveterm", label: "Wave Terminal — terminal, web and preview blocks", group: A },
  // Seeded as "closed source"; round 1 found the client AGPL-3.0 since April 2026.
  { id: "warp", label: "Warp — blocks, Markdown viewer (client AGPL since 2026-04)", group: A },
  { id: "zed-terminal", label: "Zed's terminal — gpui + alacritty_terminal", group: A },
  { id: "extraterm", label: "Extraterm — frames and rich output", group: A },
  { id: "domterm", label: "DomTerm — HTML in the terminal", group: A },
  { id: "hyper", label: "Hyper — Electron terminal", group: A },

  { id: "kitty", label: "Kitty — graphics protocol, kittens, icat, overlays", group: B },
  { id: "wezterm", label: "WezTerm", group: B },
  { id: "ghostty", label: "Ghostty", group: B },
  { id: "rio", label: "Rio", group: B },
  { id: "contour", label: "Contour", group: B },
  { id: "foot", label: "foot", group: B },
  { id: "iterm2", label: "iTerm2 — OSC 1337 inline images", group: B },

  { id: "dmux", label: "dmux — agent panes on tmux (named by Parker)", group: C },
  { id: "tmux", label: "tmux — display-popup, image passthrough", group: C },
  { id: "zellij", label: "Zellij — floating panes, plugins", group: C },
  { id: "arcan-cat9-durden", label: "Arcan + cat9 + durden", group: C },

  { id: "ueberzugpp", label: "ueberzug++", group: D },
  { id: "awrit", label: "awrit — Chromium in Kitty", group: D },
  { id: "carbonyl", label: "Carbonyl — Chromium in a terminal", group: D },
  { id: "browsh", label: "browsh", group: D },
  { id: "presenterm", label: "presenterm", group: D },
  { id: "yazi", label: "yazi — file manager with previews", group: D },
  { id: "euporie", label: "euporie — Jupyter in the terminal", group: D },
  { id: "notcurses", label: "notcurses", group: D },
  { id: "mpv-terminal-vo", label: "mpv — vo=kitty / vo=sixel / vo=tct", group: D },
  { id: "ratatui-image", label: "ratatui-image", group: D },
  { id: "chafa", label: "chafa", group: D },
  { id: "timg", label: "timg", group: D },
  { id: "rtv", label: "rtv (Laotian-telephonebell2559) — claimed FFmpeg-in-terminal, metadata only", group: D },

  { id: "hyperion-tui", label: "'GUI in the TUI' / HYPERION TUI v2.7.19 (Nix, @NixFred)", group: E },
  { id: "boyd-jev-pokemon", label: "Boyd's Jev-plays-Pokemon dashboard (@0xBOYD)", group: E },
];

// ── Derived fields, computed by code so they are consistent across subjects ──

const text = (f: Field | undefined): string => (f && !isEmpty(f.value) ? String(f.value).toLowerCase() : "");

export function closeness(d: Dossier): Field {
  const raw = text(d.fields["td_overlap"]);
  if (!raw) return { value: "not assessed", flagged: true, note: "td_overlap missing — closeness unknown, not zero" };
  const tags = raw.split(/[,\s]+/).filter(Boolean);
  const hits = TD_TRAITS.filter((t) => tags.includes(t));
  return { value: `${hits.length}/${TD_TRAITS.length}${hits.length ? ` — ${hits.join(", ")}` : ""}`, note: "computed from td_overlap" };
}

export function evidenceGrade(d: Dossier): Field {
  let verified = 0, flagged = 0, unverified = 0;
  for (const spec of rubric.fields) {
    if (spec.requirement === "derived" || spec.verify === "none" || spec.verify === undefined) continue;
    const f = d.fields[spec.key];
    if (!f) continue;
    const s = classify(f);
    if (s === "verified") verified++;
    else if (s === "flagged") flagged++;
    else if (s === "unverified") unverified++;
  }
  return { value: `${verified} cited · ${flagged} flagged · ${unverified} unsourced`, note: "counts over source-checked fields" };
}

export function applyDerived(ds: Dossier[]): Dossier[] {
  return ds.map((d) => ({ ...d, fields: { ...d.fields, closeness: closeness(d), evidence_grade: evidenceGrade(d) } }));
}

const plan: Plan = {
  rubric,
  gatherer: new ManualGatherer(`${HERE}/run`),
  budget: { maxRounds: 4, maxStall: 2 },
  subjects,
  render: {
    title: "GUI in the TUI — prior art for Terminal Delight",
    subtitle:
      "Projects that put images, documents, web pages, video or widgets inside or beside a terminal, or make a terminal the window manager. research-delight run, 2026-09-24.",
    groupOrder: [...clusters],
  },
  applyDerived,
};

export default plan;
