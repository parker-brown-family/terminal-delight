// Photograph elements out of a local web page, with nothing but node and a
// chromium already on the box.
//
// Terminal Delight needs agent-playhouse's ROBOT, not just the prop in his
// hand, and that robot is not a file anywhere — he is composed at run time from
// an SVG rig, a stylesheet that poses him by `data-state` / `data-face`, and
// three custom properties. Re-implementing that composition in a build script
// would be the fork this whole pipeline exists to prevent, and it would drift
// the first time somebody changes a pose.
//
// So we photograph the playhouse's own contact sheet. It already renders every
// frame from the real manifest with the real stylesheet — that is what it is
// FOR — which makes it both the art director's review surface and, here, the
// source of truth for the plates. If a pose changes, the photographs change.
//
// No Playwright, no puppeteer: chromium speaks the DevTools protocol over a
// WebSocket, node has had one built in since 22, and the whole driver is the
// hundred lines below. A build step that needs an npm install is a build step
// that stops working.

import { spawn } from "node:child_process";
import { createServer } from "node:http";
import { createReadStream, existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join, normalize } from "node:path";

const MIME = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".mjs": "text/javascript",
  ".css": "text/css",
  ".json": "application/json",
  ".svg": "image/svg+xml",
  ".png": "image/png",
};

/**
 * Serve one directory, read-only, on an ephemeral port. Deliberately not the
 * playhouse's own `bun run src/serve.ts`: the sheet needs four static files and
 * nothing else, and depending on another repo's server would drag its live
 * session-listing endpoints — and its runtime — into our build.
 */
function serveDir(root) {
  return new Promise((resolve) => {
    const srv = createServer((req, res) => {
      const url = new URL(req.url, "http://x");
      // normalize + prefix check: a build script is not a place to invent a new
      // path-traversal hole, however local the server is.
      const path = join(root, normalize(url.pathname));
      if (!path.startsWith(root) || !existsSync(path)) {
        res.writeHead(404).end("no");
        return;
      }
      res.writeHead(200, { "content-type": MIME[extname(path)] ?? "application/octet-stream" });
      createReadStream(path).pipe(res);
    });
    srv.listen(0, "127.0.0.1", () => resolve({ srv, port: srv.address().port }));
  });
}

/** A chromium with the DevTools protocol open, and its port. */
async function launchChromium(bin) {
  const dir = mkdtempSync(join(tmpdir(), "td-shoot-"));
  const proc = spawn(
    bin,
    [
      "--headless=new",
      "--disable-gpu",
      "--hide-scrollbars",
      "--no-first-run",
      "--remote-debugging-port=0",
      `--user-data-dir=${dir}`,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  const portFile = join(dir, "DevToolsActivePort");
  for (let i = 0; i < 150; i++) {
    await new Promise((r) => setTimeout(r, 100));
    if (existsSync(portFile)) {
      const [p] = readFileSync(portFile, "utf8").split("\n");
      if (p?.trim()) return { proc, port: p.trim(), dir };
    }
  }
  proc.kill();
  rmSync(dir, { recursive: true, force: true });
  throw new Error(`${bin} never opened a debugging port`);
}

/** A request/response wrapper over one page's CDP socket. */
async function attach(port) {
  const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("chromium exposed no page target");
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = rej;
  });
  let id = 0;
  const waiting = new Map();
  ws.onmessage = (e) => {
    const m = JSON.parse(e.data);
    if (m.id && waiting.has(m.id)) {
      waiting.get(m.id)(m);
      waiting.delete(m.id);
    }
  };
  const send = (method, params = {}) =>
    new Promise((res, rej) => {
      const n = ++id;
      waiting.set(n, (m) => (m.error ? rej(new Error(`${method}: ${m.error.message}`)) : res(m.result)));
      ws.send(JSON.stringify({ id: n, method, params }));
    });
  return { send, close: () => ws.close() };
}

/** Evaluate an expression in the page and hand back its value. */
async function evaluate(send, expression) {
  const r = await send("Runtime.evaluate", { expression, returnByValue: true, awaitPromise: true });
  if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? "page threw");
  return r.result.value;
}

/**
 * Load `path` from `webDir` and photograph every named cell, `frames` times.
 *
 * `pick` is evaluated IN THE PAGE and returns `[{name, rect}]` — the caller
 * decides what a cell is and how it is framed, because only the caller knows
 * the page.
 *
 * With `frames > 1` the page is shot once per frame with `frameCss(i, frames)`
 * applied, which is how a CSS animation is sampled: pause every animation and
 * give it a negative `animation-delay`, and the browser renders it frozen at
 * that instant. The rects are measured ONCE, before the first frame, so every
 * frame of a cell is the same crop — a per-frame measurement would make the
 * camera bob along with the subject and cancel the motion out.
 *
 * Returns `Map<name, Buffer[]>` — one PNG per frame, in order.
 */
export async function shoot({
  webDir,
  path = "/",
  pick,
  css = "",
  frames = 1,
  frameCss = () => "",
  size = 320,
  scale = 2,
  chromium = "chromium",
}) {
  const { srv, port: httpPort } = await serveDir(webDir);
  let chrome;
  let cdp;
  try {
    chrome = await launchChromium(chromium);
    cdp = await attach(chrome.port);
    const { send } = cdp;

    await send("Page.enable");
    await send("Runtime.enable");
    await send("Emulation.setDeviceMetricsOverride", {
      width: size * 4 + 120,
      height: size * 3,
      deviceScaleFactor: scale,
      mobile: false,
    });
    await send("Page.navigate", { url: `http://127.0.0.1:${httpPort}${path}` });

    // The sheet builds itself from four fetches, so "loaded" is not "drawn".
    // Poll for the thing the page itself only writes once it is finished.
    for (let i = 0; ; i++) {
      if (await evaluate(send, `Boolean(document.querySelector('.grid .cell'))`)) break;
      if (i > 150) throw new Error("the page never rendered a cell");
      await new Promise((r) => setTimeout(r, 100));
    }

    // Size the cells for the camera. Injected rather than asked of the
    // playhouse: its layout is right for a human reviewing a sheet, and this is
    // a camera rig, not an opinion about their page.
    const style = (id, text) =>
      evaluate(
        send,
        `(() => { let s = document.getElementById(${JSON.stringify(id)});
          if (!s) { s = document.createElement('style'); s.id = ${JSON.stringify(id)}; document.head.appendChild(s); }
          s.textContent = ${JSON.stringify(text)}; })()`,
      );

    await style(
      "td-shoot-layout",
      `.grid{grid-template-columns:repeat(auto-fill,${size}px)!important}` +
        `.cell .scene{height:${size}px!important}` +
        `*,*::before,*::after{transition:none!important}` +
        css,
    );
    // One tick for layout to settle after the reflow.
    await new Promise((r) => setTimeout(r, 250));

    const cells = await evaluate(send, `(${pick.toString()})()`);
    if (!cells?.length) throw new Error("the picker matched no cells");

    const out = new Map(cells.map((c) => [c.name, []]));
    for (let i = 0; i < frames; i++) {
      await style("td-shoot-frame", frameCss(i, frames));
      // A style change needs a paint before the pixels are the new ones.
      await send("Runtime.evaluate", {
        expression: "new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))",
        awaitPromise: true,
      });
      for (const { name, rect } of cells) {
        const shot = await send("Page.captureScreenshot", {
          format: "png",
          captureBeyondViewport: true,
          clip: { x: rect.x, y: rect.y, width: rect.width, height: rect.height, scale },
        });
        out.get(name).push(Buffer.from(shot.data, "base64"));
      }
    }
    return out;
  } finally {
    // Teardown must never lose a successful capture. Chromium keeps writing to
    // its profile while it exits, so removing the directory the instant after
    // `kill()` raced it and threw ENOTEMPTY — out of a `finally`, which
    // discarded three hundred good screenshots. Wait for the exit, then treat
    // any leftover as litter in /tmp rather than as a failure.
    cdp?.close();
    if (chrome) {
      const gone = new Promise((r) => chrome.proc.once("exit", r));
      chrome.proc.kill();
      await Promise.race([gone, new Promise((r) => setTimeout(r, 3000))]);
      try {
        rmSync(chrome.dir, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
      } catch {
        // a stray profile dir is not worth failing a build over
      }
    }
    srv.close();
  }
}
