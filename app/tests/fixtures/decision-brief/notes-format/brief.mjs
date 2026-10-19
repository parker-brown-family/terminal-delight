// Builds a small synthetic brief the way an agent following SKILL.md does: the release's
// notes.css inline, a body with one of each target, the release's markup block pasted
// before </body> with NOTES_FILE set, and the release's notes.js inline. No real brief's
// content goes into the fixtures.
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const HERE = dirname(fileURLToPath(import.meta.url));
export const SKILL = join(HERE, '..', '..');

// A release is the three files a brief inlines. 'current' is the working tree; anything
// else is a commit of this repository.
export function release(rev) {
  const read = (p) => rev === 'current'
    ? readFileSync(join(SKILL, p), 'utf8')
    : execFileSync('git', ['show', `${rev}:decision-brief/${p}`], { cwd: SKILL, encoding: 'utf8' });
  return { rev, js: read('assets/notes.js'), css: read('assets/notes.css'), markup: read('reference/notes-markup.html') };
}

// The custom properties notes.css leans on, so the note affordances have a size and a
// colour. base.css proper is left out to keep the fixtures small; layout plays no part.
const TOKENS = `:root {
  --surface-0: #111110; --surface-1: #1a1a19; --surface-2: #232321; --surface-3: #2e2e2b;
  --line: #3a3a36; --text-primary: #ffffff; --text-secondary: #c3c2b7; --text-muted: #8b8a80;
  --warning: #c98500; --s1: #3987e5; --mono: monospace; --sans: sans-serif;
}
body { margin: 0; background: var(--surface-0); color: var(--text-primary); font: 15px/1.6 var(--sans); }
main { max-width: 760px; margin: 0 auto; padding: 32px 24px 160px; }`;

export const BODY = `<main>
<h1>Fixture brief</h1>
<div class="finding"><h3>The island is the record</h3><p>Notes baked into the file are what an agent reads.</p></div>
<figure><div class="lbl">01 · Two writers, one file</div><svg viewBox="0 0 40 10" aria-label="two boxes"><rect x="1" y="1" width="16" height="8"/><rect x="23" y="1" width="16" height="8"/></svg><figcaption>The browser and Terminal Delight write the same bytes.</figcaption></figure>
<div class="card"><h3>Reference writer</h3><p>writer.mjs holds the rule.</p></div>
<div class="callout"><p>A callout that takes a note.</p></div>
<section class="grill">
<div class="ask"><h3>Stamp a revision on the island?</h3><p>Recommend yes.</p></div>
<div class="ask"><h3>Keep the plain-text mirror?</h3><p>Recommend yes.</p></div>
</section>
</main>`;

// opts:
//   notesFile   the value NOTES_FILE is set to (null leaves the line out)
//   notes       inner text of the notes island (null drops the island)
//   concurs     inner text of the concurs island (null drops it; ignored by releases without one)
//   notesAttrs / concursAttrs  replace the islands' open-tag attributes after the id
//   islandAfterScript  move the islands below notes.js
//   extraIslands  raw HTML pasted after the islands (a second pasted template, a decoy)
//   tail        raw HTML appended just before </body> (stale mirror comments)
//   bodyClose   false to leave out </body></html>
export function buildBrief(rel, opts = {}) {
  const o = { notesFile: 'brief.html', notes: '{}', concurs: '{}', bodyClose: true, ...opts };
  let m = rel.markup
    .replace(/^<!-- decision-brief — the markup[\s\S]*?-->\n\n/, '')
    .replace(/\n?<!-- then paste[^\n]*-->\n?$/, '\n');
  const islandRe = (id) => new RegExp(`<script type="application/json" id="${id}"[^>]*>\\{\\}</script>\\n`);
  const openTag = (id, attrs) => `<script type="application/json" id="${id}"${attrs ?? ''}>`;
  const hadConcurs = islandRe('report-concurs').test(m);
  const keepAttrs = (id) => (m.match(new RegExp(`id="${id}"([^>]*)>`)) || [])[1] ?? '';
  const notesAttrs = o.notesAttrs ?? keepAttrs('report-notes');
  const concursAttrs = o.concursAttrs ?? keepAttrs('report-concurs');
  let islands = '';
  if (o.notes !== null) islands += openTag('report-notes', notesAttrs) + o.notes + '</script>\n';
  if (hadConcurs && o.concurs !== null) islands += openTag('report-concurs', concursAttrs) + o.concurs + '</script>\n';
  if (o.extraIslands) islands += o.extraIslands;
  m = m.replace(islandRe('report-notes'), '\u0000').replace(islandRe('report-concurs'), '');
  if (o.islandAfterScript) m = m.replace('\u0000', '');
  else m = m.replace('\u0000', islands);
  if (o.notesFile === null) m = m.replace(/<script>\n[^\n]*\n\s*window\.NOTES_FILE = '[^']*';\n<\/script>\n/, '');
  else m = m.replace(/window\.NOTES_FILE = '[^']*';/, `window.NOTES_FILE = '${o.notesFile}';`);
  let html = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Fixture brief</title>
<style>
${TOKENS}
${rel.css}</style>
</head>
<body>
${BODY}

${m}<script>
${rel.js}</script>
`;
  if (o.islandAfterScript) html += islands;
  if (o.tail) html += o.tail;
  if (o.bodyClose) html += '</body>\n</html>\n';
  return html;
}
