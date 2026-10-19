// The fixture cases: how each brief.html is made and which edits each one takes.
// build.mjs writes brief.html and edits.json from these; check.mjs --update derives the
// rest. Every brief is synthetic, built from this skill's own markup at some release.
import { release, buildBrief } from './brief.mjs';
import { planWrite, islandJson, mirrorComment, buildMap } from './writer.mjs';

export const A = {
  finding: ['finding-the-island-is-the', 'The island is the record'],
  fig: ['fig-01-two-writers-one', '01 · Two writers, one file'],
  card: ['verdict-reference-writer', 'Reference writer'],
  callout: ['callout-a-callout-that-takes', 'A callout that takes a note.'],
  ask1: ['ask-stamp-a-revision-on', 'Stamp a revision on the island?'],
  ask2: ['ask-keep-the-plain-text', 'Keep the plain-text mirror?'],
};
export const add = ([nid, title], text, ts) => ({ op: 'add', nid, title, text, ts });
export const del = ([nid], text, ts) => ({ op: 'delete', nid, text, ...(ts ? { ts } : {}) });
export const concur = ([nid], ts) => ({ op: 'concur', nid, ts });
export const unconcur = ([nid], ts) => ({ op: 'unconcur', nid, ts });

// Text a person can type into the note box. The box trims, and a textarea gives LF only.
export const HOSTILE = [
  'ends a script </script><b>LEAKED-1</b> and </SCRIPT > too',
  'opens a comment <!-- then <!--<script> double-escapes, and <![CDATA[ x ]]>',
  'closes a comment --> LEAKED-2 and --!> LEAKED-3, keeps ---',
  'ünïcødé — “quoted” ✓ 🎯 日本語',
  'line one\nline two\n\nline four',
];
// Text only another writer can put in a file: a CR, a control character, U+2028.
export const FOREIGN = [
  'crlf line\r\nnext, a bell \u0007 and a line separator \u2028 here',
  '</script> baked by another writer, with <!-- and --> inside',
];

const stamped = (rev) => ` data-format="1" data-rev="${rev}"`;
const anchorsCurrent = Object.values(A).map(([nid, title]) => ({ nid, title }));

// A brief as some writer left it: the islands and a mirror that agrees with them.
function baked(rel, notes, concurs, rev, extra = {}) {
  const label = extra.notesFile ?? 'brief.html';
  const mirror = mirrorComment(buildMap(label, notes, anchorsCurrent, concurs)) + '\n';
  return buildBrief(rel, {
    notes: islandJson(notes), concurs: islandJson(concurs),
    notesAttrs: stamped(rev), concursAttrs: stamped(rev), ...extra,
    tail: (extra.tail ?? '') + mirror,
  });
}

export const CASES = [
  {
    name: 'current-pristine',
    release: 'current',
    pins: 'The ordinary path on this release: empty islands that declare format 1, one note on a finding and one on a figure, two concurs. The first save stamps a revision on both islands and inserts the mirror before </body>.',
    brief: () => buildBrief(release('current')),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.finding, 'The revision goes on both islands.', '2026-09-24 20:00'),
      add(A.fig, 'Draw the browser and TD side by side.', '2026-09-24 20:01'),
      concur(A.ask1, '2026-09-24 20:02'), concur(A.ask2, '2026-09-24 20:03')],
  },
  {
    name: 'concur-era-pristine',
    release: '250188f',
    pins: 'A brief from 250188f, the concur release: both islands {}, no format declared. Written as that release writes it, with no data-format and no data-rev, but escaped and with one mirror.',
    brief: () => buildBrief(release('250188f')),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.finding, 'One note beside two concurs.', '2026-09-24 20:00'),
      concur(A.ask1, '2026-09-24 20:01'), concur(A.ask2, '2026-09-24 20:02')],
  },
  ...['b689671', 'eca5cb8', '5717474'].map((rev) => ({
    name: rev === 'b689671' ? 'b689671-saved' : rev,
    pins: `A brief from ${rev}, with no concurs, that has been through its own "save into file" once: the DOM re-serialised, a data-nid on every anchor, notes in the island and one mirror. The write keeps it in that release's format and never adds a concurs island.`,
    release: rev,
    prep: [add(A.finding, 'First note, written in the browser.', '2026-09-20 09:00'),
      add(A.finding, 'Second note on the same finding.', '2026-09-20 09:01'),
      add(A.card, 'A note on the card.', '2026-09-20 09:02')],
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.card, 'Added later, by the writer.', '2026-09-24 20:00'),
      del(A.finding, 'First note, written in the browser.', '2026-09-20 09:00')],
  })),
  {
    name: 'hostile-text',
    release: 'current',
    pins: 'Note text that would end the island (</script>, </SCRIPT >), open a comment or the double-escaped script state (<!--, <!--<script>, <![CDATA[), end the mirror (-->, --!>), plus ---, non-ASCII, emoji and newlines. The file already holds two notes another writer baked in with a CR, a bell and U+2028. Every one round-trips exactly.',
    brief: () => {
      const pristine = Buffer.from(buildBrief(release('current')));
      return planWrite(pristine, [add(A.callout, FOREIGN[0], '2026-09-24 19:00'), add(A.callout, FOREIGN[1], '2026-09-24 19:01')],
        { anchors: anchorsCurrent, label: 'brief.html', concurSupport: 'supported', rev: '2026-09-24T19:02:00.000Z' }).out.toString('utf8');
    },
    rev: '2026-09-24T20:05:00.000Z',
    edits: HOSTILE.map((t, i) => add(i < 3 ? A.finding : A.fig, t, `2026-09-24 20:0${i}`)),
  },
  {
    name: 'orphan-note',
    release: 'current',
    pins: 'The island holds a note whose anchor is gone (counted in the map header, never listed), keys out of document order (kept), and a field another writer added (kept). A note is added and one deleted.',
    brief: () => baked(release('current'), {
      [A.fig[0]]: [{ text: 'Figure note one.', title: A.fig[1], ts: '2026-09-23 10:00', author: 'terminal-delight' },
        { text: 'Figure note two.', title: A.fig[1], ts: '2026-09-23 10:01' }],
      'finding-a-passage-since-cut': [{ text: 'This passage was cut from the brief.', title: 'A passage since cut', ts: '2026-09-22 08:00' }],
      [A.finding[0]]: [{ text: 'An older finding note.', title: A.finding[1], ts: '2026-09-23 10:02' }],
    }, {}, '2026-09-23T10:03:00.000Z'),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.callout, 'New note beside an orphan.', '2026-09-24 20:00'), del(A.fig, 'Figure note two.')],
  },
  {
    name: 'two-islands',
    release: 'current',
    pins: 'The markup pasted twice: two notes islands and two concurs islands. Only the first of each is read and written, which is the one document.getElementById returns; the second pair stays byte-identical.',
    brief: () => baked(release('current'), { [A.card[0]]: [{ text: 'In the first island.', title: A.card[1], ts: '2026-09-23 11:00' }] }, {}, '2026-09-23T11:01:00.000Z', {
      extraIslands: '<script type="application/json" id="report-notes" data-format="1">{"verdict-reference-writer":[{"text":"In the second island. Never read.","title":"Reference writer","ts":"2026-09-01 00:00"}]}</script>\n' +
        '<script type="application/json" id="report-concurs" data-format="1">{"ask-keep-the-plain-text":"2026-09-01 00:00"}</script>\n',
    }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.callout, 'Written into the first island.', '2026-09-24 20:00'), concur(A.ask1, '2026-09-24 20:01')],
  },
  {
    name: 'stale-mirrors',
    release: 'current',
    pins: 'Three READER NOTES comments, left by saves before format 1 appended instead of replacing. Only the last is rewritten; the two before it stay byte-identical.',
    brief: () => {
      const stale = (n) => mirrorComment(buildMap('brief.html', { [A.finding[0]]: [{ text: `Stale map ${n}.`, title: A.finding[1], ts: '2026-09-20 09:00' }] }, anchorsCurrent, {})) + '\n';
      return baked(release('current'), { [A.finding[0]]: [{ text: 'The note the island holds.', title: A.finding[1], ts: '2026-09-21 09:00' }] }, {},
        '2026-09-21T09:01:00.000Z', { tail: stale(1) + stale(2) });
    },
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.finding, 'Added after the stale mirrors.', '2026-09-24 20:00')],
  },
  {
    name: 'notes-file-override',
    release: 'current',
    pins: 'NOTES_FILE names another file (a copy pointing at its original). The map header and the download use that name, never the path.',
    brief: () => buildBrief(release('current'), { notesFile: '2026-09-09-the-original-brief.html' }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.card, 'The header should name the original.', '2026-09-24 20:00'), concur(A.ask2, '2026-09-24 20:01')],
  },
  {
    name: 'no-body-close',
    release: 'current',
    pins: 'No </body> and no </html>, which HTML allows, and a notes island with no text at all, which reads as {}. The mirror goes at the end of the file.',
    brief: () => buildBrief(release('current'), { bodyClose: false, notes: '' }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.finding, 'The mirror lands at the end of the file.', '2026-09-24 20:00')],
  },
  {
    name: 'no-island',
    release: null,
    pins: 'A page with no notes system at all. It reads as read-only, not as an empty map, and a write is refused: NoIsland.',
    brief: () => '<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>No notes here</title>\n</head>\n<body>\n<main>\n<h1>A page without notes</h1>\n<div class="finding"><h3>Nothing to annotate</h3><p>No island, no notes.js.</p></div>\n</main>\n</body>\n</html>\n',
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(['finding-nothing-to-annotate', 'Nothing to annotate'], 'Refused: there is nowhere to put this.', '2026-09-24 20:00')],
  },
  {
    name: 'island-after-script',
    release: 'current',
    pins: 'The islands sit after notes.js, which reads them before they are parsed, so the note in the file never shows in a browser. A write is refused: IslandAfterScript.',
    brief: () => buildBrief(release('current'), { islandAfterScript: true,
      notes: islandJson({ [A.finding[0]]: [{ text: 'In the file, invisible in the page.', title: A.finding[1], ts: '2026-09-23 12:00' }] }) }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.card, 'Refused: the page could never show it.', '2026-09-24 20:00')],
  },
  {
    name: 'unreadable-island',
    release: 'current',
    pins: 'The notes island is cut off mid-string. The page shows no notes and throws nothing; a write is refused, Unreadable, rather than replacing what it could not read.',
    brief: () => buildBrief(release('current'), { notes: '{\n "finding-the-island-is-the": [\n  {\n   "text": "cut off' }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.card, 'Refused: the island cannot be read.', '2026-09-24 20:00')],
  },
  {
    name: 'future-format',
    release: 'current',
    pins: 'The notes island declares data-format="2", a format newer than this writer knows. A write is refused, UnknownFormat, and the page falls back to read-only in TD.',
    brief: () => buildBrief(release('current'), { notesAttrs: ' data-format="2"' }),
    rev: '2026-09-24T20:05:00.000Z',
    edits: [add(A.card, 'Refused: format 2 is not known.', '2026-09-24 20:00')],
  },
];

