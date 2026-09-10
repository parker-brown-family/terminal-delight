#!/usr/bin/env node
// A hands-on lab for the session host, deliberately unable to touch a live
// Terminal Delight.
//
// Three separations, because this runs on the machine TD is developed on:
//
//   1. A different BINARY — the debug build in this worktree, never the one on
//      your PATH. It is printed on every `up` so there is no guessing.
//   2. A different RUNTIME DIRECTORY — the host names its socket after the
//      session and puts it under XDG_RUNTIME_DIR, so the lab points that at
//      /tmp and its socket cannot land beside your real ones.
//   3. No windows at all. The host has no user interface and never links one;
//      nothing here can open, adopt, or write a session of yours. It writes
//      exactly one thing: its own socket.
//
// Usage:
//   node scripts/td-host-lab.mjs up            start a host (session "lab")
//   node scripts/td-host-lab.mjs spawn [cwd]   start a terminal in it
//   node scripts/td-host-lab.mjs list          what it is holding
//   node scripts/td-host-lab.mjs attach <id>   type into a terminal; ctrl-] leaves
//   node scripts/td-host-lab.mjs kill <id>     close a pane — the verb that kills
//   node scripts/td-host-lab.mjs down          stop the host
//
// The demonstration worth doing:
//   up → spawn → attach → run something slow, `top` or a build → ctrl-] →
//   attach again. The work never stopped, and the second attach is shown
//   everything it missed. Then `down`, and watch it all go.

import net from "node:net";
import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "..");
const BINARY = path.join(REPO, "app", "target", "debug", "terminal-delight");
const SESSION = "lab";
const ROOT = path.join(os.tmpdir(), `td-host-lab-${os.userInfo().username}`);
const RUNTIME = path.join(ROOT, "run");
const SOCKET = path.join(RUNTIME, "terminal-delight", `session-${SESSION}.sock`);
const LOG = path.join(ROOT, "host.log");
const PROTO = 1;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function die(message) {
  console.error(`td-host-lab: ${message}`);
  process.exit(1);
}

/// One control conversation: connect, say hello, send the verbs, collect the
/// replies, hang up.
function control(requests) {
  return new Promise((resolve, reject) => {
    const socket = net.connect(SOCKET);
    const replies = [];
    let pending = [
      JSON.stringify({ verb: "hello", proto: PROTO, kind: "tool" }),
      ...requests,
    ];
    let buffer = "";
    socket.on("error", reject);
    socket.on("connect", () => socket.write(pending.shift() + "\n"));
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
      let cut;
      while ((cut = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, cut);
        buffer = buffer.slice(cut + 1);
        if (line.trim()) replies.push(JSON.parse(line));
        if (pending.length) socket.write(pending.shift() + "\n");
        else {
          socket.end();
          // drop the hello's own reply: the caller asked for the rest
          resolve(replies.slice(1));
          return;
        }
      }
    });
  });
}

async function up() {
  if (!fs.existsSync(BINARY)) {
    die(
      `no binary at ${BINARY}\n` +
        `  build it first:  cd ${path.join(REPO, "app")} && cargo build`,
    );
  }
  if (fs.existsSync(SOCKET)) {
    try {
      const [seen] = await control([JSON.stringify({ verb: "list-panes" })]);
      console.log(`already up — ${seen.panes.length} pane(s)`);
      return;
    } catch {
      fs.rmSync(SOCKET, { force: true });
    }
  }
  fs.mkdirSync(RUNTIME, { recursive: true });

  const log = fs.openSync(LOG, "a");
  const child = spawn(BINARY, ["serve", "--session", SESSION], {
    detached: true,
    stdio: ["ignore", log, log],
    env: {
      ...process.env,
      XDG_RUNTIME_DIR: RUNTIME,
      // Belt and braces: the host writes no configuration or state today, and
      // if that ever changes this makes sure it changes somewhere harmless.
      XDG_CONFIG_HOME: path.join(ROOT, "config"),
      XDG_STATE_HOME: path.join(ROOT, "state"),
      XDG_DATA_HOME: path.join(ROOT, "data"),
    },
  });
  child.unref();
  fs.writeFileSync(path.join(ROOT, "host.pid"), String(child.pid));

  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline && !fs.existsSync(SOCKET)) await sleep(25);
  if (!fs.existsSync(SOCKET)) die(`the host never bound its socket — see ${LOG}`);

  console.log(`host up — pid ${child.pid}, session "${SESSION}"`);
  console.log(`  binary  ${BINARY}`);
  console.log(`  socket  ${SOCKET}`);
  console.log(`  log     ${LOG}`);
  console.log(
    `\nyour installed terminal-delight is untouched:\n  ${
      fs.existsSync(`${os.homedir()}/.local/bin/terminal-delight`)
        ? fs.realpathSync(`${os.homedir()}/.local/bin/terminal-delight`)
        : "(none installed)"
    }`,
  );
  console.log(`\nnext:  node scripts/td-host-lab.mjs spawn`);
}

async function spawn_pane(cwd) {
  const geom = {
    cols: process.stdout.columns || 100,
    rows: process.stdout.rows || 30,
    cell_width: 8,
    cell_height: 16,
  };
  const [reply] = await control([
    JSON.stringify({ verb: "spawn-pane", cwd: cwd ?? process.cwd(), geom }),
  ]);
  if (!reply.outcome.ok) die(`could not start a terminal: ${reply.outcome.err}`);
  const { pane, shell_pid } = reply.outcome.ok;
  console.log(`pane ${pane} — process ${shell_pid}`);
  console.log(`attach:  node scripts/td-host-lab.mjs attach ${pane}`);
}

async function list() {
  const [reply] = await control([JSON.stringify({ verb: "list-panes" })]);
  if (!reply.panes.length) {
    console.log("no panes — try: node scripts/td-host-lab.mjs spawn");
    return;
  }
  for (const p of reply.panes) {
    console.log(
      `pane ${p.pane}  process ${p.shell_pid}  ${
        p.attached ? "attached" : "nobody watching"
      }${p.ended ? "  (ended)" : ""}  ${p.geom.cols}x${p.geom.rows}`,
    );
    // What the host's own clocks have learned. A dash is not a default: it is
    // the host saying it has not looked yet, which for a fresh pane it has not.
    console.log(
      `  running ${name(p.mode)}   in ${p.cwd ?? "—"}${
        p.resume ? `   resume: ${p.resume}` : ""
      }`,
    );
  }
}

/// A mode as the wire spells it: a plain name, or a program's own.
function name(mode) {
  if (mode === null || mode === undefined) return "—";
  return typeof mode === "string" ? mode : Object.values(mode)[0];
}

/// Type into a terminal that belongs to another process.
///
/// This is the whole feature in one command: what happens here survives this
/// command going away, however it goes away.
async function attach(id) {
  const pane = Number(id);
  if (!Number.isInteger(pane)) die("usage: attach <pane-id>");

  // Tell the host the size first: a terminal's size is a fact the owner is
  // told, never one it infers, because more than one client may be looking.
  const geom = {
    cols: process.stdout.columns || 100,
    rows: process.stdout.rows || 30,
    cell_width: 8,
    cell_height: 16,
  };
  const [resized] = await control([
    JSON.stringify({ verb: "resize", pane, geom }),
  ]);
  if (resized.outcome && resized.outcome.err) die(resized.outcome.err);

  const socket = net.connect(SOCKET);
  socket.on("error", (e) => die(`${e.message}`));
  await new Promise((r) => socket.on("connect", r));

  const tty = process.stdin.isTTY;
  // Borrow the alternate screen for the duration.
  //
  // A snapshot begins by clearing the screen and the scrollback, which is
  // exactly right for a window that owns its terminal and exactly wrong here,
  // where the terminal is yours and full of your work. On the alternate screen
  // the snapshot lands on a scratch page, and detaching hands back the screen
  // you had, unscrolled and unerased.
  if (tty) process.stdout.write("\x1b[?1049h");
  if (tty) process.stdin.setRawMode(true);
  socket.write(`stream ${pane}\n`);

  // Said once the snapshot has landed, because the snapshot would have wiped
  // it. A working thing that shows no sign of working reads as a broken one —
  // which is what a bare shell prompt on an otherwise blank screen looks like.
  const banner = () => {
    if (!tty) return;
    const rows = process.stdout.rows || 30;
    process.stdout.write(
      `\x1b7\x1b[${rows};1H\x1b[2K\x1b[7m pane ${pane} · this terminal belongs to the host · ctrl-] detaches, it keeps running \x1b[0m\x1b8`,
    );
  };
  setTimeout(banner, 250);

  // Give the terminal back, however this ends. Borrowing the alternate screen
  // and raw mode means a client that dies without tidying up leaves a person
  // with a terminal that does not echo and does not show their work — so the
  // restore hangs off exit itself, not off the paths that expect to be taken.
  let restored = false;
  const restore = () => {
    if (restored || !tty) return;
    restored = true;
    try {
      process.stdin.setRawMode(false);
    } catch {}
    process.stdout.write("\x1b[?1049l");
  };
  process.on("exit", restore);

  const leave = (why) => {
    restore();
    socket.destroy();
    process.stderr.write(`\x1b[2m— ${why} —\x1b[0m\n`);
    process.exit(0);
  };
  for (const signal of ["SIGTERM", "SIGHUP", "SIGINT"]) {
    process.on(signal, () => leave(`stopped by ${signal}; the terminal keeps running`));
  }

  socket.on("data", (chunk) => process.stdout.write(chunk));
  socket.on("close", () => leave("the host closed this stream"));
  process.stdin.on("data", (chunk) => {
    // ctrl-] — the same key telnet has used to mean "let me out" since 1983.
    if (chunk.includes(0x1d)) leave("detached; the terminal is still running");
    socket.write(chunk);
  });
  process.on("SIGWINCH", () => {
    control([
      JSON.stringify({
        verb: "resize",
        pane,
        geom: {
          cols: process.stdout.columns,
          rows: process.stdout.rows,
          cell_width: 8,
          cell_height: 16,
        },
      }),
    ]).catch(() => {});
    setTimeout(banner, 100);
  });
}

async function killPane(id) {
  const pane = Number(id);
  if (!Number.isInteger(pane)) die("usage: kill <pane-id>");
  const [reply] = await control([JSON.stringify({ verb: "close-pane", pane })]);
  if (reply.outcome.err) die(reply.outcome.err);
  console.log(
    `pane ${pane} closed — process ${reply.outcome.ok.shell_pid} ${
      reply.outcome.ok.signalled ? "hung up" : "was already gone"
    }`,
  );
}

async function down() {
  if (!fs.existsSync(SOCKET)) {
    console.log("nothing running");
    return;
  }
  try {
    await control([JSON.stringify({ verb: "shutdown" })]);
  } catch {
    /* it may hang up before replying, which is what we asked for */
  }
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline && fs.existsSync(SOCKET)) await sleep(25);
  console.log("host down");
}

const [command, ...rest] = process.argv.slice(2);
const commands = { up, spawn: spawn_pane, list, attach, kill: killPane, down };
if (!commands[command]) {
  console.log(
    [
      "usage: node scripts/td-host-lab.mjs <command>",
      "",
      "  up            start an isolated session host",
      "  spawn [cwd]   start a terminal inside it",
      "  list          what it is holding",
      "  attach <id>   type into one; ctrl-] leaves it running",
      "  kill <id>     close a pane — the verb that kills",
      "  down          stop the host",
    ].join("\n"),
  );
  process.exit(command ? 2 : 0);
}
commands[command](...rest).catch((e) => die(e.message));
