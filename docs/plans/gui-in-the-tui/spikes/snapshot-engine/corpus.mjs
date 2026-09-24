// Q2/Q6 background: survey the byte shapes of every brief in the corpus, so the write
// rule is judged against what actually exists rather than against the template.
// Regex over bytes on purpose: TD's writer will see bytes, not a DOM.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { basename } from 'node:path';
import { corpus, SCRATCH } from './lib.mjs';

const ISLAND_RE = /<script\b[^>]*\bid=["']?report-notes["']?[^>]*>([\s\S]*?)<\/script>/gi;
const CONCUR_RE = /<script\b[^>]*\bid=["']?report-concurs["']?[^>]*>/gi;
const SCRIPT_RE = /<script\b([^>]*)>([\s\S]*?)<\/script>/gi;

const rows = [];
for (const { path, bytes } of corpus()) {
  const buf = readFileSync(path);
  const s = buf.toString('utf8');
  const lines = s.split('\n');
  const row = {
    file: path.replace('/home/parker/Work/', ''),
    bytes,
    lines: lines.length,
    maxLine: Math.max(...lines.map((l) => l.length)),
    crlf: s.includes('\r\n'),
    bom: buf[0] === 0xef && buf[1] === 0xbb && buf[2] === 0xbf,
    utf8RoundTrips: Buffer.from(s, 'utf8').equals(buf),
  };

  const islands = [...s.matchAll(ISLAND_RE)];
  row.islands = islands.length;
  if (islands.length) {
    const m = islands[0];
    row.islandOpenTag = m[0].slice(0, m[0].indexOf('>') + 1);
    row.islandOffset = m.index;
    row.islandBodyBytes = Buffer.byteLength(m[1]);
    try {
      const j = JSON.parse(m[1].trim() || '{}');
      row.islandKeys = Object.keys(j).length;
      row.islandNotes = Object.values(j).reduce((a, v) => a + (Array.isArray(v) ? v.length : 0), 0);
      row.islandParse = 'ok';
    } catch (e) { row.islandParse = 'FAIL ' + e.message; }
    row.islandHasScriptCloseRisk = /<\/script/i.test(m[1]);
    row.islandHasHtmlComment = /<!--/.test(m[1]);
  }
  row.concurIsland = [...s.matchAll(CONCUR_RE)].length;
  row.readerNotesComments = (s.match(/<!--\s*\nREADER NOTES —/g) || []).length;

  // notes.js fingerprint: the inline script that defines the notes IIFE.
  row.notesJs = null;
  for (const m of s.matchAll(SCRIPT_RE)) {
    const body = m[2];
    if (/reader notes/i.test(body) && /function tag\(\)/.test(body)) {
      row.notesJs = createHash('sha1').update(body.trim()).digest('hex').slice(0, 10);
      row.notesJsBytes = Buffer.byteLength(body);
      row.notesJsHasConcur = /concurZones/.test(body);
      row.notesJsHasFigureTarget = /\['figure', 'fig'\]/.test(body);
      row.notesJsOffset = m.index;
      break;
    }
  }
  row.islandBeforeNotesJs = row.islands && row.notesJs ? row.islandOffset < row.notesJsOffset : null;
  row.targetsOverride = /window\.NOTES_TARGETS\s*=/.test(s);
  const nf = s.match(/window\.NOTES_FILE\s*=\s*['"]([^'"]+)['"]/);
  row.notesFile = nf ? nf[1] : null;
  row.notesFileMatches = nf ? nf[1] === basename(path) : null;
  row.dialogs = (s.match(/<dialog\b/gi) || []).length;
  row.dataDlg = (s.match(/\bdata-dlg=/gi) || []).length;
  row.bakedNids = (s.match(/\bdata-nid=/g) || []).length;       // saved-from-DOM shape
  row.externalScript = /<script\b[^>]*\bsrc=/i.test(s);
  row.externalHttp = (s.match(/(?:src|href)=["']https?:\/\//gi) || []).length;
  rows.push(row);
}

writeFileSync(`${SCRATCH}/corpus.json`, JSON.stringify(rows, null, 1));

const n = rows.length;
const withIsland = rows.filter((r) => r.islands);
const tally = (f) => rows.reduce((m, r) => { const k = String(f(r)); m[k] = (m[k] || 0) + 1; return m; }, {});
const sizes = rows.map((r) => r.bytes).sort((a, b) => a - b);
console.log(JSON.stringify({
  files: n,
  withIsland: withIsland.length,
  islandCountDist: tally((r) => r.islands),
  islandParse: tally((r) => r.islandParse || 'no-island'),
  islandWithNotes: withIsland.filter((r) => r.islandKeys > 0).length,
  islandOpenTags: tally((r) => r.islandOpenTag || 'none'),
  islandBeforeNotesJs: tally((r) => r.islandBeforeNotesJs),
  readerNotesComments: tally((r) => r.readerNotesComments),
  concurIsland: tally((r) => r.concurIsland),
  notesJsVersions: tally((r) => r.notesJs ? `${r.notesJs} concur=${r.notesJsHasConcur} fig=${r.notesJsHasFigureTarget}` : 'none'),
  notesJsWithoutIsland: rows.filter((r) => r.notesJs && !r.islands).length,
  islandWithoutNotesJs: rows.filter((r) => !r.notesJs && r.islands).length,
  targetsOverride: tally((r) => r.targetsOverride),
  notesFileMatches: tally((r) => r.notesFileMatches),
  bakedNidFiles: rows.filter((r) => r.bakedNids > 0).length,
  crlf: tally((r) => r.crlf), bom: tally((r) => r.bom), utf8RoundTrips: tally((r) => r.utf8RoundTrips),
  minifiedLike: rows.filter((r) => r.maxLine > 5000).map((r) => `${r.file} maxLine=${r.maxLine}`),
  dialogsDist: tally((r) => r.dialogs),
  dataDlgFiles: rows.filter((r) => r.dataDlg > 0).length,
  externalScript: tally((r) => r.externalScript),
  externalHttpFiles: rows.filter((r) => r.externalHttp > 0).length,
  bytes: { min: sizes[0], median: sizes[n >> 1], max: sizes[n - 1] },
  largest: rows.slice().sort((a, b) => b.bytes - a.bytes).slice(0, 5).map((r) => `${r.file} ${r.bytes}`),
  noIslandFiles: rows.filter((r) => !r.islands).map((r) => `${r.file} notesJs=${!!r.notesJs} dialogs=${r.dialogs}`),
  islandRisks: rows.filter((r) => r.islandHasScriptCloseRisk || r.islandHasHtmlComment).map((r) => r.file),
}, null, 1));
