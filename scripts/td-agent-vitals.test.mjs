#!/usr/bin/env node
// Tests for td-agent-vitals.
//
// Every case here is a bug that shipped a plausible number rather than an
// error, which is the only failure mode that matters for a metric: nothing
// crashes, a bar draws, and the call it produces is wrong. Each test builds a
// transcript whose answer is known by construction and asserts the measure
// recovers it.
//
//   node --test scripts/td-agent-vitals.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";
import {
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
} from "./td-agent-vitals.mjs";

// ─── session discovery ─────────────────────────────────────────────────────

test("a pane's session id is the argument to --resume", () => {
  assert.equal(
    resumeId("claude --resume 9c3b95e3-a6f5-4358-9b5d-48c0b9614886"),
    "9c3b95e3-a6f5-4358-9b5d-48c0b9614886"
  );
});

test("a uuid in a scratchpad path is not a session id", () => {
  // Every temp directory under /tmp/claude-<uid>/ is named after a session,
  // so taking the first uuid anywhere in a command line reported a bash
  // process as a live agent — with real bars drawn for it.
  const cmdline =
    "/usr/bin/bash -c cd /tmp/claude-1000/-home-parker-Work-terminal-delight/" +
    "f738ed09-e6b3-4579-9f02-23fb8d4d79ed/scratchpad && nohup chromium";
  assert.equal(resumeId(cmdline), null);
});

test("a bare id, which is what the ledger stores, is accepted", () => {
  assert.equal(
    resumeId("3ef6a985-6c77-45cc-9be4-f8c6e4dd11fa"),
    "3ef6a985-6c77-45cc-9be4-f8c6e4dd11fa"
  );
});

test("a shell pane, which has no session at all, yields nothing", () => {
  assert.equal(resumeId(undefined), null);
  assert.equal(resumeId("parker@legion:~/Work/conclave"), null);
});

// ─── binding a pane to its own transcript ──────────────────────────────────
//
// Both cases below are real, taken off this machine with seventeen agents
// running. `edges` is pre-filled so these never touch the disk.

const T = (id, slug, began, spoke) => ({
  id,
  slug,
  path: `/fake/${id}.jsonl`,
  edges: { began: began === null ? null : Date.parse(began) / 1000, spoke: Date.parse(spoke) / 1000 },
});
const P = (pid, cwd, startedAt, declared = null) => ({
  pid,
  panePid: pid,
  cwd,
  declared,
  startedAt: startedAt === null ? null : Date.parse(startedAt) / 1000,
});
const ADDEV = "-home-parker-Work-addev";

test("an agent is bound to the conversation that began when IT started", () => {
  // The three addev agents. Ranking by "last spoken into" put two of them on
  // each other's transcript: e6bcd0ef was the most recently active, so the
  // first pane considered took it — but it was opened by the OTHER process,
  // two seconds after that one started.
  const transcripts = [
    T("e6bcd0ef", ADDEV, "2026-09-02T20:15:46Z", "2026-09-03T00:47:42Z"),
    T("f88ffe3f", ADDEV, "2026-09-02T18:37:24Z", "2026-09-02T18:39:37Z"),
    T("104e5de3", ADDEV, "2026-09-02T20:15:46Z", "2026-09-02T21:49:09Z"),
  ];
  const panes = [
    P(3862538, "/home/parker/Work/addev", "2026-09-02T18:37:21Z"),
    P(784585, "/home/parker/Work/addev", "2026-09-02T20:15:44Z"),
    P(1194661, "/home/parker/Work/addev", "2026-09-02T20:42:58Z", "104e5de3"),
  ];
  const byPid = new Map(bindSessions(panes, transcripts).map((e) => [e.pid, e.id]));
  assert.equal(byPid.get(3862538), "f88ffe3f");
  assert.equal(byPid.get(784585), "e6bcd0ef");
  assert.equal(byPid.get(1194661), "104e5de3");
});

test("after a /clear the agent is in the newer conversation, not the one it opened", () => {
  // The Akshat pane: process up since 20:26Z, but the conversation it is in
  // began at 22:26Z. Its predecessor 5f07a6ad stopped being spoken into at
  // 22:23 — while its FILE kept being touched, which is why mtime is not the
  // signal and the last record is.
  const BFS = "-home-parker-BROWN-FAMILY-SPORTS";
  const transcripts = [
    T("5f07a6ad", BFS, "2026-09-02T21:15:13Z", "2026-09-02T22:23:52Z"),
    T("6697c339", BFS, "2026-09-02T22:26:06Z", "2026-09-02T22:41:22Z"),
  ];
  const panes = [P(2879008, "/home/parker/BROWN-FAMILY-SPORTS", "2026-09-02T20:26:04Z")];
  assert.equal(bindSessions(panes, transcripts)[0].id, "6697c339");
});

test("two panes never share one conversation", () => {
  // TD's own forensic answer gave two same-cwd panes the same session id.
  // Deduping on that reported one agent where there were two.
  const transcripts = [
    T("aaaaaaaa", ADDEV, "2026-09-02T10:00:00Z", "2026-09-02T12:00:00Z"),
    T("bbbbbbbb", ADDEV, "2026-09-02T11:00:00Z", "2026-09-02T13:00:00Z"),
  ];
  const panes = [
    P(111, "/home/parker/Work/addev", "2026-09-02T10:00:00Z"),
    P(222, "/home/parker/Work/addev", "2026-09-02T11:00:00Z"),
  ];
  const got = bindSessions(panes, transcripts);
  assert.equal(got.length, 2);
  assert.equal(new Set(got.map((e) => e.id)).size, 2);
});

test("a declared --resume id outranks any forensic guess", () => {
  const transcripts = [
    T("aaaaaaaa", ADDEV, "2026-09-02T10:00:00Z", "2026-09-02T23:00:00Z"),
    T("bbbbbbbb", ADDEV, "2026-09-02T11:00:00Z", "2026-09-02T12:00:00Z"),
  ];
  // Birth and recency both point at aaaaaaaa; the agent says otherwise.
  const panes = [P(111, "/home/parker/Work/addev", "2026-09-02T10:00:00Z", "bbbbbbbb")];
  assert.equal(bindSessions(panes, transcripts)[0].id, "bbbbbbbb");
});

// ─── transcript fixtures ───────────────────────────────────────────────────

let clock = 0;
const ts = () => new Date((clock += 30_000)).toISOString();

function assistant({ ctx = 0, created = 0, out = 100, model = "claude-opus-5", calls = [] }) {
  return {
    type: "assistant",
    timestamp: ts(),
    message: {
      model,
      usage: {
        input_tokens: 2,
        cache_creation_input_tokens: created,
        cache_read_input_tokens: Math.max(0, ctx - created - 2),
        output_tokens: out,
      },
      content: calls.map((c, i) => ({ type: "tool_use", id: c.id || `t${clock}_${i}`, name: c.name, input: c.input })),
    },
  };
}

function results(items) {
  return {
    type: "user",
    timestamp: ts(),
    message: {
      content: items.map((r) => ({
        type: "tool_result",
        tool_use_id: r.id,
        is_error: r.error === true,
        content: "x".repeat(r.chars ?? 400),
      })),
    },
  };
}

const prompt = (text) => ({ type: "user", timestamp: ts(), message: { content: text } });

function model(rows) {
  const m = buildModel(rows);
  attributeTokens(m);
  return m;
}

// A session of `n` read/result pairs over `files`, each result `chars` long.
function session(n, files, chars = 4000, opts = {}) {
  const rows = [prompt("do the thing")];
  let ctx = 20_000;
  for (let i = 0; i < n; i++) {
    const id = `c${i}`;
    const file = files[i % files.length];
    ctx += chars / 4;
    rows.push(assistant({ ctx, created: chars / 4, calls: [{ id, name: "Read", input: { file_path: file } }] }));
    rows.push(results([{ id, chars, error: opts.error?.(i) === true }]));
  }
  rows.push(assistant({ ctx, created: 0, out: 50 }));
  return rows;
}

// ─── context limit ─────────────────────────────────────────────────────────

test("the 1M variant is read from cost-state, which is the only record that carries it", () => {
  // `.message.model` says plain `claude-opus-5` even in a 1M session. Trusting
  // it reported a 550k window as 275% full and told you to compact an agent
  // with 450k of headroom.
  const m = { model: "claude-opus-5", cost: { modelUsage: { "claude-opus-5[1m]": {} } } };
  assert.equal(contextLimit(m, 549_905), 1_000_000);
});

test("a window that HELD more than the nominal limit is not the nominal limit", () => {
  // No cost-state at all — the transcript is the remaining evidence, and a
  // context observed at 550k tokens cannot be a 200k context.
  assert.equal(contextLimit({ model: "claude-opus-5", cost: null }, 549_905), 1_000_000);
});

test("an ordinary session keeps the nominal limit", () => {
  assert.equal(contextLimit({ model: "claude-opus-5", cost: null }, 120_000), 200_000);
});

// ─── window ────────────────────────────────────────────────────────────────

test("window size is the sum of the last turn's three input figures", () => {
  const m = model([assistant({ ctx: 40_000, created: 1_000 }), assistant({ ctx: 60_000, created: 2_000 })]);
  const w = windowStat(m);
  assert.equal(w.tokens, 60_000);
  assert.equal(w.limit, 200_000);
  assert.equal(Math.round(w.score), 30);
});

test("headroom is counted in turns at the recent growth rate, ignoring compaction drops", () => {
  const rows = [];
  let ctx = 10_000;
  for (let i = 0; i < 14; i++) rows.push(assistant({ ctx: (ctx += 1_000) }));
  const w = windowStat(model(rows));
  assert.equal(w.growthPerTurn, 1_000);
  // 92% of 200k is 184k; from 24k at 1k a turn.
  assert.equal(w.turnsLeft, 160);
});

// ─── token attribution ─────────────────────────────────────────────────────

test("parallel tool calls in one turn split that turn's cache write, never each take all of it", () => {
  const rows = [
    assistant({ ctx: 10_000, created: 0, out: 100, calls: [{ id: "a", name: "Read", input: { file_path: "a.rs" } }, { id: "b", name: "Read", input: { file_path: "b.rs" } }] }),
    results([{ id: "a", chars: 4_000 }, { id: "b", chars: 4_000 }]),
    assistant({ ctx: 12_100, created: 2_100, out: 50 }),
  ];
  const m = model(rows);
  const [a, b] = m.results;
  // 2,100 written minus 100 output = 2,000, split evenly by size.
  assert.equal(a.tokens, 1_000);
  assert.equal(b.tokens, 1_000);
});

test("a cache figure that dwarfs the text it supposedly measures is not believed", () => {
  // The real case: eleven errored results totalling 3,800 characters were
  // charged 197,400 tokens, because cache_creation also covers pasted
  // messages, hook output and injected reminders. A 300-character error is
  // not 18k tokens, whatever the turn's cache write says.
  const rows = [
    assistant({ ctx: 10_000, created: 0, out: 100, calls: [{ id: "a", name: "Bash", input: { command: "false" } }] }),
    results([{ id: "a", chars: 300, error: true }]),
    assistant({ ctx: 60_100, created: 50_100, out: 100 }),
  ];
  const m = model(rows);
  assert.equal(m.results[0].tokens, 75); // 300 chars over four, not 50,000
  assert.equal(m.results[0].measured, false);
});

// ─── call targets ──────────────────────────────────────────────────────────

test("a path is lifted out of a shell command, not just off a file_path argument", () => {
  const t = targetsOf("Bash", { command: "grep -n foo app/src/main.rs" });
  assert.ok(t.paths.includes("app/src/main.rs"));
});

test("a dotted module name is not a path", () => {
  // `python3 -m http.server` contributed `http.server` to the focus set, and
  // one junk entry there scores unrelated work as on-topic.
  const t = targetsOf("Bash", { command: "python3 -m http.server 8000" });
  assert.deepEqual(t.paths, []);
});

test("two different shell commands are two different calls", () => {
  // Keying a path-less call by tool name alone made every shell call in a
  // session supersede every other one, and scored a healthy transcript at 8%
  // relevant.
  const a = targetsOf("Bash", { command: "git status" });
  const b = targetsOf("Bash", { command: "cargo test" });
  assert.notEqual(a.key, b.key);
});

// ─── relevance ─────────────────────────────────────────────────────────────

test("a session reading a set of files once each, all in one tree, scores high", () => {
  const files = Array.from({ length: 20 }, (_, i) => `app/src/f${i}.rs`);
  const m = model(session(20, files));
  assert.ok(relevanceStat(m, windowStat(m)).score > 70);
});

test("the same file fetched twenty times is nineteen dead copies", () => {
  // Not a defect in the measure — the window really is carrying nineteen
  // superseded copies of one file, and that is what low relevance means.
  const m = model(session(20, ["app/src/main.rs"]));
  const r = relevanceStat(m, windowStat(m));
  assert.ok(r.score < 40, `scored ${r.score}`);
  assert.ok(r.buckets.superseded > r.buckets.live);
});

test("a re-read supersedes the copy it replaces", () => {
  const m = model(session(20, ["a.rs", "b.rs"]));
  const r = relevanceStat(m, windowStat(m));
  assert.ok(r.buckets.superseded > 0, "earlier copies of a re-read file are dead weight");
});

test("only a FILE is superseded by a later call, never one shell command by another", () => {
  const rows = [prompt("go")];
  let ctx = 20_000;
  for (let i = 0; i < 12; i++) {
    const id = `c${i}`;
    ctx += 1_000;
    rows.push(assistant({ ctx, created: 1_000, calls: [{ id, name: "Bash", input: { command: `echo run-${i}` } }] }));
    rows.push(results([{ id, chars: 4_000 }]));
  }
  rows.push(assistant({ ctx, created: 0, out: 50 }));
  const m = model(rows);
  const r = relevanceStat(m, windowStat(m));
  assert.equal(r.buckets.superseded, 0);
});

test("relevance is measured over the window, not over everything ever loaded", () => {
  // Summing every result a long session ever fetched put 1.6M tokens of
  // "context" inside a 456k window. The denominator has to be what is
  // resident, or the ratio is taken against a number that was never true.
  const m = model(session(400, ["a.rs", "b.rs", "c.rs"], 8_000));
  const w = windowStat(m);
  const r = relevanceStat(m, w);
  assert.ok(
    r.totalResultTokens <= w.tokens * 1.1,
    `scored ${r.totalResultTokens} tokens against a ${w.tokens} window`
  );
});

test("content dropped at a compaction boundary leaves the denominator with it", () => {
  const rows = session(30, ["old.rs"], 4_000);
  rows.push({
    type: "system",
    subtype: "compact_boundary",
    timestamp: ts(),
    compactMetadata: { trigger: "manual", preTokens: 180_000, postTokens: 12_000, cumulativeDroppedTokens: 168_000 },
  });
  let ctx = 12_000;
  for (let i = 0; i < 10; i++) {
    const id = `n${i}`;
    ctx += 1_000;
    rows.push(assistant({ ctx, created: 1_000, calls: [{ id, name: "Read", input: { file_path: "new.rs" } }] }));
    rows.push(results([{ id, chars: 4_000 }]));
  }
  rows.push(assistant({ ctx, created: 0, out: 50 }));

  const m = model(rows);
  const r = relevanceStat(m, windowStat(m));
  assert.equal(r.summaryTokens, 12_000);
  assert.ok(r.totalResultTokens < 60_000, "pre-compaction results are not still in the window");
});

// ─── fatigue ───────────────────────────────────────────────────────────────

test("a clean short session is not tired", () => {
  const m = model(session(12, ["a.rs", "b.rs", "c.rs", "d.rs"]));
  const f = fatigueStat(m, windowStat(m));
  assert.ok(f.score < 35, `scored ${f.score}`);
});

test("surviving a compaction is a scar", () => {
  const clean = model(session(20, ["a.rs"]));
  const rows = session(20, ["a.rs"]);
  rows.splice(10, 0, {
    type: "system",
    subtype: "compact_boundary",
    timestamp: ts(),
    compactMetadata: { trigger: "auto", preTokens: 180_000, postTokens: 20_000, cumulativeDroppedTokens: 160_000 },
  });
  const scarred = model(rows);
  const a = fatigueStat(clean, windowStat(clean));
  const b = fatigueStat(scarred, windowStat(scarred));
  assert.equal(a.parts.scars, 0);
  assert.ok(b.parts.scars > 0.3);
  assert.ok(b.score > a.score);
});

test("iterating on two files twice over is not churn", () => {
  // The third time, not the second. Counting a repeat pinned every working
  // session at 100 and measured nothing.
  const m = model(session(4, ["a.rs", "b.rs"]));
  assert.equal(fatigueStat(m, windowStat(m)).parts.churn, 0);
});

test("calling the same thing over and over is churn", () => {
  const m = model(session(30, ["a.rs"]));
  assert.ok(fatigueStat(m, windowStat(m)).parts.churn > 0.5);
});

test("two late errors are not a trend", () => {
  // A session with 2 errors in 81 calls read 100 for "errors climbing",
  // because both happened to land in the recent window and the ratio against
  // a zero baseline is unbounded.
  const m = model(session(40, ["a.rs", "b.rs"], 4_000, { error: (i) => i === 38 || i === 39 }));
  assert.equal(fatigueStat(m, windowStat(m)).parts.errors, 0);
});

test("a rising error rate registers, a flat one does not", () => {
  const flat = model(session(40, ["a.rs", "b.rs"], 4_000, { error: (i) => i % 5 === 0 }));
  const rising = model(session(40, ["a.rs", "b.rs"], 4_000, { error: (i) => i > 30 }));
  const a = fatigueStat(flat, windowStat(flat));
  const b = fatigueStat(rising, windowStat(rising));
  assert.equal(a.parts.errors, 0, "a steady error rate is the task, not fatigue");
  assert.ok(b.parts.errors > 0.5);
});

// ─── the call ──────────────────────────────────────────────────────────────

const at = (fill, rel, fatigue = 10, extra = {}) => ({
  win: { score: fill, tokens: fill * 10_000, limit: 1_000_000, turnsLeft: 200, growthPerTurn: 500, ...extra.win },
  fat: { score: fatigue, parts: { scars: 0, errors: 0, churn: 0, latency: 0, retread: 0, age: 0 }, compactions: 0, droppedTokens: 0, errorRate: 0, hours: 1 },
  rel: { score: rel, ballastTokens: Math.round((1 - rel / 100) * fill * 10_000) },
});

test("a full window whose content is still in use is a hand-off, not a compaction", () => {
  // The whole point of the pair. These two look identical on a token counter
  // and need opposite treatment: compacting here destroys detail in use.
  const v = at(91, 93);
  assert.equal(verdict(v.win, v.fat, v.rel).call, "HAND OFF");
});

test("a full window of ballast is a cheap compaction", () => {
  const v = at(90, 30);
  assert.equal(verdict(v.win, v.fat, v.rel).call, "COMPACT");
});

test("a fresh focused session runs", () => {
  const v = at(20, 90);
  assert.equal(verdict(v.win, v.fat, v.rel).call, "RUN");
});

test("fatigue overrides a comfortable window", () => {
  const v = at(30, 90, 75);
  assert.equal(verdict(v.win, v.fat, v.rel).call, "STOP");
});

test("ballast is judged against the window it sits in, not a flat number", () => {
  // A flat threshold judged a 1M context by a 200k yardstick and fired on
  // nearly every long session. A call that is always the same call is not a
  // call.
  const small = at(70, 40, 10, { win: { limit: 200_000 } });
  small.win.limit = 200_000;
  small.rel.ballastTokens = 130_000;
  assert.equal(verdict(small.win, small.fat, small.rel).call, "COMPACT");

  const large = at(20, 85, 10);
  large.win.limit = 1_000_000;
  large.rel.ballastTokens = 130_000;
  assert.equal(verdict(large.win, large.fat, large.rel).call, "RUN");
});
