// The write rule under test, as a reference for TD's Rust side. It rewrites ONLY:
//   1. the inner bytes of the FIRST <script ... id="report-notes"> element
//      (the one document.getElementById and querySelector both return);
//   2. the inner bytes of the FIRST <script ... id="report-concurs"> element, or — only when
//      there are concurs to write — inserts one immediately after the notes island's
//      </script>, exactly where notes.js's own save puts it (insertBefore island.nextSibling);
//   3. the LAST "<!--\nREADER NOTES —" comment, or inserts one before the last </body>.
// Every other byte is carried over untouched. Work happens on Buffers; markers are found
// in a latin1 view so string offsets equal byte offsets.
//
// Two escapes that notes.js itself does NOT apply (q7 shows its own save breaking on them):
//   - inside an island, "</" becomes "<\/" and "<!" becomes the JSON unicode escape for "<" followed by "!"
//     (see islandJson below) — still valid JSON,
//     JSON.parse returns the same strings, and the HTML tokenizer can no longer leave the
//     script element early or enter its double-escaped state;
//   - inside the comment, "-->" becomes "-- >" and "--!>" becomes "--! >", the only two
//     sequences that end an HTML comment. Parker's own "---" survives unchanged.

const L = (s) => Buffer.from(s, 'utf8').toString('latin1');
const MIRROR_MARK = L('<!--\nREADER NOTES —');
const openRe = (id) => new RegExp(`<script\\b[^>]*\\bid\\s*=\\s*["']?${id}["']?[^>]*>`, 'i');

export function findIsland(buf, id = 'report-notes') {
  const s = buf.toString('latin1');
  const re = openRe(id);
  const m = re.exec(s);
  if (!m) return null;
  const start = m.index + m[0].length;
  const end = s.toLowerCase().indexOf('</script', start);
  if (end < 0) return null;
  const closeEnd = s.indexOf('>', end) + 1;
  return { openTag: m[0], tagStart: m.index, start, end, closeEnd, count: (s.match(new RegExp(re.source, 'gi')) || []).length };
}

export function readIsland(buf, id = 'report-notes') {
  const isl = findIsland(buf, id);
  if (!isl) return null;
  const text = buf.subarray(isl.start, isl.end).toString('utf8');
  return JSON.parse(text.trim() || '{}');
}

export function islandJson(obj) {
  // notes.js writes JSON.stringify(N, null, 1) and JSON.stringify(C, null, 1); keep that shape.
  return JSON.stringify(obj, null, 1).replace(/<\//g, '<\\/').replace(/<!/g, '\\u003c!');
}

// Mirrors notes.js buildMap() as of skill commit 250188f: document order, one anchor line
// ("  ✓ concur" appended when concurred), one line per note, concur count in the header.
export function buildMap(fileLabel, notes, anchorsInDocOrder, concurs = {}) {
  let count = 0, els = 0;
  for (const k of Object.keys(notes)) { els++; count += notes[k].length; }
  const cc = Object.keys(concurs).length;
  const out = [
    'NOTES — ' + fileLabel,
    count + ' notes on ' + els + ' elements' + (cc ? ' · ' + cc + (cc === 1 ? ' concur' : ' concurs') : '') + '.',
    'Each [anchor] is an element id in that file — search it to find the passage.',
    '',
  ];
  for (const a of anchorsInDocOrder) {
    const list = notes[a.nid] || [];
    const agreed = !!concurs[a.nid];
    if (!list.length && !agreed) continue;
    out.push('[' + a.nid + '] ' + a.title + (agreed ? '  ✓ concur' : ''));
    for (const n of list) out.push('  · ' + n.text.replace(/\n+/g, ' '));
    out.push('');
  }
  return out.join('\n');
}

export function commentSafe(s) {
  return s.replace(/--!>/g, '--! >').replace(/-->/g, '-- >');
}

// concurs === null means "TD is not touching concurs" (the brief's notes.js predates them).
export function writeNotes(buf, notes, anchorsInDocOrder, fileLabel, concurs = null) {
  const isl = findIsland(buf, 'report-notes');
  if (!isl) throw new Error('no report-notes island: refusing to write');
  const edits = [{ what: 'notes-island', start: isl.start, end: isl.end, bytes: Buffer.from(islandJson(notes), 'utf8') }];

  if (concurs) {
    const cis = findIsland(buf, 'report-concurs');
    if (cis) edits.push({ what: 'concurs-island', start: cis.start, end: cis.end, bytes: Buffer.from(islandJson(concurs), 'utf8') });
    else if (Object.keys(concurs).length) {
      edits.push({ what: 'concurs-island-inserted', start: isl.closeEnd, end: isl.closeEnd,
        bytes: Buffer.from('<script type="application/json" id="report-concurs">' + islandJson(concurs) + '</script>', 'utf8') });
    }
  }

  const mirror = Buffer.from('<!--\nREADER NOTES —\n' + commentSafe(buildMap(fileLabel, notes, anchorsInDocOrder, concurs || {})) + '\n-->', 'utf8');
  const s = buf.toString('latin1');
  const at = s.lastIndexOf(MIRROR_MARK);
  if (at > isl.closeEnd) {
    const close = s.indexOf('-->', at + 4);
    if (close < 0) throw new Error('unterminated READER NOTES comment');
    edits.push({ what: 'mirror-replaced', start: at, end: close + 3, bytes: mirror });
  } else {
    const bodyClose = s.toLowerCase().lastIndexOf('</body>');
    const pos = bodyClose >= 0 ? bodyClose : s.length;
    edits.push({ what: bodyClose >= 0 ? 'mirror-inserted-before-body-close' : 'mirror-appended-at-eof', start: pos, end: pos, bytes: mirror });
  }

  edits.sort((a, b) => a.start - b.start);
  for (let i = 1; i < edits.length; i++) if (edits[i].start < edits[i - 1].end) throw new Error('overlapping edits');
  const parts = [];
  let o = 0;
  for (const e of edits) { parts.push(buf.subarray(o, e.start), e.bytes); o = e.end; }
  parts.push(buf.subarray(o));
  return { out: Buffer.concat(parts), edits, islandCount: isl.count };
}

// Proof that nothing outside the declared edits moved: walk both buffers edit by edit
// and compare every byte in between.
export function untouchedOutsideEdits(orig, out, edits) {
  let o = 0, n = 0;
  for (const e of edits) {
    const len = e.start - o;
    if (!orig.subarray(o, e.start).equals(out.subarray(n, n + len))) return false;
    n += len + e.bytes.length; o = e.end;
  }
  return orig.subarray(o).equals(out.subarray(n));
}
