#!/usr/bin/env node
/* ==========================================================================
   Builds the Terminal Delight docs into docsite/dist/, a site rooted at /
   for https://docs.terminal-delight.brownfamilysports.com.

     node docsite/build.mjs            (docsite/deploy.sh runs this, then ships it)

   Everything a page shares is written once:

     nav.json        the page list, in reading order, by chapter. It numbers
                     every page, and drives the spine, the pager, the chapter
                     table on the introduction and the 404, the Reference menu
                     and the "soon" markers. A page is live when
                     pages/<slug>.html exists.
     layout.html     the shell: head, header, spine, article frame, footer.
     pages/*.html    one content file per page: a JSON header, then either
                     plain HTML or up to three register sections (brief, story,
                     technical), then an optional <ol data-sources>.
     assets/         docs.css and docs.js (the look, the themes, the keys),
                     td-shell.js (registers, copy, search), td-glass.js (the
                     tube), td-docs.css (figures), the Omarchy palettes and
                     wallpapers, the fonts.

   The build also writes search.json (full-text, by heading), sitemap.xml and
   404.html, and it refuses to write a site with a broken internal link, a
   dangling citation, a /docsite/ path, or one of the phrases Parker's writing
   ledger bans. No dependencies, so a fresh clone deploys without an install.
   ========================================================================== */
import { readdir, readFile, writeFile, mkdir, copyFile, rm } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const PAGES = join(HERE, 'pages');
const OUT = join(HERE, 'dist');

const nav = JSON.parse(await readFile(join(HERE, 'nav.json'), 'utf8'));
const layout = await readFile(join(HERE, 'layout.html'), 'utf8');
const files = new Set((await readdir(PAGES)).filter((f) => f.endsWith('.html')).map((f) => f.slice(0, -5)));

const problems = [];
const warnings = [];
const fail = (where, what) => problems.push(`${where}: ${what}`);
const warn = (where, what) => warnings.push(`${where}: ${what}`);

/* ------------------------------------------------------------ the page list */

/* Chapters are numbered in reading order, and a page by its place in its
   chapter: Install is 01.1, the first page of a chapter is 0N.0. The numbers
   are the field manual's, for finding your way, and are drawn by CSS, so
   search, copy and the page title never carry them. */
const pad = (n) => String(n).padStart(2, '0');
const order = [];
const chapters = [];
for (const g of nav.groups) {
  const ch = { n: pad(chapters.length + 1), title: g.title, short: g.short || g.title, pages: [] };
  g.pages.forEach((p, k) => {
    if (!p.slug) return;
    const page = { ...p, group: g.title, chapter: ch, num: `${ch.n}.${k}`, live: files.has(p.slug) };
    order.push(page); ch.pages.push(page);
  });
  if (ch.pages.length) chapters.push(ch);
}
for (const f of files) if (!order.some((p) => p.slug === f)) fail(`pages/${f}.html`, 'is not listed in nav.json');
const live = order.filter((p) => p.live);
const url = (slug) => (slug === 'index' ? '/' : '/' + slug);

/* ------------------------------------------------------------------ helpers */

const esc = (s) => String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const text = (html) => html
  .replace(/<script[\s\S]*?<\/script>|<style[\s\S]*?<\/style>|<svg[\s\S]*?<\/svg>/g, ' ')
  .replace(/<[^>]+>/g, ' ')
  .replace(/&nbsp;/g, ' ').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&amp;/g, '&')
  .replace(/\s+/g, ' ').trim();
const slugify = (s) => text(s).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'section';
const COPY_SVG = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/></svg>';
const LABEL = { brief: 'Brief', story: 'Story', technical: 'Technical' };

/* Give every h2 and h3 an id and a # link, so any part of a register can be
   linked to and found by search. */
function anchor(html, prefix, used) {
  return html.replace(/<h([23])(\s[^>]*)?>([\s\S]*?)<\/h\1>/g, (m, lvl, attrs = '', inner) => {
    if (/\bid="/.test(attrs)) return m;
    let id = (prefix ? prefix + '-' : '') + slugify(inner), n = 2;
    while (used.has(id)) id = id.replace(/-\d+$/, '') + '-' + n++;
    used.add(id);
    return `<h${lvl}${attrs} id="${id}">${inner}<a class="hash" href="#${id}" aria-label="Link to this section">#</a></h${lvl}>`;
  });
}

/* ------------------------------------------------------------------- lint
   Parker's writing ledger (~/.claude/writing/ai-slop.md, house-style.md) as
   checks. A phrase here is one he has flagged or banned outright. */
const BANNED = [
  /\bdelve/i, /\bleverag(e|es|ed|ing)\b/i, /\butiliz/i, /\btapestry\b/i, /\brealm\b/i, /\bshowcase/i,
  /\bseamless/i, /\belevate/i, /testament to/i, /in today'?s/i, /it'?s worth noting/i, /let'?s dive/i,
  /buckle up/i, /(^|[.!?]\s+)(Moreover|Furthermore),/, /\bsupercharge/i, /\bunlock(s|ed|ing)?\b/i,
  /it'?s not just\b/i, /\bgame[- ]changer/i, /stated honestly|to be honest|\bhonestly\b/i,
];
/* the split negation, sentence-anchored as the ledger's pattern is */
const SPLIT_NEG = /(?:^|[.!?]\s+)(?:The|It|They|That|This|[A-Z][a-z]+)(?:\s+\w+){0,2}\s+(?:is|was|are|were|did|does|do)\s+not\s+[^.!?<]{0,60}[.!?]\s+(?:It|They|The)\s+\w+\s+(?:is|was|are|were|did|does|do)\b/;
function lint(where, html) {
  const t = text(html);
  for (const re of BANNED) { const m = t.match(re); if (m) fail(where, `banned phrase "${m[0].trim()}"`); }
  const s = t.match(SPLIT_NEG); if (s) fail(where, `split negation: "${s[0].trim().slice(0, 90)}"`);
  const lab = html.match(/<p[^>]*>\s*<(b|strong)>[^<]{1,40}:<\/\1>/); if (lab) fail(where, `labelled declarative opener: ${text(lab[0])}`);
  const dot = html.match(/<h[1-3][^>]*>([^<]*\.)\s*(<a class="hash"|<\/h)/); if (dot) fail(where, `a heading ending in a full stop: "${dot[1]}"`);
  const q = html.match(/<label[^>]*>[^<]*\?\s*<\/label>/); if (q) fail(where, `a label written as a question: ${text(q[0])}`);
}

/* -------------------------------------------------------------- the pieces */

/* The spine: every chapter, then the pages of the one you are in. The chapter
   is lit weakly and the page strongly, so where you are reads at two scales. */
function spine(current) {
  const here = order.find((o) => o.slug === current);
  const first = (ch) => ch.pages.find((p) => p.live) || ch.pages[0];
  const chs = chapters.map((ch) => {
    const lit = here && here.chapter === ch;
    return `    <li><a href="${url(first(ch).slug)}"${lit ? ' class="weak"' : ''}><span class="n">${ch.n} /</span>${esc(ch.short)}</a></li>`;
  }).join('\n');
  let out = `  <div class="td-group"><h6>Contents</h6><ul>\n${chs}\n  </ul></div>`;
  if (here) {
    const items = here.chapter.pages.map((p) => {
      if (!p.live) return `    <li><span class="is-soon"><span class="n">${p.num}</span>${esc(p.title)}</span></li>`;
      return `    <li><a href="${url(p.slug)}"${p.slug === current ? ' aria-current="page"' : ''}><span class="n">${p.num}</span>${esc(p.title)}</a></li>`;
    }).join('\n');
    out += `\n  <div class="td-group"><h6>In ${here.chapter.n} · ${esc(here.chapter.title)}</h6><ul>\n${items}\n  </ul></div>`;
  }
  return out;
}

/* Every chapter on one quiet table, generated from nav.json, for the
   introduction and the 404. */
function chapterTable() {
  return '<table class="q">' + chapters.map((ch) => {
    const items = ch.pages.map((p) => (p.live ? `<a href="${url(p.slug)}">${esc(p.title)}</a>` : `<span class="soon-link">${esc(p.title)}</span>`))
      .join('<span class="dot"> · </span>');
    return `<tr><td>${ch.n} ${esc(ch.short)}</td><td>${items}</td></tr>`;
  }).join('') + '</table>';
}

function pager(slug) {
  const i = live.findIndex((p) => p.slug === slug);
  const prev = i > 0 ? live[i - 1] : null, next = i >= 0 && i < live.length - 1 ? live[i + 1] : null;
  return (prev ? `<a class="prev" href="${url(prev.slug)}"><small>Previous</small>← <span class="n">${prev.num}</span>${esc(prev.title)}</a>` : '')
    + (next ? `<a class="next" href="${url(next.slug)}"><small>Next</small><span class="n">${next.num}</span>${esc(next.title)} →</a>` : '');
}

/* A command block gets a prompt per line, and its comments stay visible but
   are never copied (td-shell.js drops them when it joins the lines). */
function commands(html) {
  return html.replace(/<pre class="cmd"><code>([\s\S]*?)<\/code><\/pre>/g, (m, body) => {
    const lines = body.replace(/\n+$/, '').split('\n').map((l) => (l.trim().startsWith('#') ? `<span class="cm">${l}</span>` : `<span class="ln">${l}</span>`));
    return `<pre class="cmd"><code>${lines.join('\n')}</code></pre>`;
  });
}

/* ------------------------------------------------------ the app's key sheet
   The Keymapping tab opens the sheet F1 shows in the app, read straight out of
   the app's source when the docs are built: the SHORTCUTS columns in main.rs,
   and their English strings in lang.rs. So the sheet cannot drift from the app.
   A few of the app's own rows no longer say what the code does (#803 records
   them). Each fix below gives the code's version and where it comes from, and
   the build refuses to ship a fix whose row has disappeared, because that means
   the app's row was put right and the fix should be deleted. */
const APP_KEY_FIXES = [
  { key: 'Ctrl+Shift+PgUp / PgDn', text: 'Move tab (in / across groups)', to: [null, 'Move tab left / right, within its group'],
    why: 'a tab never crosses a group (main.rs, the tab move)' },
  { key: 'theme icon (top-right)', text: 'DESIGN — Themes & colour wheel', to: ['🎨 (bottom-right)', null],
    why: 'the 🎨 sits bottom-right' },
  { key: '▲ / ▼', text: 'Jump to your previous / next message', drop: true,
    why: 'the jump glyph is hidden (pane.rs)' },
  { key: '🤖 (mother bar)', text: 'MCP — read-only agent-watch surface', drop: true,
    why: 'the wall has no chrome glyph' },
  { key: 'Ctrl+Shift+A', text: 'MCP — read-only agent-watch surface', to: [null, 'The agent wall: every agent pane as a card'],
    why: 'OpenAgentPanel raises the agent wall (pane.rs, main.rs)' },
  { key: 'wheel / shift+wheel', text: 'Pan a zoomed FOCUS read down / sideways', to: ['wheel', 'Pan a zoomed FOCUS read; it wraps, so only down'],
    why: 'the reader wraps and has no horizontal axis' },
  { key: 'Ctrl+Alt+T', text: 'New window (quick scratch)', to: [null, 'Opens Terminal Delight on GNOME, once scripts/install-hotkey.sh binds it'],
    why: 'a desktop hotkey; the app does nothing with the chord' },
];
async function appKeys() {
  const where = 'the key sheet';
  const main = await readFile(join(ROOT, 'app', 'src', 'main.rs'), 'utf8');
  const lang = await readFile(join(ROOT, 'app', 'src', 'lang.rs'), 'utf8');
  const block = lang.match(/pub const EN: Strings = Strings \{([\s\S]*?)\n\};/);
  if (!block) { fail(where, 'app/src/lang.rs has no EN strings block'); return []; }
  const en = {};
  for (const m of block[1].matchAll(/^\s+(\w+): "((?:[^"\\]|\\.)*)",/gm)) {
    en[m[1]] = m[2].replace(/\\u\{([0-9a-f]+)\}/gi, (_, h) => String.fromCodePoint(parseInt(h, 16))).replace(/\\"/g, '"');
  }
  const start = main.indexOf('let col_a = div()'), end = main.indexOf('// The FEATURES view', start);
  if (start < 0 || end < 0) { fail(where, 'the help sheet in app/src/main.rs moved; update appKeys()'); return []; }
  const region = main.slice(start, end), split = region.indexOf('let col_b');
  const string = (name) => (name in en ? en[name] : (fail(where, `lang.rs has no English string ${name}`), name));
  const label = (expr) => {
    let m;
    if ((m = expr.match(/^"((?:[^"\\]|\\.)*)"$/))) return m[1];
    if ((m = expr.match(/^s\.(\w+)$/))) return string(m[1]);
    if ((m = expr.match(/^&format!\("((?:[^"\\]|\\.)*)",\s*s\.(\w+)\)$/))) return m[1].replace('{}', string(m[2]));
    fail(where, `a help row it cannot read: ${expr}`); return expr;
  };
  const ROW = /row\(\s*("(?:[^"\\]|\\.)*"|&format!\("(?:[^"\\]|\\.)*",\s*s\.\w+\)|s\.\w+)\s*,\s*s\.(\w+)\s*\)/g;
  const cols = [region.slice(0, split), region.slice(split)].map((part) =>
    [...part.matchAll(/section\(\s*s\.(\w+),\s*vec!\[([\s\S]*?)\]\s*,?\s*\)/g)].map((sm) => ({
      title: string(sm[1]),
      rows: [...sm[2].matchAll(ROW)].map((rm) => [label(rm[1]), string(rm[2])]),
    })));
  const count = cols.flat().reduce((n, s) => n + s.rows.length, 0);
  if (cols.some((c) => !c.length) || count < 30) fail(where, `read only ${count} rows from app/src/main.rs; the help sheet's shape changed`);
  for (const f of APP_KEY_FIXES) {
    let hit = false;
    for (const sec of cols.flat()) {
      sec.rows = sec.rows.flatMap((r) => {
        if (r[0] !== f.key || r[1] !== f.text) return [r];
        hit = true;
        return f.drop ? [] : [[f.to[0] ?? r[0], f.to[1] ?? r[1]]];
      });
    }
    if (!hit) fail(where, `the fix for "${f.key}" matches no row: the app changed it, so check the row and delete the fix`);
  }
  return cols;
}
function keySheet(cols) {
  const sec = (s) => `<h4>${esc(s.title)}</h4><dl>${s.rows.map((r) => `<dt>${esc(r[0])}</dt><dd>${esc(r[1])}</dd>`).join('')}</dl>`;
  return '<div class="td-pop td-sheet" id="td-sheet" role="dialog" aria-label="Every key in Terminal Delight">'
    + '<div class="sh"><h3>▸ TERMINAL DELIGHT · KEYS</h3><span class="k">F1 in the app · k here · Esc to close</span></div>'
    + `<div class="cols">${cols.map((c) => `<div>${c.map(sec).join('')}</div>`).join('')}</div>`
    + '<button class="td-demo-btn" type="button" data-demo>🖥️ Open an example window</button>'
    + '<p class="td-demo-sub">A drawing of the app, every pane curved on its own the way the app curves it, in the theme you picked. In the app this button opens a real window of your layout, filled with lorem ipsum.</p>'
    + '<p class="kf">The sheet F1 opens in the app, read from the app\'s source. A few of the app\'s own rows are out of date, and here they say what the code does. '
    + '<a href="/keys">Every key, surface by surface →</a></p></div>';
}

/* ------------------------------------------------------ the example window
   The key sheet's big button opens a drawing of the app: the info kiosk's
   hero window, whose panes td-panes.js bends one by one the way the app does.
   It is read out of info.html between td-window markers (two CSS ranges and
   the markup), so the kiosk and the docs draw the same window from one
   source. The CSS is linked by every page, because the tube's snapshot reads
   linked sheets; the markup is fetched only when someone opens it. */
async function exampleWindow() {
  const where = 'the example window';
  const src = await readFile(join(ROOT, 'info.html'), 'utf8');
  const css = [...src.matchAll(/\/\* td-window:start[^*]*\*\/([\s\S]*?)\/\* td-window:end \*\//g)].map((m) => m[1].trim());
  const html = src.match(/<!-- td-window:start[^>]*-->([\s\S]*?)<!-- td-window:end -->/);
  if (css.length !== 2) fail(where, `info.html has ${css.length} td-window CSS ranges, and the docs read exactly two (the window, its container queries)`);
  if (!html || !/data-pane-warp/.test(html[1])) { fail(where, 'info.html has no td-window markup carrying data-pane-warp'); return null; }
  return {
    css: '/* Generated by docsite/build.mjs from info.html (td-window markers). Edit it there. */\n' + css.join('\n\n') + '\n',
    html: html[1].trim().replace(' id="win"', ' data-wear="docs"') + '\n',
  };
}

/* The Reference tab's menu, from nav.json, so its links are checked like any other. */
const refMenu = (nav.reference || []).map((r) => `<a href="${esc(r.href)}">${esc(r.title)}<small>${esc(r.note)}</small></a>`).join('');

function registers(regs) {
  const panel = (r, single) => {
    const head = `<div class="reg-head"><span class="reg-kicker">${LABEL[r.name]}</span><button class="td-copy" data-copy>${COPY_SVG} Copy this version</button></div>`;
    const h2 = r.headline ? `<h2>${r.headline}</h2>` : '';
    return `<section class="reg-panel${single ? ' single' : ''}" id="${r.name}" role="tabpanel" data-register="${LABEL[r.name]}">\n${head}\n${h2}\n${r.body}\n</section>`;
  };
  if (regs.length === 1) return `<div class="reg">\n${panel(regs[0], true)}\n</div>`;
  const radios = regs.map((r, i) => `<input type="radio" name="register" id="r-${r.name}"${i === 0 ? ' checked' : ''} aria-controls="${r.name}">`).join('\n');
  const tabs = regs.map((r) => `<label for="r-${r.name}" role="tab">${LABEL[r.name]}</label>`).join('\n');
  return `<div class="reg">\n${radios}\n<div class="reg-tabs" role="tablist" aria-label="Registers">\n${tabs}\n</div>\n<div class="reg-body">\n${regs.map((r) => panel(r)).join('\n')}\n</div>\n</div>`;
}

/* ----------------------------------------------------------------- parsing */

function parse(slug, src) {
  const where = `pages/${slug}.html`;
  const head = src.match(/<script type="application\/json" data-page>([\s\S]*?)<\/script>/);
  if (!head) { fail(where, 'has no <script type="application/json" data-page> header'); return null; }
  let meta;
  try { meta = JSON.parse(head[1]); } catch (e) { fail(where, 'header is not JSON: ' + e.message); return null; }
  let rest = src.replace(head[0], '');
  let style = '';
  rest = rest.replace(/<style>([\s\S]*?)<\/style>/, (_, s) => { style = s; return ''; });
  let sources = '';
  rest = rest.replace(/<ol data-sources>([\s\S]*?)<\/ol>/, (_, s) => { sources = s; return ''; });
  const regs = [];
  rest = rest.replace(/<section data-register="(brief|story|technical)"([^>]*)>([\s\S]*?)<\/section>/g, (_, name, attrs, body) => {
    const hl = attrs.match(/data-headline="([^"]*)"/);
    regs.push({ name, headline: hl ? hl[1] : '', body: body.trim() });
    return '';
  });
  if (regs.length && rest.trim()) fail(where, 'has content outside its register sections');
  const want = ['brief', 'story', 'technical'];
  if (regs.length && regs.map((r) => r.name).join() !== want.filter((n) => regs.some((r) => r.name === n)).join()) fail(where, 'registers must come in the order brief, story, technical');
  if (regs.length && regs[0].name !== 'brief') fail(where, 'a page with registers must have a Brief');
  if (regs.length && regs[0].headline) fail(where, 'the Brief carries no headline — the title is its headline');
  return { meta, style, sources: sources.trim(), regs, plain: regs.length ? '' : rest.trim() };
}

/* ------------------------------------------------------------------- build */

await rm(OUT, { recursive: true, force: true });
await mkdir(join(OUT, 'assets', 'fonts'), { recursive: true });

const parsed = {};
for (const p of live) {
  parsed[p.slug] = parse(p.slug, await readFile(join(PAGES, p.slug + '.html'), 'utf8'));
}
const metaBySlug = Object.fromEntries(Object.entries(parsed).filter(([, v]) => v).map(([k, v]) => [k, v.meta]));

const search = [];
const pagesOut = {};
const appSheet = keySheet(await appKeys());
const demoWindow = await exampleWindow();

function renderPage(p, doc) {
  const where = `pages/${p.slug}.html`;
  const used = new Set(['brief', 'story', 'technical']);
  let body;
  if (doc.regs.length) {
    const regs = doc.regs.map((r) => ({ ...r, body: anchor(r.body, r.name, used) }));
    for (const r of regs) lint(`${where} (${r.name})`, (r.headline ? `<h2>${r.headline}</h2>` : '') + r.body);
    const brief = regs[0];
    const words = text(brief.body.replace(/<p class="invite">[\s\S]*?<\/p>/, '').replace(/<pre[\s\S]*?<\/pre>/g, '')).split(' ').length;
    if (words < 60 || words > 260) warn(where, `Brief is ${words} words (aim for 120 to 200)`);
    if (regs.length > 1 && !/class="invite"/.test(brief.body)) warn(where, 'Brief has no closing invitation to the other registers');
    body = registers(regs);
    for (const r of regs) {
      const chunks = ((r.headline ? `<h2 id="${r.name}">${r.headline}</h2>` : '') + r.body).split(/(?=<h[23][^>]*id=")/);
      for (const c of chunks) {
        const h = c.match(/^<h[23][^>]*id="([^"]+)"[^>]*>([\s\S]*?)<\/h[23]>/);
        const x = text(h ? c.slice(h[0].length) : c);
        if (!x) continue;
        search.push({ u: url(p.slug), t: doc.meta.title || p.title, g: p.group, r: LABEL[r.name], s: h ? text(h[2]).replace(/#$/, '').trim() : '', a: h ? h[1] : r.name, x: x.slice(0, 2400) });
      }
    }
  } else {
    body = anchor(doc.plain, '', used);
    lint(where, body);
    body = body.replace(/<div data-docs-chapters><\/div>/, chapterTable());
    search.push({ u: url(p.slug), t: doc.meta.title || p.title, g: p.group, r: '', s: '', a: '', x: text(body).slice(0, 2400) });
    body = `<div class="plain">\n${body}\n</div>`;
  }
  body = commands(body);
  /* A link to a page that is listed but not written yet is drawn as text,
     and becomes a link by itself the day that page exists. */
  const soonUrls = new Set(order.filter((o) => !o.live).map((o) => url(o.slug)));
  const unlinkSoon = (html) => html.replace(/<a href="(\/[a-z0-9-]*)(#[^"]*)?">([\s\S]*?)<\/a>/g, (m, path, hash, inner) => {
    if (!soonUrls.has(path)) return m;
    warn(where, `link to ${path} waits for that page`);
    return `<span class="soon-link" title="This page is still being written">${inner}</span>`;
  });
  body = unlinkSoon(body);
  const sources = doc.sources ? `<h2 class="src-head" id="sources">Sources</h2>\n<ol class="src">\n${unlinkSoon(doc.sources)}\n</ol>` : '';
  if (doc.sources) lint(`${where} (sources)`, doc.sources);
  const title = doc.meta.title || p.title;
  const numbered = !!p.num;
  const tab = (name) => {
    const on = name === 'keys' ? p.slug === 'keys'
      : name === 'languages' ? p.slug === 'languages'
      : name === 'reference' ? p.group === 'Reference' && p.slug !== 'languages'
      : !['keys', 'languages'].includes(p.slug) && p.group !== 'Reference';
    return on ? ' aria-current="page"' : '';
  };
  const fill = {
    TITLE_TAG: p.slug === 'index' ? 'Terminal Delight docs' : `${title} · Terminal Delight docs`,
    DESCRIPTION: esc(doc.meta.description || ''),
    UPDATED: esc(`${doc.meta.updated} · true of ${doc.meta.trueOf}`),
    CANONICAL: nav.site + url(p.slug),
    OG_TYPE: p.slug === 'index' ? 'website' : 'article',
    PAGE_STYLE: doc.style ? `\n  <style>${doc.style}</style>` : '',
    SPINE: spine(p.slug),
    DOC_ATTRS: numbered ? ` class="td-doc numbered" style="--pn:'${p.num}'"` : ' class="td-doc"',
    H1_ATTRS: numbered ? ` data-n="${p.num.endsWith('.0') ? p.num.slice(0, -2) : p.num}"` : '',
    H1: esc(title),
    META_ROW: esc(`Updated ${doc.meta.updated} · true of ${doc.meta.trueOf}`),
    TAB_DOCS: tab('docs'), TAB_REF: tab('reference'), TAB_LANG: tab('languages'), TAB_KEYS: tab('keys'),
    REF_MENU: refMenu,
    APP_KEYS: appSheet,
    BODY: body,
    SOURCES: sources,
    PAGER: pager(p.slug),
    EDIT_URL: p.slug === "404" ? `${nav.repo}/blob/main/docsite/build.mjs` : `${nav.repo}/blob/main/docsite/pages/${p.slug}.html`,
    INSTALL_CURRENT: p.slug === 'install' ? ' aria-current="page"' : '',
    KIOSK: nav.kiosk,
    REPO: nav.repo,
  };
  if (!doc.meta.description) fail(where, 'has no description');
  if (!doc.meta.updated || !doc.meta.trueOf) fail(where, 'needs "updated" and "trueOf" in its header');
  const html = layout.replace(/\{\{([A-Z0-9_]+)\}\}/g, (m, k) => (k in fill ? fill[k] : (fail(where, `layout token ${m} has no value`), m)));
  /* anything still shaped like a token was never filled, whatever its name */
  const left = html.match(/\{\{[^}]{1,40}\}\}/); if (left) fail(where, `unfilled token ${left[0]}`);
  return html;
}

for (const p of live) {
  const doc = parsed[p.slug];
  if (!doc) continue;
  pagesOut[p.slug] = renderPage(p, doc);
}

/* the 404 page wears the same shell and carries the map */
pagesOut['404'] = renderPage({ slug: '404', title: 'Not here', group: 'Docs' }, {
  meta: { title: 'Not here', description: 'That page is not in the Terminal Delight docs.', updated: new Date().toISOString().slice(0, 10), trueOf: 'the docs as built' },
  style: '', sources: '', regs: [],
  plain: '<p class="lede">That address is not a page in these docs. It may be one that is still being written; everything that exists is listed here.</p>\n<div data-docs-chapters></div>',
}).replace('<link rel="canonical" href="' + nav.site + '/404" />', '');

/* ------------------------------------------------------------- the checks */

const SHIPPED = ['td-shell.js', 'td-glass.js', 'td-panes.js', 'td-docs.css', 'docs.css', 'docs.js', 'kiosk-theme.js'];
const WALLS = (await readdir(join(ROOT, 'assets', 'omarchy', 'bg'))).filter((f) => f.endsWith('.webp'));
/* single recordings a page plays (docs.js "clips"), and example briefs a reader
   can open and annotate in the browser; both are copied whole */
const CLIPS = (await readdir(join(ROOT, 'assets', 'clips')).catch(() => [])).filter((f) => /\.(mp4|jpg)$/.test(f));
const BRIEFS = (await readdir(join(ROOT, 'assets', 'briefs')).catch(() => [])).filter((f) => f.endsWith('.html'));
const assets = new Set([...SHIPPED.map((f) => '/assets/' + f), ...CLIPS.map((f) => '/assets/clips/' + f), ...BRIEFS.map((f) => '/assets/briefs/' + f),
  '/assets/td-window.css', '/assets/td-window.html', '/assets/fonts/fonts.css', '/favicon.ico', '/favicon.svg', '/search.json', '/sitemap.xml']);
const liveUrls = new Set(live.map((p) => url(p.slug)));
for (const [slug, html] of Object.entries(pagesOut)) {
  const where = `pages/${slug}.html`;
  if (/(href|src)="\/docsite\//.test(html)) fail(where, 'a /docsite/ path survived');
  const ids = new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
  for (const m of html.matchAll(/href="([^"]+)"/g)) {
    const h = m[1];
    if (h.startsWith('#')) { if (h.length > 1 && !ids.has(h.slice(1))) fail(where, `anchor ${h} has no target`); continue; }
    if (!h.startsWith('/') || h.startsWith('//')) continue;
    const [path, hash] = h.split('#');
    if (!liveUrls.has(path) && !assets.has(path)) fail(where, `link to ${h} goes nowhere`);
    if (hash && liveUrls.has(path) && path !== url(slug)) {
      const other = pagesOut[path === '/' ? 'index' : path.slice(1)];
      if (other && !new RegExp(`\\sid="${hash}"`).test(other)) fail(where, `link to ${h}: that page has no #${hash}`);
    }
  }
  for (const m of html.matchAll(/<sup><a href="#(s\d+)">/g)) if (!ids.has(m[1])) fail(where, `citation ${m[1]} has no source`);
  /* a clip's video and poster are src and poster, not href, so the link check above never sees them */
  for (const m of html.matchAll(/(?:src|poster)="(\/assets\/clips\/[^"]+)"/g)) if (!assets.has(m[1])) fail(where, `${m[1]} is not in assets/clips`);
}

/* the Introduction's reel is one recording per theme; docs.js swaps them with the theme */
const REEL = (await readdir(join(ROOT, 'assets', 'reel')).catch(() => [])).filter((f) => /\.(mp4|jpg)$/.test(f));
for (const [, name] of (await readFile(join(ROOT, 'assets', 'kiosk-theme.js'), 'utf8')).matchAll(/"name":"([a-z0-9-]+)"/g)) {
  if (!WALLS.includes(name + '.webp')) fail('assets/omarchy/bg', `no wallpaper for the ${name} theme`);
  if (!REEL.includes(name + '.mp4') || !REEL.includes(name + '.jpg')) fail('assets/reel', `no reel take and poster for the ${name} theme`);
}

if (warnings.length) console.warn('warnings:\n  ' + warnings.join('\n  '));
if (problems.length) {
  console.error('build refused:\n  ' + problems.join('\n  '));
  process.exit(1);
}

/* ------------------------------------------------------------------ write */

for (const [slug, html] of Object.entries(pagesOut)) await writeFile(join(OUT, slug + '.html'), html);
await writeFile(join(OUT, 'search.json'), JSON.stringify(search));
await writeFile(join(OUT, 'sitemap.xml'), '<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
  + live.map((p) => `  <url><loc>${nav.site}${url(p.slug)}</loc><lastmod>${parsed[p.slug].meta.updated}</lastmod></url>`).join('\n') + '\n</urlset>\n');
for (const f of SHIPPED) await copyFile(join(ROOT, 'assets', f), join(OUT, 'assets', f));
if (demoWindow) {
  await writeFile(join(OUT, 'assets', 'td-window.css'), demoWindow.css);
  await writeFile(join(OUT, 'assets', 'td-window.html'), demoWindow.html);
}
/* every theme wears its own Omarchy wallpaper; docs.js asks for them by palette name */
await mkdir(join(OUT, 'assets', 'omarchy', 'bg'), { recursive: true });
for (const f of WALLS) await copyFile(join(ROOT, 'assets', 'omarchy', 'bg', f), join(OUT, 'assets', 'omarchy', 'bg', f));
for (const f of await readdir(join(ROOT, 'assets', 'fonts'))) await copyFile(join(ROOT, 'assets', 'fonts', f), join(OUT, 'assets', 'fonts', f));
await mkdir(join(OUT, 'assets', 'reel'), { recursive: true });
for (const f of REEL) await copyFile(join(ROOT, 'assets', 'reel', f), join(OUT, 'assets', 'reel', f));
for (const [dir, files] of [['clips', CLIPS], ['briefs', BRIEFS]]) {
  await mkdir(join(OUT, 'assets', dir), { recursive: true });
  for (const f of files) await copyFile(join(ROOT, 'assets', dir, f), join(OUT, 'assets', dir, f));
}
for (const f of ['favicon.ico', 'favicon.svg']) await copyFile(join(ROOT, f), join(OUT, f));

console.log(`built ${live.length} pages (${order.length - live.length} still to write) and ${search.length} search entries into ${OUT}`);
