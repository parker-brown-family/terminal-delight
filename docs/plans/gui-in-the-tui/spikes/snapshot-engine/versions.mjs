// Which notes.js does each brief carry? Hash every inline copy the same way corpus.mjs
// does, hash each release of the skill's notes.js from its git history, and report which
// corpus copies are a release verbatim and which are forks (hand-edited in one brief).
// Export the releases first:  git -C ~/.claude/skills/decision-brief show <rev>:./assets/notes.js
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { SCRATCH } from './lib.mjs';

const RELEASES = { '250188f': '2026-09-24 concur', b689671: '2026-09-24 figures', eca5cb8: '2026-09-03', '5717474': '2026-08-29 first' };
const sha = (s) => createHash('sha1').update(s.trim()).digest('hex').slice(0, 10);
const lines = (s) => new Set(s.trim().split('\n').map((l) => l.trim()).filter(Boolean));

const rel = {};
for (const [rev, label] of Object.entries(RELEASES)) {
  const body = readFileSync(`${SCRATCH}/notes-${rev}.js`, 'utf8');
  rel[rev] = { label, hash: sha(body), lines: lines(body) };
}

const SCRIPT_RE = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;
const corpus = JSON.parse(readFileSync(`${SCRATCH}/corpus.json`, 'utf8'));
const byHash = {};
for (const r of corpus) {
  if (!r.notesJs) continue;
  const s = readFileSync('/home/parker/Work/' + r.file, 'utf8');
  let body = null;
  for (const m of s.matchAll(SCRIPT_RE)) if (/reader notes/i.test(m[2]) && /function tag\(\)/.test(m[2])) { body = m[2]; break; }
  const h = sha(body);
  if (!byHash[h]) {
    const L = lines(body);
    let best = null;
    for (const [rev, x] of Object.entries(rel)) {
      let diff = 0;
      for (const l of L) if (!x.lines.has(l)) diff++;
      for (const l of x.lines) if (!L.has(l)) diff++;
      if (!best || diff < best.diff) best = { rev, label: x.label, diff };
    }
    const exact = Object.entries(rel).find(([, x]) => x.hash === h);
    byHash[h] = { exactRelease: exact ? `${exact[0]} (${exact[1].label})` : null, nearest: best, concur: /concurZones/.test(body), files: [] };
  }
  byHash[h].files.push(r.file);
}
const summary = Object.entries(byHash).map(([h, v]) => ({ hash: h, briefs: v.files.length, concurCapable: v.concur, exactRelease: v.exactRelease, nearest: `${v.nearest.rev} ${v.nearest.label}, ${v.nearest.diff} differing lines`, example: v.files[0] }));
writeFileSync(`${SCRATCH}/versions.json`, JSON.stringify(byHash, null, 1));
console.log(JSON.stringify({
  releaseHashes: Object.fromEntries(Object.entries(rel).map(([k, v]) => [k, v.hash])),
  variants: summary.sort((a, b) => b.briefs - a.briefs),
  concurCapableBriefs: Object.values(byHash).filter((v) => v.concur).flatMap((v) => v.files),
  olderBriefs: Object.values(byHash).filter((v) => !v.concur).reduce((a, v) => a + v.files.length, 0),
}, null, 1));
