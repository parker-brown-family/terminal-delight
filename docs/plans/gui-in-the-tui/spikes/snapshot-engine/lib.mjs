// Shared helpers for the snapshot-engine spike. No Terminal Delight code; this only
// drives the system Chromium through Playwright's library to measure things.
import { createRequire } from 'node:module';
import { readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';

const require = createRequire('/home/parker/BROWN-FAMILY-SPORTS/Software/wellness-with-kate-site/package.json');
export const { chromium } = require('playwright-core');

export const SCRATCH = '/tmp/claude-1000/td-snapshot-spike';
export const SAVED_EXAMPLE = '/home/parker/Downloads/2026-09-24-gui-in-the-tui.html';
export const CORPUS_DIRS = ['/home/parker/Work/reports', '/home/parker/Work/terminal-delight/reports'];

// The pane-like viewport the task fixes: 968 CSS px wide at Parker's monitor scale.
export const PANE = { width: 968, height: 1400, deviceScaleFactor: 1.6 };

export function corpus() {
  const out = [];
  for (const d of CORPUS_DIRS) {
    for (const f of readdirSync(d)) {
      if (!f.endsWith('.html')) continue;
      const p = join(d, f);
      out.push({ path: p, bytes: statSync(p).size });
    }
  }
  return out;
}

export async function launch(extraArgs = []) {
  return chromium.launch({ executablePath: '/usr/bin/chromium', args: extraArgs });
}

export function ms(t0) { return +(performance.now() - t0).toFixed(1); }

// ---- process accounting for the Chromium tree (browser + zygotes + gpu + renderers) ----
// Launch with a marker switch; Chromium ignores unknown switches, and the browser
// process keeps it on its command line, so its descendants can be found through /proc.
import { readFileSync as _rf, readdirSync as _rd } from 'node:fs';
export function chromeTree(marker) {
  const procs = [];
  for (const d of _rd('/proc')) {
    if (!/^\d+$/.test(d)) continue;
    try {
      const stat = _rf(`/proc/${d}/stat`, 'utf8');
      const ppid = +stat.slice(stat.lastIndexOf(')') + 2).split(' ')[1];
      const cmd = _rf(`/proc/${d}/cmdline`, 'utf8');
      procs.push({ pid: +d, ppid, cmd });
    } catch { /* process went away */ }
  }
  const root = procs.find((p) => p.cmd.includes(marker) && !p.cmd.includes('--type='));
  if (!root) return [];
  const out = new Set([root.pid]);
  let grew = true;
  while (grew) { grew = false; for (const p of procs) if (!out.has(p.pid) && out.has(p.ppid)) { out.add(p.pid); grew = true; } }
  return [...out];
}
export function cpuTicks(pids) {
  let t = 0;
  for (const pid of pids) {
    try { const s = _rf(`/proc/${pid}/stat`, 'utf8'); const f = s.slice(s.lastIndexOf(')') + 2).split(' '); t += +f[11] + +f[12]; } catch { /* gone */ }
  }
  return t; // clock ticks; USER_HZ is 100 on this kernel config (getconf CLK_TCK)
}
export function pssKiB(pids) {
  let k = 0;
  for (const pid of pids) {
    try { const m = _rf(`/proc/${pid}/smaps_rollup`, 'utf8').match(/^Pss:\s+(\d+)/m); if (m) k += +m[1]; } catch { /* gone */ }
  }
  return k;
}
export function pngSize(buf) { return { w: buf.readUInt32BE(16), h: buf.readUInt32BE(20) }; }

export function fileUrl(p) { return 'file://' + p; }

// Every notable element, as TD would want it: id, title, rect in PAGE coordinates
// (CSS px from the document origin, independent of scroll), plus which dialog owns it.
export const EXTRACT_ANCHORS = () => {
  const sx = window.scrollX, sy = window.scrollY;
  return Array.from(document.querySelectorAll('.notable')).map((el) => {
    const r = el.getBoundingClientRect();
    const dlg = el.closest('dialog');
    return {
      nid: el.dataset.nid,
      title: el.dataset.ntitle,
      tag: el.tagName.toLowerCase(),
      dialog: dlg ? (dlg.id || '(anonymous dialog)') : null,
      rect: { x: r.x + sx, y: r.y + sy, w: r.width, h: r.height },
      rendered: r.width > 0 && r.height > 0,
      // Only briefs carrying the 2026-09-24 notes.js give decisions a concur zone; TD
      // must offer a concur only where the brief's own script would show it.
      concurrable: el.classList.contains('concurrable'),
      concurred: el.classList.contains('has-concur'),
    };
  });
};
