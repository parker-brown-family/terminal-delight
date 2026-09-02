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
 * Load `path` from `webDir` and photograph one element per named cell.
 *
 * `pick` is evaluated IN THE PAGE and must return `[{name, sel}]` — the caller
 * decides what a cell is, because only the caller knows the page. Every element
 * is captured at `size`² with `scale`× pixel density.
 *
 * Returns `Map<name, Buffer>` of PNGs.
 */
export async function shoot({ webDir, path = "/", pick, css = "", size = 320, scale = 2, chromium = "chromium" }) {
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

    // Square the cells and stop every animation, so a photograph is the pose
    // and not an arbitrary frame of a blink. Injected rather than asked of the
    // playhouse: its layout is right for a human reviewing a sheet, and this is
    // a camera rig, not an opinion about their page.
    await evaluate(
      send,
      `(() => { const s = document.createElement('style');
        s.textContent = ${JSON.stringify(
          `.grid{grid-template-columns:repeat(auto-fill,${size}px)!important}` +
            `.cell .scene{height:${size}px!important}` +
            `*,*::before,*::after{animation:none!important;transition:none!important}` +
            css,
        )};
        document.head.appendChild(s); })()`,
    );
    // One frame for layout to settle after the reflow.
    await new Promise((r) => setTimeout(r, 250));

    const cells = await evaluate(send, `(${pick.toString()})()`);
    if (!cells?.length) throw new Error("the picker matched no cells");

    const out = new Map();
    for (const { name, rect } of cells) {
      const shot = await send("Page.captureScreenshot", {
        format: "png",
        captureBeyondViewport: true,
        clip: { x: rect.x, y: rect.y, width: rect.width, height: rect.height, scale },
      });
      out.set(name, Buffer.from(shot.data, "base64"));
    }
    return out;
  } finally {
    cdp?.close();
    chrome?.proc.kill();
    if (chrome) rmSync(chrome.dir, { recursive: true, force: true });
    srv.close();
  }
}
