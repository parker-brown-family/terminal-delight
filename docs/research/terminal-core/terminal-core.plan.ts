/**
 * Run plan: Terminal core — what Terminal Delight's emulator engine could be.
 *
 *   bun run /home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/bin/research-delight.ts \
 *     run docs/research/terminal-core/terminal-core.plan.ts \
 *     --out docs/research/terminal-core/run
 *
 * THE QUESTION
 * ------------
 * Terminal Delight (TD) parses every pane's output with alacritty_terminal 0.26
 * (and vte 0.15 underneath). Programs that draw pictures through the Kitty
 * graphics protocol (icat, viu, chafa, timg, mpv, ratatui-image) cannot draw in
 * TD, because vte drops APC and alacritty has no picture model. TD's foundation
 * review (docs/2026-08-29-foundation-interrogation-zed-gpui-quickshell.md,
 * option F) reserved swapping the VT seam "when image support becomes a
 * committed TD feature". This run gathers, for every candidate core INCLUDING
 * staying on alacritty, the facts a decision needs, each one cited.
 *
 * TD'S CONSTRAINTS (what "fit" means here)
 * ----------------------------------------
 * - MIT-licensed product: a GPL core cannot be linked or copied.
 * - A session host parses each pane's bytes and a window parses its own copy
 *   (a replica); a re-attaching window receives a snapshot re-encoded as VT
 *   bytes, and both sides' grids are hashed and compared. Replies to programs
 *   are written exactly once, by the host.
 * - The renderer (gpui) reads the grid row by row as styled text runs: char,
 *   zero-width marks, fg/bg, underline colour, flags, wide-char spacers.
 * - Seven TD files import alacritty_terminal; pane.rs reads the grid in 51 places.
 * - Build: cargo plus a pinned zed/gpui checkout with six patches. Distributed
 *   as an AppImage and (planned) AUR package.
 *
 * GATHERER
 * --------
 * ManualGatherer: each round writes run/round-N.brief.md and pauses; agents fill
 * run/round-N.part-X.json per cluster; scripts/merge-parts.ts assembles
 * run/round-N.result.json; re-running merges, re-scores, stops or emits round N+1.
 *
 * THE QUOTE RULE (why it exists)
 * ------------------------------
 * Every value with a source carries, in `note`, the verbatim passage it rests on:
 *   note: 'quote: "<exact excerpt, <= 400 chars>" — [measured|inferred] <remark>'
 * A second pass asks Jev, per value, whether the quoted passage states the value
 * (docs/research/terminal-core/jev/). The morning research of 2026-09-24 had to
 * discard an earlier pass that invented APIs; the quote makes that checkable.
 * Values measured by a command (gh api, crates.io API, cargo) quote the command's
 * output instead.
 *
 * EVIDENCE LABELS. provenance.confidence: high = read on a primary source this
 * session (repo, docs, crate API, source file); medium = secondary write-up or
 * reasoning over a primary; low = hunch. Notes say which.
 */
import { type Plan } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/plan.ts";
import { ManualGatherer } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/gather/manual.ts";
import { type Rubric } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/rubric.ts";
import { classify } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/provenance.ts";
import { isEmpty, type Dossier, type Field } from "/home/parker/BROWN-FAMILY-SPORTS/Software/research-delight/src/core/types.ts";

const HERE = import.meta.dir;

/** Kitty graphics features a core can implement. `kitty_coverage` counts these. */
export const KITTY_FEATURES = [
  "transmit-direct", // t=d, base64 in the escape, chunked
  "transmit-file", // t=f, a path the terminal opens
  "transmit-temp", // t=t, a temp file the terminal deletes
  "transmit-shm", // t=s, POSIX shared memory
  "format-png", // f=100
  "format-raw", // f=24 / f=32
  "compression", // o=z
  "place-direct", // a=T / a=p at the cursor
  "place-virtual", // U=1 virtual placements (Unicode placeholders)
  "delete-by-id", // d=i/I/n/N/r/R
  "delete-by-position", // d=c/p/q/x/y/z
  "query-reply", // answers a=q so programs can detect support
  "z-layers", // pictures above/below text by z-index
  "animation", // a=f / a=a / a=c
] as const;

/** Modern VT features TD relies on or will. */
export const VT_FEATURES = [
  "kitty-keyboard", "osc8-hyperlinks", "sync-output-2026", "osc52-clipboard",
  "grapheme-clusters", "reflow-on-resize", "alt-screen", "bracketed-paste", "mouse-sgr",
] as const;

const src = (key: string, requirement: "required" | "preferred", hint: string) =>
  ({ key, requirement, verify: "source-required" as const, hint });
const sof = (key: string, requirement: "required" | "preferred", hint: string) =>
  ({ key, requirement, verify: "source-or-flag" as const, hint });

export const rubric: Rubric = {
  name: "terminal-core-candidates",
  subjectType: "terminal-emulator-core",
  bubbleUp: true,
  specSuggestThreshold: 4,
  done: { requiredAll: true, preferredMinRatio: 0.6 },
  fields: [
    // ── identity ──
    src("what_it_is", "required", "one or two sentences: what this core/option is and who maintains it"),
    src("license", "required", "SPDX id, measured with `gh api repos/O/R --jq .license.spdx_id` or the crate's license field; say whether an MIT product may link it"),
    src("language", "required", "implementation language(s)"),
    sof("integration_path", "required", "how TD would consume it: crates.io crate | git dependency | C ABI through Rust bindings (name the binding crate) | fork + [patch] | not a library (reference only)"),
    sof("build_requirements", "required", "every toolchain and system library beyond stable cargo (e.g. Zig with version, C/C++ compiler, cmake, network access at build time); 'cargo only' if none"),
    // ── maturity ──
    src("latest_release", "required", "version and date of the newest release TD could depend on (crates.io API, GitHub releases, or tag)"),
    src("first_release", "preferred", "date of the first published release of the embeddable form"),
    src("releases_last_12mo", "preferred", "count of releases in the 12 months before 2026-09-25, measured from crates.io versions or GitHub tags; give the command"),
    sof("api_stability", "required", "the project's own words about API stability (quote it), or the semver situation (pre-1.0? breaking changes in recent releases?)"),
    src("maintainers", "required", "people carrying the work: number of contributors with >= 10% of commits in the last 12 months, measured (gh api repos/O/R/stats/contributors or commits); name the lead maintainer"),
    src("adoption", "preferred", "downloads (crates.io), dependents, and named products that embed it"),
    // ── pictures ──
    src("kitty_graphics", "required", "what of the Kitty graphics protocol it implements, citing source files: transmit media, formats, direct and virtual placements, deletes, query replies, z-layers, animation, quotas. 'none' if none, with the evidence that it drops APC"),
    { key: "kitty_tags", requirement: "required", verify: "none", hint: `comma-separated tags from exactly this vocabulary for what is IMPLEMENTED (not planned): ${KITTY_FEATURES.join(", ")} — or 'none'. Unknowns are listed in the note as 'unknown: tag, tag'. Drives the derived kitty_coverage.` },
    sof("sixel", "required", "Sixel support: yes / no / partial, with the source file or doc"),
    sof("iterm2_images", "required", "iTerm2 OSC 1337 inline images: yes / no / partial"),
    sof("picture_anchoring", "preferred", "how a placed picture is tied to the grid so it scrolls, survives scrollback and reflow: tracked pin, absolute row counter, per-cell attribute, placeholder cells, or n/a; and its storage quota"),
    // ── VT ──
    sof("vt_conformance", "preferred", "published conformance evidence: vttest / esctest results, a conformance matrix, or the project's stated coverage"),
    { key: "vt_features", requirement: "required", verify: "source-or-flag", hint: `for each of ${VT_FEATURES.join(", ")}: yes / no / unknown, one line, citing where each was established` },
    sof("performance_evidence", "preferred", "published or measured parse throughput and memory per cell; name the benchmark and the machine"),
    // ── fit with TD's architecture ──
    src("grid_api", "required", "how an embedder reads the screen: the types and methods for iterating rows/cells and what a cell carries (char, zero-width/grapheme, fg/bg, underline colour, flags, wide spacer), the cursor, the modes. Quote the doc comment or signature"),
    sof("damage_tracking", "preferred", "does it report which rows changed since the last read"),
    sof("state_snapshot", "required", "can the full terminal state be serialized, snapshotted, or replayed for a client that attaches later (TD's host/window replica)? Name the API, or 'no'"),
    sof("threading_and_loop", "required", "who owns the read/parse loop (embedder or the library), thread-safety (Send/Sync, locks), and whether it includes a PTY layer"),
    sof("reply_channel", "required", "how answers to program queries (device attributes, window-size reports, Kitty a=q) reach the embedder so it can write them to the PTY exactly once"),
    // ── analyst (inference) ──
    { key: "td_migration", requirement: "required", verify: "none", hint: "INFERRED: what TD would have to rewrite to adopt this, in terms of TD's seven alacritty-importing files (host.rs, term.rs, socketpty.rs, gridwire.rs, pane.rs, main.rs, pane/bench.rs) — label it inferred" },
    { key: "biggest_risk", requirement: "required", verify: "none", hint: "INFERRED: the one thing most likely to make this choice a regret, in one sentence" },
    src("primary_sources", "required", "space-separated URLs actually read this session; mark secondary write-ups 'synthesis:'"),
    // ── derived by code ──
    { key: "licence_gate", requirement: "derived", hint: "computed: MIT-compatible or blocked" },
    { key: "kitty_coverage", requirement: "derived", hint: "computed: implemented Kitty features, n of 14" },
    { key: "evidence_grade", requirement: "derived", hint: "computed: cited / flagged / unsourced counts" },
  ],
};

export const clusters = [
  "A · Keep alacritty_terminal",
  "B · Swap to a Rust core",
  "C · Swap to a core across a C boundary",
  "D · Hybrids and references",
] as const;
const [A, B, C, D] = clusters;

export const subjects = [
  { id: "alacritty-own-loop", label: "alacritty_terminal 0.26 + a TD-owned read loop; TD writes Kitty graphics itself", group: A },
  { id: "alacritty-patched-vte", label: "alacritty_terminal + patched vte (an APC callback) and a patched Term", group: A },
  { id: "alacritty-graphics-fork", label: "ayosec's alacritty graphics fork / vte-graphics (Sixel)", group: A },

  { id: "rio-vt", label: "rio-vt — Rio's embeddable core (crates.io)", group: B },
  { id: "wezterm-term", label: "wezterm-term / termwiz — WezTerm's core", group: B },
  { id: "vt100-rust", label: "vt100 (doy) — the crate under tui-term", group: B },
  { id: "td-own-core", label: "TD writes its own emulator core on a parser (prior art: Warp, Zellij)", group: B },

  { id: "libghostty-vt", label: "libghostty-vt — Ghostty's core as a library, via Rust bindings", group: C },
  { id: "libvterm", label: "libvterm — Neovim's C terminal library", group: C },
  { id: "contour-vtbackend", label: "Contour's vtbackend (C++)", group: C },
  { id: "kitty-core", label: "kitty's own core (C, GPL-3.0) — licence check", group: C },

  { id: "ghostty-shadow", label: "Hybrid: alacritty for text, libghostty-vt beside it for pictures only", group: D },
  { id: "zellij-grid", label: "Zellij — a Rust multiplexer whose server owns its own grid (host/client prior art)", group: D },
  { id: "zmx", label: "zmx — server-side snapshots with libghostty-vt (host/replica prior art)", group: D },
];

// ── Derived fields, computed by code so they are consistent across subjects ──

const text = (f: Field | undefined): string => (f && !isEmpty(f.value) ? String(f.value).toLowerCase() : "");

const MIT_COMPATIBLE = ["mit", "apache-2.0", "bsd-2-clause", "bsd-3-clause", "isc", "zlib", "mpl-2.0", "unlicense", "0bsd"];

export function licenceGate(d: Dossier): Field {
  const raw = text(d.fields["license"]);
  if (!raw) return { value: "unknown", flagged: true, note: "licence not gathered — unknown, not compatible" };
  if (/\bagpl|\bgpl|lgpl/.test(raw) && !/or (mit|apache)/.test(raw)) return { value: "blocked", note: "copyleft: an MIT product cannot link or copy it" };
  const ok = MIT_COMPATIBLE.some((l) => raw.includes(l));
  return ok ? { value: "compatible", note: "computed from the licence string" } : { value: "unknown", flagged: true, note: `licence '${raw}' not in the compatible list — check by hand` };
}

export function kittyCoverage(d: Dossier): Field {
  const raw = text(d.fields["kitty_tags"]);
  if (!raw) return { value: "not assessed", flagged: true, note: "kitty_tags missing — coverage unknown, not zero" };
  if (raw.trim() === "none") return { value: `0/${KITTY_FEATURES.length}`, note: "computed: none implemented" };
  const tags = raw.split(/[,\s]+/).filter(Boolean);
  const hits = KITTY_FEATURES.filter((t) => tags.includes(t));
  return { value: `${hits.length}/${KITTY_FEATURES.length}`, note: `computed from kitty_tags: ${hits.join(", ") || "none"}` };
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
  return ds.map((d) => ({
    ...d,
    fields: { ...d.fields, licence_gate: licenceGate(d), kitty_coverage: kittyCoverage(d), evidence_grade: evidenceGrade(d) },
  }));
}

const plan: Plan = {
  rubric,
  gatherer: new ManualGatherer(`${HERE}/run`),
  budget: { maxRounds: 3, maxStall: 2 },
  subjects,
  render: {
    title: "Terminal core — what Terminal Delight's emulator engine could be",
    subtitle:
      "Every candidate core, including staying on alacritty_terminal, with each fact cited and quoted. research-delight run, 2026-09-25.",
    groupOrder: [...clusters],
  },
  applyDerived,
};

export default plan;
