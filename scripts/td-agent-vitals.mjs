#!/usr/bin/env node
// td-agent-vitals — read an agent's own transcript and answer the only question
// the agent wall cannot currently answer: should this session be shut down?
//
// A long session is not free. Every turn re-sends the whole conversation, so a
// window that has grown to 550k tokens costs 550k tokens of re-read before the
// agent has thought about anything, and it will cost that again on the next
// turn, and the one after. That is the drag the wall has no number for. The
// status line reports Σ session tokens, which goes up whether the session is
// healthy or dying, and tells you nothing about which.
//
// Three bars, measuring three different things, deliberately independent:
//
//   WINDOW     how full the context is. Exact, not estimated — the sum of
//              input + cache_creation + cache_read on the newest assistant
//              turn IS the prompt that was sent, and the model id gives the
//              limit. Also answers "how many turns of headroom is that".
//
//   FATIGUE    accumulated damage and drag, independent of size: compactions
//              survived and what they threw away, a rising tool-error rate,
//              the same file read for the fourth time, latency per token of
//              output drifting up, hours on the clock.
//
//   RELEVANCE  what fraction of the loaded context still serves the task in
//              front of the agent. A proxy, and named as one — it scores tool
//              results by whether their target is still in the working set,
//              whether a later call superseded them, and whether they errored.
//
// The pair that matters is WINDOW against RELEVANCE, because it decides what
// to DO about a full context, and the two failure modes look identical on a
// token counter:
//
//   full + irrelevant → compact. It is cheap; you are dropping ballast.
//   full + relevant   → hand off. Compaction will destroy detail you need,
//                       and the summary will read as if it did not.
//
// That is the call this script exists to make. Everything else is the evidence
// for it.
//
//   node scripts/td-agent-vitals.mjs                 every live agent
//   node scripts/td-agent-vitals.mjs --json          the wall's contract
//   node scripts/td-agent-vitals.mjs <session-id>    one session
//   node scripts/td-agent-vitals.mjs path/to.jsonl   one transcript
//   node scripts/td-agent-vitals.mjs --cwd ~/Work/x  newest for a directory
//
// Read-only. It opens transcripts and the TD agent ledger, and writes nothing.

import {
  closeSync,
  createReadStream,
  existsSync,
  fstatSync,
  openSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  readSync,
  statSync,
} from "node:fs";
import { createConnection } from "node:net";
import { createInterface } from "node:readline";
import { homedir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";

// ─── model limits ──────────────────────────────────────────────────────────
//
// The `[1m]` suffix is Claude Code's marker for the 1M-token context beta, and
// the obvious place to read it is the wrong one: `.message.model` on every
// assistant turn says plain `claude-opus-5` even in a session demonstrably
// holding 550k tokens. Only the `cost-state` record spells the variant out.
// Reading the message field alone reports such a session as 275% full, clamps
// it to 100%, and tells you to compact an agent with 450k of headroom.
//
// So there are three sources, in order of how much they can be trusted:
// the cost-state key, the message model, and the transcript itself. The last
// one outranks both — a window that has HELD 550k tokens is not a 200k window,
// whatever any id claims.

const CONTEXT_LIMITS = [
  [/\[1m\]/, 1_000_000],
  [/opus-5|sonnet-5|fable-5/, 200_000],
  [/opus-4|sonnet-4|haiku-4/, 200_000],
];
const TIERS = [200_000, 1_000_000];
const DEFAULT_LIMIT = 200_000;

// Claude Code triggers autocompaction before the hard ceiling, so the usable
// headroom is short of the limit. This is the fraction it aims at.
const AUTOCOMPACT_FLOOR = 0.92;

function nominalLimit(id) {
  if (!id) return 0;
  for (const [re, n] of CONTEXT_LIMITS) if (re.test(id)) return n;
  return 0;
}

function contextLimit(m, peak, override = null) {
  if (override) return override;
  let limit = 0;
  for (const k of Object.keys(m.cost?.modelUsage || {})) limit = Math.max(limit, nominalLimit(k));
  if (!limit) limit = nominalLimit(m.model) || DEFAULT_LIMIT;
  if (peak > limit) limit = TIERS.find((t) => t >= peak) ?? peak;
  return limit;
}

// ─── transcript loading ────────────────────────────────────────────────────

async function readTranscript(path) {
  const rows = [];
  const rl = createInterface({ input: createReadStream(path), crlfDelay: Infinity });
  for await (const line of rl) {
    if (!line || line[0] !== "{") continue;
    try {
      rows.push(JSON.parse(line));
    } catch {
      // A transcript being appended to while we read it can hand us a torn
      // final line. Skipping it costs one turn of freshness; throwing would
      // cost the whole reading.
    }
  }
  return rows;
}

// ─── the turn model ────────────────────────────────────────────────────────
//
// A transcript is not a list of turns. It carries queue operations, mode
// flips, file-history snapshots, ATIS latches and per-session bookkeeping
// interleaved with the conversation. This pulls out the four things the
// metrics actually stand on: assistant turns with a usage block, tool calls,
// tool results, and compaction boundaries.

function buildModel(rows) {
  const turns = []; // assistant turns carrying usage
  const calls = []; // tool_use, in order
  const results = []; // tool_result, in order, joined to their call
  const compactions = [];
  const prompts = []; // human prompts, not tool results
  const byId = new Map(); // tool_use_id → call

  let model = null;
  let started = null;
  let ended = null;
  let cost = null;

  for (const r of rows) {
    const ts = r.timestamp ? Date.parse(r.timestamp) : null;
    if (ts) {
      if (started === null || ts < started) started = ts;
      if (ended === null || ts > ended) ended = ts;
    }

    if (r.type === "cost-state") {
      cost = r;
      continue;
    }

    if (r.type === "system" && r.subtype === "compact_boundary") {
      const m = r.compactMetadata || {};
      compactions.push({
        ts,
        trigger: m.trigger || "unknown",
        pre: m.preTokens || 0,
        post: m.postTokens || 0,
        dropped: m.cumulativeDroppedTokens || 0,
        index: calls.length, // where in the call stream it fell
      });
      continue;
    }

    if (r.type === "assistant" && r.message) {
      const u = r.message.usage;
      if (r.message.model && r.message.model !== "<synthetic>") model = r.message.model;
      if (u) {
        turns.push({
          ts,
          model: r.message.model,
          ctx: (u.input_tokens || 0) + (u.cache_creation_input_tokens || 0) + (u.cache_read_input_tokens || 0),
          created: u.cache_creation_input_tokens || 0,
          read: u.cache_read_input_tokens || 0,
          out: u.output_tokens || 0,
        });
      }
      for (const c of r.message.content || []) {
        if (c.type !== "tool_use") continue;
        const t = targetsOf(c.name, c.input || {});
        const call = {
          ts,
          name: c.name,
          id: c.id,
          paths: t.paths,
          key: t.key,
          turnIndex: turns.length - 1,
        };
        calls.push(call);
        if (c.id) byId.set(c.id, call);
      }
      continue;
    }

    if (r.type === "user" && r.message) {
      const content = r.message.content;
      if (typeof content === "string") {
        if (!r.isCompactSummary) prompts.push({ ts, text: content });
        continue;
      }
      if (!Array.isArray(content)) continue;
      const toolResults = content.filter((c) => c.type === "tool_result");
      if (toolResults.length === 0) {
        const text = content
          .filter((c) => c.type === "text")
          .map((c) => c.text)
          .join("\n");
        if (text && !r.isCompactSummary) prompts.push({ ts, text });
        continue;
      }
      for (const tr of toolResults) {
        const call = tr.tool_use_id ? byId.get(tr.tool_use_id) : null;
        const body = typeof tr.content === "string" ? tr.content : JSON.stringify(tr.content ?? "");
        results.push({
          ts,
          error: tr.is_error === true,
          chars: body.length,
          call,
          index: results.length,
        });
        if (call) call.result = results[results.length - 1];
      }
    }
  }

  return { turns, calls, results, compactions, prompts, model, started, ended, cost };
}

// ─── what a tool call touched ──────────────────────────────────────────────
//
// Relevance needs to know what each call was ABOUT, and the transcript will
// not say directly. Half the calls in a lean-ctx session are a shell command
// with the path buried in an argument, so a file_path lookup alone sees
// nothing and scores the whole session as unfocused. Pulling paths out of the
// command string is what makes the measure work on a real transcript rather
// than a tidy one.

const PATH_RE = /(?:^|[\s"'`=(,])((?:\/|~\/|\.{1,2}\/)?(?:[\w.@+-]+\/)*[\w.@+-]+(?:\.[A-Za-z0-9]{1,8})?)/g;

// A bare dotted token is only a file if its extension says so. Without this
// list `python3 -m http.server` contributes `http.server` as a path, and one
// junk entry in the focus set is enough to score unrelated work as on-topic.
const EXTS = new Set(
  ("rs ts tsx js mjs cjs jsx py rb go java c h cpp hpp sh bash zsh fish md mdx txt json jsonl toml yaml yml " +
    "html css scss lock sql xml svg png webp conf ini env gitignore service socket qml lua vim el nix")
    .split(" ")
);

// Two different things are wanted from a call, and conflating them was the
// first version's worst bug. `paths` is what the call TOUCHED — only a real
// path counts, and only a later call on the same path supersedes an earlier
// one. `key` is what the call WAS, so that churn can tell `git status` run
// eight times from eight different commands. Keying both on the tool name
// made every shell call in a session supersede every other one, which scored
// a healthy transcript at 8% relevant.
function targetsOf(name, input) {
  const paths = new Set();
  for (const key of ["file_path", "path", "notebook_path", "filePath"]) {
    if (typeof input[key] === "string") addPath(paths, input[key]);
  }
  if (Array.isArray(input.paths)) for (const p of input.paths) if (typeof p === "string") addPath(paths, p);

  const prose = [input.command, input.pattern, input.query, input.task, input.description]
    .filter((v) => typeof v === "string")
    .join(" ");
  for (const m of prose.matchAll(PATH_RE)) addPath(paths, m[1]);

  const list = [...paths];
  // A call naming no path is still a call, and identified by what it did:
  // the command itself, collapsed, so repetition is visible and variety is
  // not mistaken for it.
  const key = list.length ? list.sort().join("|") : `${name}::${signature(input)}`;
  return { paths: list, key };
}

function addPath(set, raw) {
  const s = normalise(raw);
  if (!s) return;
  if (s.includes("/")) {
    set.add(s);
    return;
  }
  const dot = s.lastIndexOf(".");
  if (dot > 0 && EXTS.has(s.slice(dot + 1).toLowerCase())) set.add(s);
}

function normalise(p) {
  let s = p.trim().replace(/[)\]},.:;'"`]+$/, "");
  if (!s || s === "." || s === "..") return "";
  if (s.startsWith("~/")) s = join(homedir(), s.slice(2));
  if (s.startsWith("./")) s = s.slice(2);
  return s;
}

// Enough of a command to tell two apart, little enough that the same command
// with a different timeout is still the same command.
function signature(input) {
  const raw = input.command || input.pattern || input.query || input.task || "";
  return String(raw).replace(/\s+/g, " ").trim().slice(0, 120);
}

// The parent directory, when there is a real one. `dirname("notes.md")` is
// ".", and "." in the focus set matches every relative filename in the
// transcript — the whole session then scores as on-topic.
function parentDir(p) {
  const d = dirname(p);
  return d && d !== "." && d !== "/" ? d : null;
}

// ─── 1. WINDOW ─────────────────────────────────────────────────────────────

function windowStat(m, override = null) {
  const turns = m.turns;
  const last = turns[turns.length - 1];
  const peak = turns.reduce((a, t) => Math.max(a, t.ctx), 0);
  const limit = contextLimit(m, peak, override);
  const ctx = last ? last.ctx : 0;
  const fill = limit > 0 ? ctx / limit : 0;

  // Growth per turn, over the recent tail only. An early-session average is
  // meaningless: the first turns load a system prompt and the working set,
  // which never happens again.
  const tail = turns.slice(-12);
  const deltas = [];
  for (let i = 1; i < tail.length; i++) {
    const d = tail[i].ctx - tail[i - 1].ctx;
    if (d > 0) deltas.push(d); // a negative delta is a compaction, not growth
  }
  const growth = deltas.length ? median(deltas) : 0;
  const ceiling = limit * AUTOCOMPACT_FLOOR;
  const turnsLeft = growth > 0 ? Math.max(0, Math.floor((ceiling - ctx) / growth)) : null;

  return {
    score: clamp01(fill) * 100,
    tokens: ctx,
    limit,
    fill,
    growthPerTurn: Math.round(growth),
    turnsLeft,
    turns: turns.length,
    peak,
  };
}

// ─── 2. FATIGUE ────────────────────────────────────────────────────────────
//
// Deliberately NOT a restatement of context size — the wall already has that
// bar, and two bars that move together are one bar. Fatigue is the damage a
// long session accumulates that a fresh one would not have: summaries standing
// in for detail, a tool-error rate that has started climbing, the same file
// fetched again because the first copy scrolled out of attention, turns taking
// longer per token produced.

const FATIGUE_WEIGHTS = {
  scars: 0.28, // compactions survived, and what they threw away
  errors: 0.18, // error rate rising against this session's own baseline
  churn: 0.18, // the same target called again and again
  latency: 0.14, // ms per 1k output tokens, recent against early
  retread: 0.12, // tokens spent re-reading what was already loaded
  age: 0.10, // hours on the clock
};

function fatigueStat(m, win) {
  const parts = {};

  // Scars. A compaction is not free: it trades detail for room, and the agent
  // afterwards is working from its own summary. Two of them is a different
  // animal from none.
  const dropped = m.compactions.reduce((a, c) => a + c.dropped, 0);
  parts.scars = clamp01(m.compactions.length * 0.34 + (dropped / win.limit) * 0.25);

  // Errors, as a TREND. The level is a property of the task — a session doing
  // exploratory shell work errors more than one editing files, and neither is
  // fatigue. A rate that has doubled against its own early baseline is.
  const errs = m.results.map((r) => (r.error ? 1 : 0));
  const cut = Math.floor(errs.length * 0.6);
  const base = mean(errs.slice(0, cut));
  const recent = mean(errs.slice(cut));
  // Two errors that both happened to land late is not a trend, however
  // dramatic the ratio looks — a session with 2 errors in 81 calls read 100
  // for "errors climbing". Enough calls to have a baseline AND enough errors
  // to be a pattern.
  const errorCount = errs.filter(Boolean).length;
  parts.errors = errs.length < 20 || errorCount < 4 ? 0 : clamp01(recent / Math.max(base, 0.03) - 1);
  const errorRate = mean(errs);

  // Churn. Calling the same target repeatedly is the visible form of an agent
  // that has stopped holding what it already fetched. Counted over the recent
  // half, so a legitimately repetitive early phase does not stain the score.
  const half = m.calls.slice(Math.floor(m.calls.length / 2));
  const seen = new Map();
  let repeats = 0;
  for (const c of half) {
    const n = (seen.get(c.key) || 0) + 1;
    seen.set(c.key, n);
    // The third time, not the second. Doing a thing twice is how iteration
    // looks — edit, re-run the test, edit, re-run. Counting that as fatigue
    // pinned every working session at 100 and measured nothing.
    if (n > 2) repeats++;
  }
  parts.churn = half.length ? clamp01((repeats / half.length) * 1.2) : 0;

  // Latency drift. Not raw duration — a long turn may be a big job. Time per
  // 1k tokens of output, recent against early, catches the turn that takes
  // longer to produce the same amount of work.
  parts.latency = latencyDrift(m);

  // Retread. Tokens spent on a read whose target was already read earlier and
  // is still in the window. Distinct from churn: churn counts calls, this
  // counts what they cost.
  const { retreadTokens, totalTokens } = retreadCost(m);
  parts.retread = totalTokens ? clamp01((retreadTokens / totalTokens) * 1.5) : 0;

  // Age. Weakest signal, and last for that reason — a six-hour session that is
  // clean is not tired. It is here because wall-clock does eventually count.
  const hours = m.started && m.ended ? (m.ended - m.started) / 3.6e6 : 0;
  parts.age = clamp01(hours / 8);

  const score = Object.entries(FATIGUE_WEIGHTS).reduce((a, [k, w]) => a + w * parts[k], 0) * 100;

  return {
    score,
    parts,
    compactions: m.compactions.length,
    droppedTokens: dropped,
    errorRate,
    errors: errs.filter(Boolean).length,
    calls: m.calls.length,
    hours: round(hours, 1),
    resendPerTurn: win.tokens, // what the next turn costs before it thinks
  };
}

function latencyDrift(m) {
  const pairs = [];
  for (let i = 1; i < m.turns.length; i++) {
    const dt = m.turns[i].ts - m.turns[i - 1].ts;
    const out = m.turns[i].out;
    if (dt > 0 && dt < 15 * 60_000 && out > 40) pairs.push((dt / out) * 1000);
  }
  if (pairs.length < 12) return 0; // fewer than this and one slow turn is the whole signal
  const cut = Math.floor(pairs.length * 0.6);
  const early = median(pairs.slice(0, cut));
  const recent = median(pairs.slice(cut));
  if (!early) return 0;
  // Halved, so the bar reads full at three times slower rather than twice.
  // Twice as long per token is a session that moved from reading files to
  // running builds, which is a change of work, not a tired agent.
  return clamp01((recent / early - 1) / 2);
}

// Only a call naming a real path can retread — fetching the same file twice
// is the thing being measured. A shell command run twice may be checking
// whether something changed, which is work, not waste; churn counts that.
//
// Measured over the recent half against everything seen before it. Counted
// across a whole session it saturates at 100 for any session long enough to
// revisit a file, which is every real one.
function retreadCost(m) {
  const seen = new Set();
  const cut = Math.floor(m.calls.length / 2);
  let retreadTokens = 0;
  let totalTokens = 0;
  for (let i = 0; i < m.calls.length; i++) {
    const c = m.calls[i];
    if (!c.result) {
      for (const p of c.paths) seen.add(p);
      continue;
    }
    if (i >= cut) {
      const t = resultTokens(c.result);
      totalTokens += t;
      // Only a SUBSTANTIAL re-fetch counts. Re-reading a file you are editing
      // is how editing works, and with lean-ctx a repeat read costs a diff.
      // Pulling a large body back that the window already holds is the waste.
      if (t > RETREAD_FLOOR && c.paths.length > 0 && c.paths.every((p) => seen.has(p))) retreadTokens += t;
    }
    for (const p of c.paths) seen.add(p);
  }
  return { retreadTokens, totalTokens };
}

// What each tool result actually cost in context.
//
// The exact figure is there and worth the trouble: cache_creation on the NEXT
// assistant turn is the tokens newly written since the last one — these
// results plus the assistant's own output before them, and that output is
// known exactly. Subtracting it leaves what the results weighed.
//
// Two traps, both of which produced numbers that could not be true and
// printed anyway.
//
// The first: a turn can issue several tool calls at once and they share one
// cache write, so charging each the whole turn's creation counts it many
// times. The budget is split across the turn's results in proportion to their
// size — the honest division of a figure only ever measured jointly.
//
// The second is worse, because splitting does not fix it. `cache_creation`
// covers everything new since the last write, and a tool result is not the
// only thing that arrives between turns: a pasted message, hook output,
// injected reminders and a re-sent system block all land in the same number.
// Measured on a real session, eleven errored results totalling 3,800
// characters — under a thousand tokens of text — were charged 197,400. The
// exact figure was exactly right about the turn and wildly wrong about the
// result.
//
// So the estimate leads and the measurement corroborates. Characters over
// four is the baseline; the cache figure is used only where it agrees with
// the text to within a factor of a couple, which is where it is describing
// the same thing. Outside that band the turn added something these results
// did not, and the estimate stands.
const CHARS_PER_TOKEN = 4;

function attributeTokens(m) {
  const byTurn = new Map();
  for (const c of m.calls) {
    if (!c.result) continue;
    if (!byTurn.has(c.turnIndex)) byTurn.set(c.turnIndex, []);
    byTurn.get(c.turnIndex).push(c);
  }

  for (const [ti, group] of byTurn) {
    const cur = m.turns[ti];
    const next = m.turns[ti + 1];
    const chars = group.reduce((a, c) => a + c.result.chars, 0) || 1;
    const estimate = chars / CHARS_PER_TOKEN;

    let budget = null;
    if (cur && next && next.created > 0) {
      const exact = next.created - cur.out;
      // The corroboration band. Below it the cache did not capture these
      // results; above it the turn carried something else as well.
      if (exact > estimate * 0.4 && exact < estimate * 2.5) budget = exact;
    }

    for (const c of group) {
      const share = c.result.chars / chars;
      c.result.tokens =
        budget === null
          ? Math.max(1, Math.ceil(c.result.chars / CHARS_PER_TOKEN))
          : Math.max(1, Math.round(budget * share));
      c.result.measured = budget !== null;
    }
  }

  // A result whose call never landed in a turn (a torn line, a resumed
  // transcript) still occupies room.
  for (const r of m.results) if (r.tokens == null) r.tokens = Math.ceil(r.chars / CHARS_PER_TOKEN);
}

const resultTokens = (res) => res.tokens || 0;

// ─── 3. RELEVANCE ──────────────────────────────────────────────────────────
//
// The question it answers: if this agent stopped now and a fresh one picked
// up the task in front of it, how much of this context would it load again?
// Everything else is ballast that the model still has to read past on every
// single turn.
//
// This is a PROXY, and the breakdown is printed so it can be argued with
// rather than believed. It scores each tool result's tokens into four buckets
// and reports the split, so a number you distrust can be checked against the
// thing that produced it.

const STALE_CREDIT = 0.35; // background context is not worthless, just not load-bearing
const RETREAD_FLOOR = 2000; // below this a repeat fetch is too cheap to be waste

function relevanceStat(m, win) {
  const { set: focus, files } = focusSet(m);
  const lastCompact = m.compactions.length ? m.compactions[m.compactions.length - 1].index : -1;

  // A FILE read again later is superseded by that later copy — the earlier
  // one is dead weight the window is still carrying. This holds for paths and
  // nothing else: two shell commands are not two copies of one thing, and
  // treating them as such condemns most of a healthy session.
  const lastReadOf = new Map();
  for (const c of m.calls) for (const p of c.paths) lastReadOf.set(p, c);

  // Only what is STILL in the window can be scored. Content before the last
  // compaction was dropped and replaced by a summary, so counting its original
  // results put 776k of weight inside a 663k window — the ratio was being
  // taken over everything the session ever loaded rather than over what it is
  // carrying now, which is the thing the question is about. The summary that
  // replaced them is counted at its real size, and counted as live: it is by
  // construction a statement of the task.
  const summaryTokens = m.compactions.length ? m.compactions[m.compactions.length - 1].post : 0;

  // Which results are still resident. A transcript is the whole history; the
  // window is its tail. Summing every result a 621-turn session ever loaded
  // gave 1.6M tokens of "context" inside a 456k window — a denominator over
  // everything that was ever true rather than over what is loaded now, and
  // the ratio taken against it means nothing.
  //
  // So walk back from the newest call, accumulating weight until it fills the
  // measured window, and score that. What falls off the back has fallen out
  // of the context too.
  const resident = [];
  let acc = summaryTokens;
  for (let i = m.calls.length - 1; i >= 0; i--) {
    if (i < lastCompact) break; // dropped at the boundary; the summary stands for it
    const c = m.calls[i];
    if (!c.result) continue;
    resident.push(i);
    acc += resultTokens(c.result);
    if (acc >= win.tokens) break;
  }
  resident.reverse();

  const bucket = { live: summaryTokens, stale: 0, superseded: 0, errored: 0 };
  let total = summaryTokens;

  for (const i of resident) {
    const c = m.calls[i];
    const t = resultTokens(c.result);
    total += t;
    const superseded = c.paths.length > 0 && c.paths.every((p) => lastReadOf.get(p) !== c);

    if (c.result.error) {
      bucket.errored += t;
    } else if (superseded) {
      bucket.superseded += t;
    } else if (c.paths.some((p) => focus.has(p) || focus.has(parentDir(p)))) {
      bucket.live += t;
    } else if (c.paths.length === 0 && focus.has(c.key)) {
      bucket.live += t;
    } else {
      bucket.stale += t;
    }
  }

  const useful = bucket.live + STALE_CREDIT * bucket.stale;
  const score = total ? (useful / total) * 100 : 100;

  return {
    score,
    buckets: bucket,
    totalResultTokens: total,
    summaryTokens,
    residentCalls: resident.length,
    focusSize: focus.size,
    // The files it is working on, not the directories they sit in — a focus
    // line reading "Work, home, parker, release" names no work.
    focus: files.slice(0, 10),
    drift: driftStat(m),
    // The number the shutdown call actually uses: tokens the window is
    // carrying that a fresh agent on this task would not load.
    ballastTokens: Math.round(total ? (1 - useful / total) * win.tokens : 0),
  };
}

// The working set: what the agent has been touching since the human last
// spoke. A prompt is the cleanest topic boundary a transcript has — it is
// where the task was last restated. Below a floor it widens to the last dozen
// calls, so a long unattended run still gets a focus rather than an empty set.
function focusSet(m) {
  const lastPrompt = m.prompts.length ? m.prompts[m.prompts.length - 1].ts : null;
  let recent = lastPrompt ? m.calls.filter((c) => c.ts >= lastPrompt) : [];
  if (recent.length < 12) recent = m.calls.slice(-12);
  const set = new Set();
  const files = [];
  for (const c of recent) {
    for (const p of c.paths) {
      if (!set.has(p)) files.push(p);
      set.add(p);
      const d = parentDir(p);
      if (d) set.add(d);
    }
    if (c.paths.length === 0) set.add(c.key);
  }
  return { set, files };
}

// How far the session has moved from what it was opened to do. Reported
// beside relevance rather than folded into it — a session that legitimately
// changed topic is not a session with a bad score, it is a session whose early
// context should be dropped.
function driftStat(m) {
  const withPaths = m.calls.filter((c) => c.paths.length);
  if (withPaths.length < 20) return { value: 0, note: "too short to judge" };
  // Compared on the directories it worked in MOST, not every one it touched.
  // A path-level set comparison reads 0.9 drift on every session, because two
  // quarters of any real session touch different files; a whole-set directory
  // comparison is barely better, because one glance at an unrelated tree
  // counts the same as forty edits. The top few by call count are the topic.
  const q = Math.floor(withPaths.length * 0.25);
  const top = (cs) => {
    const n = new Map();
    for (const c of cs) for (const p of c.paths) {
      const d = parentDir(p) || p;
      n.set(d, (n.get(d) || 0) + 1);
    }
    return new Set([...n.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([d]) => d));
  };
  const early = top(withPaths.slice(0, q));
  const late = top(withPaths.slice(-q));
  let shared = 0;
  for (const t of late) if (early.has(t)) shared++;
  const union = new Set([...early, ...late]).size;
  const jaccard = union ? shared / union : 0;
  const note = jaccard >= 0.5 ? "same ground" : jaccard >= 0.2 ? "widened" : "moved on";
  return { value: round(1 - jaccard, 2), note };
}

// ─── the call ──────────────────────────────────────────────────────────────
//
// The whole point. WINDOW alone says "full", which is not a decision — a full
// context is fine if the agent is nearly done, and fatal if it is not. It is
// WINDOW crossed with RELEVANCE that says what to do about it, because the two
// full-context failures need opposite treatments and look the same on a token
// counter.

function verdict(win, fat, rel) {
  const fill = win.score;
  const r = rel.score;
  const f = fat.score;

  if (f >= 70) {
    return {
      call: "STOP",
      colour: "red",
      why: `fatigue ${Math.round(f)} — ${topFatigue(fat)}. A fresh session on the same task starts cleaner than this one continues.`,
    };
  }
  if (fill >= 85 && r >= 65) {
    return {
      call: "HAND OFF",
      colour: "red",
      why: `${pct(fill)} full and ${pct(r)} of it is load-bearing. Compaction here throws away detail that is still in use — write the brief, start fresh against it.`,
    };
  }
  if (fill >= 85) {
    return {
      call: "COMPACT",
      colour: "amber",
      why: `${pct(fill)} full, ${fmt(rel.ballastTokens)} of it ballast. Compaction is cheap right now.`,
    };
  }
  if (fill >= 60 && r < 45) {
    return {
      call: "COMPACT",
      colour: "amber",
      why: `only ${pct(r)} of the window still serves the task — ${fmt(rel.ballastTokens)} is being re-read every turn for nothing.`,
    };
  }
  // Ballast in absolute tokens, not as a share. A 1M window at 55% full reads
  // as comfortable and can still be carrying a quarter of a million tokens
  // that no longer serve the task — paid for again on every single turn. The
  // ratio hides that; the count does not.
  //
  // Scaled to the window, because a flat threshold judges a 1M context by a
  // 200k yardstick and fires on nearly every long session, and a call that is
  // always the same call is not a call.
  if (rel.ballastTokens >= Math.max(120_000, win.limit * 0.25)) {
    return {
      call: "COMPACT",
      colour: "amber",
      why: `${fmt(rel.ballastTokens)} of ballast re-read every turn — ${pct(r)} relevant at ${pct(fill)} full. Compaction pays for itself here.`,
    };
  }
  if (win.turnsLeft !== null && win.turnsLeft <= 10) {
    return {
      call: "WATCH",
      colour: "amber",
      why: `about ${win.turnsLeft} turns of headroom at the current rate (${fmt(win.growthPerTurn)}/turn).`,
    };
  }
  if (fill >= 60 || f >= 45) {
    return { call: "WATCH", colour: "amber", why: `${pct(fill)} full, fatigue ${Math.round(f)}. Nothing wrong yet.` };
  }
  // Room to spare. Say so without calling a 28%-relevant window healthy —
  // it is untidy, it is just not urgent, and the headroom is the reason.
  const room = win.turnsLeft === null ? "room to spare" : `~${win.turnsLeft} turns of headroom`;
  return r < 50
    ? { call: "RUN", colour: "green", why: `${pct(fill)} full with ${room}. Only ${pct(r)} relevant, but there is room to carry it.` }
    : { call: "RUN", colour: "green", why: `${pct(fill)} full, ${pct(r)} relevant, ${room}.` };
}

function topFatigue(fat) {
  const ranked = Object.entries(fat.parts)
    .map(([k, v]) => [k, v * FATIGUE_WEIGHTS[k]])
    .sort((a, b) => b[1] - a[1]);
  const names = {
    scars: `${fat.compactions} compaction${fat.compactions === 1 ? "" : "s"}, ${fmt(fat.droppedTokens)} tokens dropped`,
    errors: `tool errors climbing (${pct(fat.errorRate * 100)} of calls)`,
    churn: "repeating calls it has already made",
    latency: "turns slowing per token produced",
    retread: "re-reading what it already had",
    age: `${fat.hours}h on the clock`,
  };
  return names[ranked[0][0]];
}

// ─── rendering ─────────────────────────────────────────────────────────────

const C = {
  reset: "\x1b[0m",
  dim: "\x1b[2m",
  bold: "\x1b[1m",
  green: "\x1b[38;5;114m",
  amber: "\x1b[38;5;179m",
  red: "\x1b[38;5;174m",
  blue: "\x1b[38;5;110m",
};
const useColour = process.stdout.isTTY && !process.env.NO_COLOR;
const c = (name, s) => (useColour ? C[name] + s + C.reset : s);

// Bars are drawn at a fixed 28 cells so three of them line up as a block the
// eye reads in one pass. Narrow panes are the normal case here, not the edge
// one — see the tiled-width rule in the house docs.
const BAR_W = 28;

function bar(value, tone) {
  const filled = Math.round((clamp01(value / 100) * BAR_W));
  return c(tone, "█".repeat(filled)) + c("dim", "░".repeat(BAR_W - filled));
}

// WINDOW and FATIGUE are bad when high; RELEVANCE is bad when low.
const toneHigh = (v) => (v >= 85 ? "red" : v >= 60 ? "amber" : "green");
const toneLow = (v) => (v <= 40 ? "red" : v <= 65 ? "amber" : "green");

function render(v) {
  const L = [];
  const { win, fat, rel, call } = v;

  L.push("");
  L.push(`${c("bold", v.label)}  ${c("dim", v.model || "?")}  ${c("dim", v.cwd || "")}`);
  L.push("");
  L.push(`  WINDOW     ${bar(win.score, toneHigh(win.score))}  ${pct(win.score).padStart(4)}   ${fmt(win.tokens)} / ${fmt(win.limit)}`);
  L.push(`  FATIGUE    ${bar(fat.score, toneHigh(fat.score))}  ${pct(fat.score).padStart(4)}   ${topFatigue(fat)}`);
  L.push(`  RELEVANCE  ${bar(rel.score, toneLow(rel.score))}  ${pct(rel.score).padStart(4)}   ${fmt(rel.ballastTokens)} ballast`);
  L.push("");
  L.push(`  ${c(call.colour, c("bold", "▸ " + call.call))}  ${call.why}`);
  L.push("");

  if (v.detail) {
    const headroom = win.turnsLeft === null ? "—" : `${win.turnsLeft} turns`;
    L.push(c("dim", `  window     ${win.turns} turns · +${fmt(win.growthPerTurn)}/turn · ${headroom} of headroom · peak ${fmt(win.peak)}`));
    L.push(
      c(
        "dim",
        `  fatigue    ` +
          Object.entries(fat.parts)
            .map(([k, val]) => `${k} ${Math.round(val * 100)}`)
            .join(" · ")
      )
    );
    L.push(
      c(
        "dim",
        `  relevance  ` +
          Object.entries(rel.buckets)
            .map(([k, val]) => `${k} ${fmt(val)}`)
            .join(" · ") +
          ` · drift ${rel.drift.value} (${rel.drift.note})`
      )
    );
    L.push(c("dim", `  session    ${fat.calls} calls · ${fat.errors} errors · ${fat.hours}h${v.costUSD ? ` · $${v.costUSD.toFixed(2)}` : ""}`));
    L.push(c("dim", `  focus      ${[...new Set(rel.focus.map(shortPath))].join(", ") || "—"}`));
    L.push("");
  }
  return L.join("\n");
}

// Worst first. Matches the wall's own rule that the agents needing you are
// drawn ahead of the ones that do not.
const URGENCY = ["STOP", "HAND OFF", "COMPACT", "WATCH", "RUN"];

// One line for a fleet of seventeen, because scrolling seventeen cards to
// find the one that needs a decision is the problem, not the answer.
function fleetSummary(out) {
  const n = (call) => out.filter((v) => v.call.call === call).length;
  const tone = { STOP: "red", "HAND OFF": "red", COMPACT: "amber", WATCH: "amber", RUN: "green" };
  const parts = URGENCY.filter((k) => n(k)).map((k) => c(tone[k], `${n(k)} ${k.toLowerCase()}`));
  const ballast = out.reduce((a, v) => a + v.rel.ballastTokens, 0);
  return `  ${c("bold", `${out.length} agents`)}  ${parts.join(c("dim", " · "))}  ${c("dim", `· ${fmt(ballast)} ballast across the fleet`)}\n`;
}

// ─── session discovery ─────────────────────────────────────────────────────
//
// Ask Terminal Delight. It is already tracking the fleet — that is what the
// agent wall IS — and every running instance answers `mcp rpc` on its own
// control socket under `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`.
// `list_panes` returns each pane's pid, cwd, title and resumable session, so
// the set this script reports is by construction the set the wall draws.
//
// This is the correction to a wrong first answer. The td-agent-ledger hook is
// not installed on this machine, the ledger directory is empty, and I read
// that as "live-fleet discovery is unavailable" — while the wall sat there
// showing seventeen Claude agents. TD never depended on the ledger; it prefers
// it and falls back to forensics (app/src/session.rs). An empty ledger is one
// source being quiet, not the fleet being invisible.
//
// Order: TD's own answer, then the process table, then the ledger, then the
// newest transcript for a directory. Each is a strictly worse-informed guess
// than the one before it.

const CLAUDE_PROJECTS = join(homedir(), ".claude", "projects");
const LEDGER = join(homedir(), ".local/state/terminal-delight/agent-ledger");
const CTL_DIR = join(process.env.XDG_RUNTIME_DIR ?? "/tmp", "terminal-delight");

const LIST_PANES = JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "tools/call",
  params: { name: "list_panes", arguments: {} },
});

/** One request/response on an instance's control socket. `null` if it is deaf. */
function askTd(path) {
  return new Promise((done) => {
    let buf = "";
    const s = createConnection({ path }, () => s.write(`mcp rpc ${LIST_PANES}\n`));
    const finish = (v) => {
      s.destroy();
      done(v);
    };
    // `mcp rpc` waits on the gpui main thread, so this is the server's snapshot
    // budget plus slack, not a network-shaped timeout.
    s.setTimeout(5000, () => finish(buf || null));
    s.on("data", (d) => {
      buf += d;
      if (buf.includes("\n")) finish(buf.trim());
    });
    s.on("error", () => done(null));
  });
}

// ─── binding a pane to its own transcript ──────────────────────────────────
//
// TD's `session` field is not always a fact. A pane launched as bare `claude`
// carries no `--resume`, so TD reconstructs the id forensically from cwd and
// process start — and for two panes sitting in the SAME cwd, observed here on
// two terminal-delight panes, it handed back the same id for both. Deduping on
// that reported one agent where there were two, and drew the wrong bars for
// the survivor.
//
// So the reported id is trusted only when the agent process itself says it,
// via --resume. Otherwise each pane is bound to its own transcript by birth —
// a session started by a bare `claude` writes a transcript whose first record
// lands at about the moment the process started — and claimed one-to-one, so
// two panes can never take the same conversation.

const CLK_TCK = 100; // Linux USER_HZ; constant on every kernel this runs on

function bootTime() {
  try {
    const m = readFileSync("/proc/stat", "utf8").match(/^btime (\d+)/m);
    return m ? Number(m[1]) : null;
  } catch {
    return null;
  }
}

/** Unix seconds at which a process started, from field 22 of /proc/<pid>/stat. */
function procStart(pid, btime) {
  if (!btime) return null;
  try {
    const stat = readFileSync(`/proc/${pid}/stat`, "utf8");
    // The comm field is parenthesised and may itself contain spaces and
    // brackets, so fields are counted from after the LAST ')'.
    const after = stat.slice(stat.lastIndexOf(")") + 2).split(" ");
    const ticks = Number(after[19]); // field 22 overall, 20th after comm+state
    return Number.isFinite(ticks) ? btime + ticks / CLK_TCK : null;
  } catch {
    return null;
  }
}

/** The `claude` process under a pane's shell. TD reports the shell's pid. */
function agentUnder(panePid, depth = 0) {
  if (depth > 3) return null;
  let kids = [];
  try {
    for (const task of readdirSync(`/proc/${panePid}/task`)) {
      const c = readFileSync(`/proc/${panePid}/task/${task}/children`, "utf8").trim();
      if (c) kids.push(...c.split(/\s+/).map(Number));
    }
  } catch {
    return null;
  }
  for (const k of kids) {
    try {
      if (readFileSync(`/proc/${k}/comm`, "utf8").trim() === "claude") return k;
    } catch {
      /* it exited while we looked */
    }
  }
  for (const k of kids) {
    const deeper = agentUnder(k, depth + 1);
    if (deeper) return deeper;
  }
  return null;
}

// When a conversation began and when it was last SPOKEN into.
//
// Not the file's mtime. Claude Code writes bookkeeping records — leafUuid,
// last-prompt — back into transcripts a conversation has already left, so an
// abandoned session's file can be newer on disk than the live one's. Trusting
// mtime bound a pane to a conversation that had ended twenty minutes earlier,
// while the one it was actually speaking into sat there looking stale.
//
// Nor is it line one, which is routinely a `leafUuid`/`sessionId` record with
// no timestamp at all. Both ends scan for the first record that carries one,
// and a file with none is unbindable rather than silently ranked first.

const EDGE_BYTES = 256 * 1024;
const BIRTH_TOLERANCE = 90; // seconds between a process starting and its first record

function edgeTimestamps(path) {
  let fd = null;
  try {
    fd = openSync(path, "r");
    const size = fstatSync(fd).size;

    const head = Buffer.alloc(Math.min(EDGE_BYTES, size));
    readSync(fd, head, 0, head.length, 0);
    const began = firstStamp(head.toString("utf8").split("\n"));

    const from = Math.max(0, size - EDGE_BYTES);
    const tail = Buffer.alloc(Math.min(EDGE_BYTES, size));
    readSync(fd, tail, 0, tail.length, from);
    // A window into the middle of the file starts mid-line; that fragment is
    // not JSON and is skipped like any other unparseable line.
    const spoke = firstStamp(tail.toString("utf8").split("\n").reverse());

    return { began, spoke };
  } catch {
    return { began: null, spoke: null };
  } finally {
    if (fd !== null) try { closeSync(fd); } catch { /* already gone */ }
  }
}

function firstStamp(lines) {
  for (const line of lines) {
    if (!line || line[0] !== "{") continue;
    try {
      const ts = JSON.parse(line)?.timestamp;
      if (ts) {
        const n = Date.parse(ts);
        if (Number.isFinite(n)) return n / 1000;
      }
    } catch {
      /* a torn or truncated line is not a timestamp */
    }
  }
  return null;
}

// Bind panes to transcripts one-to-one. A pane whose agent named its own
// session takes that one outright; the rest are matched to the unclaimed
// transcript in their own project directory that began nearest to when their
// process did.
function bindSessions(panes, transcripts) {
  const byId = new Map(transcripts.map((t) => [t.id, t]));
  const bySlug = new Map();
  for (const t of transcripts) {
    if (!bySlug.has(t.slug)) bySlug.set(t.slug, []);
    bySlug.get(t.slug).push(t);
  }

  const claimed = new Set();
  const out = [];
  const deferred = [];

  for (const p of panes) {
    if (p.declared && byId.has(p.declared) && !claimed.has(p.declared)) {
      claimed.add(p.declared);
      out.push({ ...p, id: p.declared, ...byId.get(p.declared) });
    } else {
      deferred.push(p);
    }
  }

  const scored = deferred.map((p) => {
    const slug = p.cwd ? resolve(p.cwd).replace(/\//g, "-") : null;
    const pool = (slug ? bySlug.get(slug) || [] : []).map((t) => {
      if (t.edges === undefined) t.edges = edgeTimestamps(t.path);
      return t;
    });
    return { p, pool };
  });

  // Phase one: birth. A bare `claude` opens its conversation as it starts, and
  // the two timestamps agree to within seconds — measured at 3s and 2s on the
  // two addev agents. That is a near-identification, and it is assigned
  // globally tightest-first so the best-evidenced pair claims before a looser
  // one can take its transcript.
  //
  // Ranking by recency instead put these two agents on each OTHER's
  // conversation: the transcript last spoken into was not the one this process
  // opened, and both panes drew bars for their neighbour's context.
  const bound = new Set();
  const pairs = [];
  for (const { p, pool } of scored) {
    if (!p.startedAt) continue;
    for (const t of pool) {
      if (t.edges.began === null) continue;
      const d = Math.abs(t.edges.began - p.startedAt);
      if (d <= BIRTH_TOLERANCE) pairs.push({ p, t, d });
    }
  }
  pairs.sort((a, b) => a.d - b.d);
  for (const { p, t } of pairs) {
    if (bound.has(p.pid) || claimed.has(t.id)) continue;
    claimed.add(t.id);
    bound.add(p.pid);
    out.push({ ...p, id: t.id, ...t });
  }

  // Phase two: a /clear mints a new conversation while the process runs on, so
  // an agent's current transcript can begin hours after it started and match
  // no birth. Among what began after the process did, the one last spoken into
  // is the one it is in now — 6697c339 over 5f07a6ad, whose conversation had
  // ended twenty minutes earlier while its file kept being touched.
  for (const { p, pool } of scored) {
    if (bound.has(p.pid)) continue;
    const free = pool.filter((t) => !claimed.has(t.id) && t.edges.spoke !== null);
    if (free.length === 0) continue;
    const floor = p.startedAt ? p.startedAt - BIRTH_TOLERANCE : null;
    const eligible = floor ? free.filter((t) => t.edges.began === null || t.edges.began >= floor) : free;
    const pick = (eligible.length ? eligible : free).reduce((a, b) => (b.edges.spoke > a.edges.spoke ? b : a));
    claimed.add(pick.id);
    out.push({ ...p, id: pick.id, ...pick });
  }
  return out;
}

// Every agent pane of every live TD. Panes are kept distinct — two panes are
// two agents even when TD reports the same session for both.
async function tdFleet() {
  let socks;
  try {
    socks = readdirSync(CTL_DIR).filter((f) => f.startsWith("ctl-") && f.endsWith(".sock"));
  } catch {
    return [];
  }

  const btime = bootTime();
  const seen = new Map(); // keyed by PANE pid, so two panes stay two agents
  for (const f of socks) {
    const owner = Number(f.slice(4, -5));
    if (!existsSync(`/proc/${owner}`)) continue; // a socket outliving its process
    const reply = await askTd(join(CTL_DIR, f));
    if (!reply) continue;
    let panes;
    try {
      panes = JSON.parse(reply)?.result?.structuredContent?.panes ?? [];
    } catch {
      continue;
    }
    for (const p of panes) {
      if (!p.is_agent || seen.has(p.pid)) continue;
      const agent = agentUnder(p.pid) ?? p.pid;
      let cmdline = "";
      try {
        cmdline = readFileSync(`/proc/${agent}/cmdline`, "utf8").replace(/\0/g, " ");
      } catch {
        /* the pane is there, the process just went */
      }
      seen.set(p.pid, {
        panePid: p.pid,
        pid: agent,
        cwd: p.cwd,
        title: p.title,
        tool: p.tool ?? null,
        // Only the agent's OWN command line is a declaration. TD's `session`
        // is a reconstruction, and two same-cwd panes can reconstruct alike.
        declared: resumeId(cmdline),
        startedAt: procStart(agent, btime),
      });
    }
  }
  return [...seen.values()];
}

// No TD running, or none answering. The agents are still in the process table,
// and Claude Code puts the session id in its own command line on a resume.
function procFleet() {
  const btime = bootTime();
  const out = [];
  let pids;
  try {
    pids = readdirSync("/proc").filter((d) => /^\d+$/.test(d));
  } catch {
    return [];
  }
  for (const pid of pids) {
    // The process has to BE claude, not merely mention it. Matching the word
    // anywhere in the command line picked up a bash process whose cwd was
    // /tmp/claude-1000/<project>/<uuid>/scratchpad and reported that uuid as a
    // live agent — a scratchpad directory name read as a session.
    let comm;
    try {
      comm = readFileSync(`/proc/${pid}/comm`, "utf8").trim();
    } catch {
      continue; // it exited between the listing and the read
    }
    if (comm !== "claude") continue;

    let cmdline;
    try {
      cmdline = readFileSync(`/proc/${pid}/cmdline`, "utf8").replace(/\0/g, " ");
    } catch {
      continue;
    }
    let cwd = null;
    try {
      cwd = readlinkSync(`/proc/${pid}/cwd`);
    } catch {
      /* a process we cannot follow is still an agent */
    }
    out.push({
      panePid: Number(pid),
      pid: Number(pid),
      cwd,
      title: null,
      tool: null,
      declared: resumeId(cmdline),
      startedAt: procStart(pid, btime),
    });
  }
  return out;
}

// td-agent-ledger writes a file per live agent pid on every session-id mint,
// so where it IS installed it names the current id even across a /clear or a
// compaction — the one case a cmdline `--resume` goes stale.
function ledgerFleet() {
  if (!existsSync(LEDGER)) return [];
  const btime = bootTime();
  const out = [];
  for (const f of readdirSync(LEDGER)) {
    if (!f.endsWith(".json")) continue;
    const pid = Number(basename(f, ".json"));
    if (!existsSync(`/proc/${pid}`)) continue; // a crash never fires SessionEnd
    try {
      const e = JSON.parse(readFileSync(join(LEDGER, f), "utf8"));
      if (!e.session_id) continue;
      let cwd = null;
      try {
        cwd = readlinkSync(`/proc/${pid}/cwd`);
      } catch {
        /* still a usable declaration */
      }
      // The ledger is written by the agent itself on every id mint, so its id
      // IS a declaration — the one source that stays right across a /clear.
      out.push({ panePid: pid, pid, cwd, title: null, tool: null, declared: e.session_id, startedAt: procStart(pid, btime) });
    } catch {
      /* a torn ledger entry is not worth failing the run over */
    }
  }
  return out;
}

const UUID = "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}";
const RESUME = new RegExp(`--resume[= ]+(${UUID})\\b`);
const BARE = new RegExp(`^${UUID}$`);

// TD reports a pane's session as the command that would resume it, so the id
// is the argument to --resume. Taking the first uuid ANYWHERE in the string
// is what let a scratchpad path masquerade as a session id — every temp
// directory under /tmp/claude-<uid>/ is named after one.
function resumeId(s) {
  if (typeof s !== "string") return null;
  const m = s.match(RESUME);
  if (m) return m[1];
  const bare = s.trim();
  return BARE.test(bare) ? bare : null; // what a ledger entry carries
}

// Merged on the AGENT PROCESS, which is the one identity every source agrees
// on and the only one that cannot collide: two panes are two processes even
// when TD reconstructs the same session id for both.
async function liveFleet(transcripts) {
  const merged = new Map();
  for (const e of [...ledgerFleet(), ...procFleet(), ...(await tdFleet())]) {
    const prev = merged.get(e.pid);
    // Later sources win on the fields they carry; a declaration already made
    // is never overwritten by a source that has none.
    merged.set(e.pid, prev ? { ...prev, ...e, declared: e.declared ?? prev.declared, title: e.title ?? prev.title } : e);
  }
  return bindSessions([...merged.values()], transcripts);
}

function allTranscripts() {
  if (!existsSync(CLAUDE_PROJECTS)) return [];
  const out = [];
  for (const d of readdirSync(CLAUDE_PROJECTS)) {
    const dir = join(CLAUDE_PROJECTS, d);
    let entries;
    try {
      entries = readdirSync(dir);
    } catch {
      continue; // an owner-only project dir we cannot read is not an error
    }
    for (const f of entries) {
      if (!f.endsWith(".jsonl")) continue;
      const p = join(dir, f);
      out.push({ id: basename(f, ".jsonl"), path: p, mtime: statSync(p).mtimeMs, slug: d });
    }
  }
  return out.sort((a, b) => b.mtime - a.mtime);
}

function transcriptFor(target) {
  if (target.endsWith(".jsonl") && existsSync(target)) return { id: basename(target, ".jsonl"), path: target };
  const hit = allTranscripts().find((t) => t.id === target);
  return hit || null;
}

function newestForCwd(cwd) {
  const slug = resolve(cwd).replace(/\//g, "-");
  return allTranscripts().find((t) => t.slug === slug) || null;
}

// ─── assembly ──────────────────────────────────────────────────────────────

async function vitals(entry, detail, override = null) {
  const rows = await readTranscript(entry.path);
  const m = buildModel(rows);
  attributeTokens(m);
  if (m.turns.length === 0) return null;

  const win = windowStat(m, override);
  const fat = fatigueStat(m, win);
  const rel = relevanceStat(m, win);
  const call = verdict(win, fat, rel);

  // The pane's own title when TD gave us one — "✳ Video upload performance"
  // is what Parker is looking at on the wall; a uuid prefix is not.
  const short = entry.id.slice(0, 8);
  const label = entry.title ? `${entry.title}  ${c("dim", short)}` : entry.pid ? `${short} · pid ${entry.pid}` : short;

  return {
    id: entry.id,
    pid: entry.pid ?? null,
    title: entry.title ?? null,
    tool: entry.tool ?? null,
    label,
    path: entry.path,
    cwd: entry.cwd || rows.find((r) => r.cwd)?.cwd || null,
    model: m.model,
    costUSD: m.cost?.totalCostUSD ?? null,
    updated: m.ended,
    win,
    fat,
    rel,
    call,
    detail,
  };
}

// The wall's contract. Flat, three scores plus the call, with the evidence
// hung underneath — a card can draw the bars from the top four fields and
// never parse the rest.
function toJson(v) {
  return {
    session: v.id,
    pid: v.pid,
    title: v.title,
    tool: v.tool,
    cwd: v.cwd,
    model: v.model,
    updated: v.updated ? new Date(v.updated).toISOString() : null,
    bars: {
      window: round(v.win.score, 1),
      fatigue: round(v.fat.score, 1),
      relevance: round(v.rel.score, 1),
    },
    call: v.call.call,
    colour: v.call.colour,
    why: v.call.why,
    window: v.win,
    fatigue: v.fat,
    relevance: v.rel,
    costUSD: v.costUSD,
  };
}

// ─── helpers ───────────────────────────────────────────────────────────────

const clamp01 = (x) => (Number.isFinite(x) ? Math.max(0, Math.min(1, x)) : 0);
const mean = (a) => (a.length ? a.reduce((x, y) => x + y, 0) / a.length : 0);
const round = (x, n = 2) => Number(x.toFixed(n));
const pct = (x) => `${Math.round(x)}%`;

function median(a) {
  if (!a.length) return 0;
  const s = [...a].sort((x, y) => x - y);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
}

// The last two segments — `app/src/main.rs` reads; the basename alone gives
// four files called `main.rs` and no way to tell them apart.
function shortPath(p) {
  const parts = p.split("/").filter(Boolean);
  return parts.slice(-2).join("/");
}

function fmt(n) {
  if (n === null || n === undefined) return "—";
  const abs = Math.abs(n);
  if (abs >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(1)}k`;
  return String(Math.round(n));
}

// ─── cli ───────────────────────────────────────────────────────────────────

const USAGE = `td-agent-vitals — three bars and a shutdown call, from an agent's own transcript

  node scripts/td-agent-vitals.mjs                 every live agent (TD ledger)
  node scripts/td-agent-vitals.mjs --all           every session, newest first
  node scripts/td-agent-vitals.mjs <session-id>
  node scripts/td-agent-vitals.mjs <path.jsonl>
  node scripts/td-agent-vitals.mjs --cwd <dir>     newest session for a directory

  --json        machine output, one object per session (the wall's contract)
  --detail      show the evidence under each bar
  --limit N     with --all, how many (default 5)
  --window N    pin the context limit, e.g. --window 1000000

The context limit is read from the cost-state record, the only place a
transcript spells out the 1M variant — the per-message model field says plain
claude-opus-5 either way. Until that record is written, a young 1M session is
judged against 200k and reads as fuller than it is. It self-corrects once the
session accrues cost or exceeds 200k; --window pins it in the meantime.
`;

async function main() {
  const argv = process.argv.slice(2);
  if (argv.includes("-h") || argv.includes("--help")) {
    process.stdout.write(USAGE);
    return;
  }

  const json = argv.includes("--json");
  const detail = argv.includes("--detail") || argv.includes("-d");
  const all = argv.includes("--all");
  const limitArg = argv.indexOf("--limit");
  const limit = limitArg >= 0 ? Number(argv[limitArg + 1]) || 5 : 5;
  const winArg = argv.indexOf("--window");
  const override = winArg >= 0 ? Number(argv[winArg + 1]) || null : null;
  const cwdArg = argv.indexOf("--cwd");
  const flagged = new Set(["--limit", "--cwd", "--window"]);
  const positional = argv.filter((a, i) => !a.startsWith("-") && !flagged.has(argv[i - 1]));

  let entries = [];
  if (cwdArg >= 0) {
    const hit = newestForCwd(argv[cwdArg + 1] || process.cwd());
    if (hit) entries = [hit];
  } else if (positional.length) {
    for (const p of positional) {
      const hit = transcriptFor(p);
      if (hit) entries.push(hit);
      else process.stderr.write(`no transcript for ${p}\n`);
    }
  } else if (all) {
    entries = allTranscripts().slice(0, limit);
  } else {
    // A live agent whose transcript has not been written yet is real but has
    // nothing to measure; bindSessions leaves it out rather than reporting it
    // as an empty one.
    entries = (await liveFleet(allTranscripts())).filter((e) => e.path);
    if (entries.length === 0) {
      // Nothing running, or nothing answering. The newest session for this
      // directory is a more useful answer than none.
      const hit = newestForCwd(process.cwd());
      if (hit) entries = [hit];
    }
  }

  if (entries.length === 0) {
    process.stderr.write("no agent transcripts found\n");
    process.exitCode = 1;
    return;
  }

  const out = [];
  for (const e of entries) {
    try {
      const v = await vitals(e, detail, override);
      if (v) out.push(v);
    } catch (err) {
      process.stderr.write(`${e.id}: ${err.message}\n`);
    }
  }

  // Worst first, the way the wall draws blocked and errored agents ahead of
  // the rest: the reason to run this is to find the agent that needs a
  // decision, and that agent should not be twelfth.
  out.sort((a, b) => URGENCY.indexOf(a.call.call) - URGENCY.indexOf(b.call.call) || b.win.score - a.win.score);

  if (json) {
    process.stdout.write(JSON.stringify(out.map(toJson), null, 2) + "\n");
    return;
  }
  for (const v of out) process.stdout.write(render(v) + "\n");
  if (out.length > 1) process.stdout.write(fleetSummary(out) + "\n");
}

// Exported for scripts/td-agent-vitals.test.mjs. Every one of these has been
// wrong once in a way that printed a plausible number rather than failing, so
// they are tested against transcripts built to have a known answer.
export {
  attributeTokens,
  bindSessions,
  buildModel,
  contextLimit,
  fatigueStat,
  relevanceStat,
  resumeId,
  targetsOf,
  verdict,
  windowStat,
};

// `import` for a test must not run the CLI.
if (process.argv[1] && resolve(process.argv[1]) === resolve(new URL(import.meta.url).pathname)) {
  main().catch((e) => {
    process.stderr.write(`${e.stack || e}\n`);
    process.exitCode = 1;
  });
}
