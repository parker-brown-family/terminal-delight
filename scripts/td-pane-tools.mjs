#!/usr/bin/env node
// Ask every LIVE terminal-delight what each of its agent panes is holding.
//
// A pane's logo follows the tool its agent is using (see `app/src/toolprop.rs`).
// Checking that it is wearing the RIGHT thing used to mean photographing the
// screen — which is the standing complaint in issue #236, and which cost two
// rounds of "I looked and saw nothing" before the real fault was found. It was
// not the render at all: the feature was switched off by a `#[derive(Default)]`,
// and a screenshot cannot tell those two apart.
//
// So ask the process instead. `mcp::PaneInfo` carries the resolved tool, and
// every instance answers `mcp rpc` on its own control socket under
// `$XDG_RUNTIME_DIR/terminal-delight/ctl-<pid>.sock`. This talks to ALL of them,
// which also answers the other question that wasted a round: whether the window
// you are looking at is even running the binary you just built.
//
//   node scripts/td-pane-tools.mjs
//
// A pane shows `—` when its agent is idle or between calls: the face is only
// set while an agent is working, and that is the design, not a fault.

import { createConnection } from "node:net";
import { readdirSync, readlinkSync } from "node:fs";
import { basename, join } from "node:path";

const DIR = join(process.env.XDG_RUNTIME_DIR ?? "/tmp", "terminal-delight");
const RPC = JSON.stringify({
  jsonrpc: "2.0",
  id: 1,
  method: "tools/call",
  params: { name: "list_panes", arguments: {} },
});

/** One request/response on an instance's control socket. `null` if it is deaf. */
const ask = (path) =>
  new Promise((resolve) => {
    let buf = "";
    const s = createConnection({ path }, () => s.write(`mcp rpc ${RPC}\n`));
    const done = (v) => {
      s.destroy();
      resolve(v);
    };
    // `mcp rpc` waits on the gpui main thread, so give it the server's snapshot
    // budget plus slack rather than a network-shaped timeout.
    s.setTimeout(5000, () => done(buf || null));
    s.on("data", (d) => {
      buf += d;
      if (buf.includes("\n")) done(buf.trim());
    });
    s.on("error", () => resolve(null));
  });

let sockets;
try {
  sockets = readdirSync(DIR).filter((f) => f.startsWith("ctl-") && f.endsWith(".sock"));
} catch {
  console.error(`no control-socket directory at ${DIR} — is terminal-delight running?`);
  process.exit(1);
}

let answered = 0;
for (const f of sockets) {
  const pid = Number(f.slice(4, -5));
  let exe;
  try {
    exe = basename(readlinkSync(`/proc/${pid}/exe`));
  } catch {
    continue; // a socket left behind by a process that is gone
  }

  const reply = await ask(join(DIR, f));
  if (!reply) {
    console.log(`── pid ${pid} · ${exe} · no answer (MCP exposure off, or busy)`);
    continue;
  }

  let panes;
  try {
    const env = JSON.parse(reply);
    // The structured half carries the whole PaneInfo including `tool`; the text
    // half is the human listing and does not parse as JSON.
    panes = env?.result?.structuredContent?.panes ?? [];
  } catch {
    console.log(`── pid ${pid} · ${exe} · unparseable reply`);
    continue;
  }

  answered++;
  const agents = panes.filter((p) => p.is_agent);
  console.log(`\n── pid ${pid} · ${exe} · ${agents.length} agent panes`);
  if (!agents.some((p) => p.tool)) {
    console.log("   (nothing holding a tool: every agent idle, the feature off, or an older binary)");
  }
  for (const p of agents) {
    console.log(`   ${String(p.title).slice(0, 44).padEnd(46)} ${p.tool ?? "—"}`);
  }
}

if (!answered) {
  console.log("no live terminal-delight answered on a control socket");
  process.exit(1);
}
