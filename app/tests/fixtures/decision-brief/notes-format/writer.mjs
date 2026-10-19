// The reference writer for a decision brief's notes. Terminal Delight's Rust writer
// (docview/notes.rs) follows it rule for rule, and the fixtures hold both to the same bytes.
// Promoted from the TD snapshot spike's notes-writer.mjs, then given the format version,
// the revision, and the refusals TD's plan names.
//
// It works on bytes. Markers are found in a latin1 view, so string offsets equal byte
// offsets and the file is never decoded as a whole. A write rewrites at most three
// regions and carries every other byte over untouched:
//   1. the FIRST <script … id="report-notes"> element: its inner text, and on a format-1
//      brief its open tag, which takes the write's revision;
//   2. the FIRST <script … id="report-concurs"> element the same way, or, only when there
//      are concurs to write and no island exists, a new one straight after the notes
//      island's </script>, which is where notes.js's own save puts it;
//   3. the LAST "<!--\nREADER NOTES —" comment after the notes island, or a new one
//      before the last </body>, or at the end of the file.
//
// Escapes, the same in notes.js's own save (format 1 onwards):
//   - inside an island, "</" becomes "<\/" and "<!" becomes "\u003c!". Still valid JSON,
//     JSON.parse returns the same strings, and the HTML tokenizer can no longer leave the
//     script element early or enter its double-escaped state;
//   - inside the comment, "--!>" becomes "--! >" and then "-->" becomes "-- >", the only
//     two sequences that end an HTML comment. A reader's own "---" is left alone.

export const FORMAT = '1';   // the newest format this writer knows; it refuses anything else

export class Refusal extends Error {
  constructor(kind, detail = '') { super(detail ? `${kind}: ${detail}` : kind); this.kind = kind; }
}

const L = (s) => Buffer.from(s, 'utf8').toString('latin1');
const MIRROR_MARK = L('<!--\nREADER NOTES —');
const openRe = (id, flags = 'i') => new RegExp(`<script\\b[^>]*\\bid\\s*=\\s*["']?${id}["']?[^>]*>`, flags);

// The first element whose open tag carries the id, as document.getElementById finds it
// (checked against the browser on all 94 islands in Parker's corpus).
export function findIsland(buf, id) {
  const s = buf.toString('latin1');
  const m = openRe(id).exec(s);
  if (!m) return null;
  const start = m.index + m[0].length;
  const end = s.toLowerCase().indexOf('</script', start);
  if (end < 0) return null;
  const closeEnd = s.indexOf('>', end) + 1;
  return { openTag: m[0], tagStart: m.index, start, end, closeEnd,
    count: (s.match(openRe(id, 'gi')) || []).length };
}

// The inline script that is notes.js: the one holding both "function tag()" and
// "reader notes". Null when the page carries none.
export function findNotesJs(buf) {
  const s = buf.toString('latin1');
  for (let at = s.indexOf('function tag()'); at >= 0; at = s.indexOf('function tag()', at + 1)) {
    const open = s.toLowerCase().lastIndexOf('<script', at);
    const close = s.toLowerCase().indexOf('</script', at);
    if (open < 0 || close < 0) continue;
    if (s.slice(open, close).includes('reader notes')) return { start: open, end: close };
  }
  return null;
}

export function attr(openTag, name) {
  const m = openTag.match(new RegExp(`\\s${name}\\s*=\\s*(?:"([^"]*)"|'([^']*)'|([^\\s"'>]+))`, 'i'));
  return m ? (m[1] ?? m[2] ?? m[3]) : null;
}

// setAttribute as the browser serialises it: an existing value is replaced in place and
// double-quoted; a new attribute goes last.
export function setAttr(openTag, name, value) {
  const re = new RegExp(`(\\s${name}\\s*=\\s*)(?:"[^"]*"|'[^']*'|[^\\s"'>]+)`, 'i');
  if (re.test(openTag)) return openTag.replace(re, `$1"${value}"`);
  return openTag.replace(/\s*>$/, ` ${name}="${value}">`);
}

export function islandJson(obj) {
  return JSON.stringify(obj, null, 1).replace(/<\//g, '<\\/').replace(/<!/g, '\\u003c!');
}

export function commentSafe(s) {
  return s.replace(/--!>/g, '--! >').replace(/-->/g, '-- >');
}

export function mirrorComment(map) {
  return '<!--\nREADER NOTES —\n' + commentSafe(map) + '\n-->';
}

// notes.js buildMap(), unchanged since the concur release: document order, one anchor
// line ("  ✓ concur" when concurred), one line per note with each run of \n made a space,
// and a header that counts every key, including keys whose anchor is gone.
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

// Island text is read the way notes.js reads it: JSON.parse(textContent || '{}'), with
// surrounding whitespace ignored. Anything that is not a map of the right shape is
// Unreadable, because a writer that guessed would destroy what it could not read.
function parseIsland(buf, isl, kind) {
  const text = buf.subarray(isl.start, isl.end).toString('utf8');
  let v;
  try { v = JSON.parse(text.trim() || '{}'); } catch (e) { throw new Refusal('Unreadable', `${kind}: ${e.message}`); }
  const ok = v && typeof v === 'object' && !Array.isArray(v) && Object.values(v).every((x) => kind === 'notes'
    ? Array.isArray(x) && x.every((n) => n && typeof n === 'object' && typeof n.text === 'string')
    : typeof x === 'string');
  if (!ok) throw new Refusal('Unreadable', `${kind}: not a map of the expected shape`);
  return v;
}

// What a reader of the file sees, without writing: the maps, the declared format, the
// revision. notes is null when there is no island at all: read-only, not empty.
export function readIslands(buf) {
  const isl = findIsland(buf, 'report-notes');
  const cis = findIsland(buf, 'report-concurs');
  const out = { notes: null, concurs: {}, format: null, rev: null, notesText: null };
  if (cis) out.concurs = parseIsland(buf, cis, 'concurs');
  if (!isl) return out;
  out.notesText = buf.subarray(isl.start, isl.end).toString('utf8');
  out.format = attr(isl.openTag, 'data-format');
  out.rev = attr(isl.openTag, 'data-rev');
  out.notes = parseIsland(buf, isl, 'notes');
  return out;
}

function applyEdits(notes, concurs, edits, anchors, concurSupport) {
  const known = new Set(anchors.map((a) => a.nid));
  for (const e of edits) {
    if (e.op === 'add') {
      if (!known.has(e.nid)) throw new Refusal('AnchorGone', e.nid);
      (notes[e.nid] = notes[e.nid] || []).push({ text: e.text, title: e.title, ts: e.ts });
    } else if (e.op === 'delete') {
      // Found by its words, not its position: another writer may have added notes ahead of it.
      const list = notes[e.nid] || [];
      const i = list.findIndex((n) => n.text === e.text && (!e.ts || n.ts === e.ts));
      if (i >= 0) list.splice(i, 1);
      if (notes[e.nid] && !notes[e.nid].length) delete notes[e.nid];
    } else if (e.op === 'concur' || e.op === 'unconcur') {
      if (concurSupport !== 'supported') throw new Refusal('ConcursUnsupported', e.nid);
      if (e.op === 'concur') {
        if (!known.has(e.nid)) throw new Refusal('AnchorGone', e.nid);
        concurs[e.nid] = e.ts;
      } else delete concurs[e.nid];
    } else throw new Error('unknown edit op ' + e.op);
  }
}

// opts: anchors [{nid, title}] in document order, from the brief's own notes.js;
//       label, the page's NOTES_FILE (not the path's basename);
//       concurSupport 'supported' | 'unsupported', from the brief's own notes.js;
//       rev, the revision this write stamps: an ISO 8601 UTC time to the millisecond;
//       path, only to refuse build inputs.
export function planWrite(buf, edits, { anchors, label, concurSupport, rev, path = '' }) {
  if (/(^|\/)_[^/]*$/.test(path)) throw new Refusal('BuildInput', path);
  const isl = findIsland(buf, 'report-notes');
  if (!isl) throw new Refusal('NoIsland');
  const js = findNotesJs(buf);
  if (js && isl.tagStart > js.start) throw new Refusal('IslandAfterScript');
  const format = attr(isl.openTag, 'data-format');
  if (format !== null && format !== FORMAT) throw new Refusal('UnknownFormat', format);
  const stamp = format === FORMAT;
  if (stamp && !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d\.\d{3}Z$/.test(rev || '')) throw new Error('a format-1 write needs rev as an ISO time');

  const notes = parseIsland(buf, isl, 'notes');
  const cis = findIsland(buf, 'report-concurs');
  const touchConcurs = concurSupport === 'supported';
  const concurs = cis && touchConcurs ? parseIsland(buf, cis, 'concurs') : {};
  applyEdits(notes, concurs, edits, anchors, concurSupport);

  // The open tag stays in the latin1 view it was found in, so its bytes go back unchanged.
  const retag = (tag) => setAttr(setAttr(tag, 'data-format', FORMAT), 'data-rev', rev);
  const island = (i, map, what) => stamp
    ? { what, start: i.tagStart, end: i.end,
        bytes: Buffer.concat([Buffer.from(retag(i.openTag), 'latin1'), Buffer.from(islandJson(map), 'utf8')]) }
    : { what, start: i.start, end: i.end, bytes: islandJson(map) };
  const splices = [island(isl, notes, 'notes-island')];
  if (touchConcurs) {
    if (cis) splices.push(island(cis, concurs, 'concurs-island'));
    else if (Object.keys(concurs).length) {
      const tag = '<script type="application/json" id="report-concurs">';
      splices.push({ what: 'concurs-island-inserted', start: isl.closeEnd, end: isl.closeEnd,
        bytes: (stamp ? retag(tag) : tag) + islandJson(concurs) + '</script>' });
    }
  }

  const mirror = mirrorComment(buildMap(label, notes, anchors, touchConcurs ? concurs : {}));
  const s = buf.toString('latin1');
  const at = s.lastIndexOf(MIRROR_MARK);
  if (at > isl.closeEnd) {
    const close = s.indexOf('-->', at + 4);
    if (close < 0) throw new Refusal('UnterminatedMirror');
    splices.push({ what: 'mirror-replaced', start: at, end: close + 3, bytes: mirror });
  } else {
    const bodyClose = s.toLowerCase().lastIndexOf('</body>');
    const pos = bodyClose >= 0 ? bodyClose : s.length;
    splices.push({ what: bodyClose >= 0 ? 'mirror-inserted' : 'mirror-appended', start: pos, end: pos, bytes: mirror });
  }

  for (const sp of splices) if (typeof sp.bytes === 'string') sp.bytes = Buffer.from(sp.bytes, 'utf8');
  splices.sort((a, b) => a.start - b.start);
  for (let i = 1; i < splices.length; i++) if (splices[i].start < splices[i - 1].end) throw new Error('overlapping splices');
  const parts = [];
  let o = 0;
  for (const sp of splices) { parts.push(buf.subarray(o, sp.start), sp.bytes); o = sp.end; }
  parts.push(buf.subarray(o));
  return { out: Buffer.concat(parts), splices, notes, concurs: touchConcurs ? concurs : null, format, rev: stamp ? rev : null };
}

// The two checks TD runs before it commits a write: the islands re-parse to the maps it
// meant to write, and every byte outside the splices equals the original.
export function verify(before, plan) {
  let o = 0, n = 0;
  for (const sp of plan.splices) {
    const len = sp.start - o;
    if (!before.subarray(o, sp.start).equals(plan.out.subarray(n, n + len))) return `bytes moved before ${sp.what}`;
    n += len + sp.bytes.length; o = sp.end;
  }
  if (!before.subarray(o).equals(plan.out.subarray(n))) return 'bytes moved after the last splice';
  const back = readIslands(plan.out);
  if (JSON.stringify(back.notes) !== JSON.stringify(plan.notes)) return 'the notes island does not re-parse to the notes written';
  if (plan.concurs && JSON.stringify(back.concurs) !== JSON.stringify(plan.concurs)) return 'the concurs island does not re-parse to the concurs written';
  return null;
}
