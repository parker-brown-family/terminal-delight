#!/usr/bin/env node
// Tests for td-agent-ledger.
//
// The hook's job is to say which mints continue a conversation and which begin
// one, because a workbench keyed on the conversation empties itself when that
// answer is wrong. Every case here is a way of getting it wrong that would
// still write a plausible-looking entry: a chain silently restarted, a chain
// silently joined on no evidence, a source nobody validated reaching a reader.
//
// Several mints must land on ONE pid to be a rotation at all, and the hook
// reads its pid from $PPID. So a scenario is a list of payloads run inside a
// single `bash -c`, which makes that shell the shared parent — the same shape
// as one agent process minting several times.
//
//   node --test scripts/td-agent-ledger.test.mjs

import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, readdirSync, existsSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const HOOK = join(dirname(fileURLToPath(import.meta.url)), "td-agent-ledger");

/** Run a list of hook payloads as one process, so they share a pid. */
function scenario(payloads) {
  const home = mkdtempSync(join(tmpdir(), "td-ledger-"));
  const script = payloads
    .map((p) => `printf '%s' ${JSON.stringify(JSON.stringify(p))} | ${HOOK}`)
    .join("\n");
  execFileSync("bash", ["-c", script], { env: { ...process.env, HOME: home } });
  const dir = join(home, ".local/state/terminal-delight/agent-ledger");
  const entries = existsSync(dir)
    ? readdirSync(dir)
        .filter((f) => f.endsWith(".json"))
        .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")))
    : [];
  const lineagePath = join(dir, "lineage.jsonl");
  const lineage = existsSync(lineagePath)
    ? readFileSync(lineagePath, "utf8").trim().split("\n").filter(Boolean).map((l) => JSON.parse(l))
    : [];
  rmSync(home, { recursive: true, force: true });
  return { entries, lineage };
}

const start = (session_id, source) => ({
  hook_event_name: "SessionStart",
  session_id,
  ...(source === undefined ? {} : { source }),
});
const end = (session_id) => ({ hook_event_name: "SessionEnd", session_id });

const A = "aaaaaaaa-1111-4111-8111-aaaaaaaaaaaa";
const B = "bbbbbbbb-2222-4222-8222-bbbbbbbbbbbb";
const C = "cccccccc-3333-4333-8333-cccccccccccc";

// ─── a conversation's first mint ───────────────────────────────────────────

test("a pid nobody has seen roots the chain on its own id", () => {
  const { entries } = scenario([start(A, "startup")]);
  assert.equal(entries.length, 1);
  assert.equal(entries[0].session_id, A);
  assert.equal(entries[0].root, A);
  assert.equal(entries[0].seq, 0);
  assert.equal(entries[0].source, "startup");
});

test("a first mint has no prev and no join, rather than empty ones", () => {
  const { entries } = scenario([start(A, "startup")]);
  assert.ok(!("prev" in entries[0]), "a first mint displaced nothing");
  assert.ok(!("join" in entries[0]), "a first mint joined nothing");
});

// ─── the same id, minted again ─────────────────────────────────────────────

test("the same id minted again does not advance the chain", () => {
  // The measured case: four of sixteen live agents on 2026-09-21 had a second
  // SessionStart 10 to 48 hours in, and the id had not changed.
  const { entries } = scenario([start(A, "startup"), start(A, "compact")]);
  assert.equal(entries[0].session_id, A);
  assert.equal(entries[0].root, A);
  assert.equal(entries[0].seq, 0, "same id is the same conversation at the same depth");
  assert.equal(entries[0].source, "compact", "but the reason is the new one");
});

// ─── a rotation that continues, and one that breaks ────────────────────────

test("a compaction onto a new id keeps the root and deepens the chain", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "compact")]);
  assert.equal(entries[0].session_id, B);
  assert.equal(entries[0].root, A, "the bench follows the conversation, not the id");
  assert.equal(entries[0].seq, 1);
  assert.equal(entries[0].prev, A);
  assert.equal(entries[0].join, "declared");
});

test("an in-pane resume onto a new id also continues", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "resume")]);
  assert.equal(entries[0].root, A);
  assert.equal(entries[0].join, "declared");
});

test("a clear begins a new conversation and names what it displaced", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "clear")]);
  assert.equal(entries[0].root, B, "a clear is a new conversation, so a new root");
  assert.equal(entries[0].seq, 0);
  assert.equal(entries[0].prev, A, "the id it replaced is still recorded");
  assert.ok(!("join" in entries[0]), "a break is not a join");
});

test("a chain three deep still names the conversation it started as", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "compact"), start(C, "compact")]);
  assert.equal(entries[0].root, A);
  assert.equal(entries[0].seq, 2);
  assert.equal(entries[0].prev, B);
});

test("a clear mid-chain resets the depth as well as the root", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "compact"), start(C, "clear")]);
  assert.equal(entries[0].root, C);
  assert.equal(entries[0].seq, 0);
});

// ─── unknown is not zero ───────────────────────────────────────────────────

test("a mint with no source continues the chain and says nothing evidenced it", () => {
  const { entries } = scenario([start(A, "startup"), start(B, undefined)]);
  assert.equal(entries[0].root, A, "continuing is the recoverable error");
  assert.equal(entries[0].join, "unknown", "and it is never passed off as declared");
  assert.ok(!("source" in entries[0]), "a source nobody sent is absent, not empty");
});

test("a source outside the four known words is treated as no source at all", () => {
  const { entries } = scenario([start(A, "startup"), start(B, "wormhole")]);
  assert.ok(!("source" in entries[0]), "an unrecognised word never reaches a reader");
  assert.equal(entries[0].join, "unknown");
});

test("a first mint with no source is still a root, not an unknown join", () => {
  const { entries } = scenario([start(A, undefined)]);
  assert.equal(entries[0].root, A);
  assert.ok(!("join" in entries[0]));
});

// ─── refusing what cannot be written down ──────────────────────────────────

test("a session id that is not a plain id is refused outright", () => {
  const { entries } = scenario([start("../../etc/passwd", "startup")]);
  assert.equal(entries.length, 0, "nothing that could name a path gets written");
});

test("a session id carrying shell metacharacters is refused", () => {
  const { entries } = scenario([start("abc; rm -rf /", "startup")]);
  assert.equal(entries.length, 0);
});

test("an empty session id writes nothing rather than an entry with no id", () => {
  const { entries } = scenario([start("", "startup")]);
  assert.equal(entries.length, 0);
});

// ─── the durable half ──────────────────────────────────────────────────────

test("the lineage outlives the process whose entry was removed", () => {
  const { entries, lineage } = scenario([start(A, "startup"), start(B, "compact"), end(B)]);
  assert.equal(entries.length, 0, "SessionEnd takes the live entry away");
  assert.equal(lineage.length, 3, "and leaves every mint plus the end behind");
  assert.equal(lineage[0].event, "start");
  assert.equal(lineage[2].event, "end");
  assert.equal(lineage[2].root, A, "the end still names the conversation, not the last id");
});

test("every lineage line carries the root, so a chain can be walked without the pid file", () => {
  const { lineage } = scenario([start(A, "startup"), start(B, "compact"), start(C, "clear")]);
  assert.deepEqual(
    lineage.map((l) => [l.session_id, l.root, l.seq]),
    [
      [A, A, 0],
      [B, A, 1],
      [C, C, 0],
    ],
  );
});

test("an end with no live entry appends nothing rather than a half-filled line", () => {
  const { lineage } = scenario([end(A)]);
  assert.equal(lineage.length, 0);
});

test("the lineage is bounded", () => {
  // The cap is 5000; write past it and assert the file stops growing rather
  // than asserting an exact length, which would pin the test to the constant.
  const many = [];
  for (let i = 0; i < 40; i++) many.push(start(A, "compact"));
  const { lineage } = scenario(many);
  assert.ok(lineage.length <= 5000);
  assert.equal(lineage.length, 40, "under the cap, nothing is dropped");
});

test("a forged previous entry cannot carry a root into the next mint", () => {
  // The per-pid file is writable by any process running as this user, and the
  // hook reads it back to compute the chain. A root that is not a plain id must
  // not survive that read — it would become a store directory name.
  const home = mkdtempSync(join(tmpdir(), "td-ledger-forge-"));
  const dir = join(home, ".local/state/terminal-delight/agent-ledger");
  execFileSync("bash", ["-c", `mkdir -p ${JSON.stringify(dir)}`]);
  const script = [
    `printf '%s' ${JSON.stringify(JSON.stringify(start(A, "startup")))} | ${HOOK}`,
    // a hostile hand reaches in between two legitimate mints
    `jq '.root = "../../etc/passwd"' "$(ls ${JSON.stringify(dir)}/*.json)" > /tmp/forged.$$ && mv /tmp/forged.$$ ${JSON.stringify(dir)}/$(basename "$(ls ${JSON.stringify(dir)}/*.json)")`,
    `printf '%s' ${JSON.stringify(JSON.stringify(start(B, "compact")))} | ${HOOK}`,
  ].join("\n");
  execFileSync("bash", ["-c", script], { env: { ...process.env, HOME: home } });
  const entry = JSON.parse(
    readFileSync(join(dir, readdirSync(dir).find((f) => f.endsWith(".json"))), "utf8"),
  );
  assert.notEqual(entry.root, "../../etc/passwd", "a forged root never propagates");
  assert.ok(/^[A-Za-z0-9_-]+$/.test(entry.root), `root must stay a plain id, got ${entry.root}`);
  rmSync(home, { recursive: true, force: true });
});
