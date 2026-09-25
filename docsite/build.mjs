#!/usr/bin/env node
/* ==========================================================================
   Builds the Terminal Delight docs into docsite/dist/, a site rooted at /
   for https://docs.terminal-delight.brownfamilysports.com.

     node docsite/build.mjs            (docsite/deploy.sh runs this, then ships it)

   Everything a page shares is written once:

     nav.json        the page list, in reading order, by group. It drives the
                     spine, the pager, the page map on the introduction and the
                     "soon" markers. A page is live when pages/<slug>.html exists.
     layout.html     the shell: head, top bar, spine, article frame, footer.
     pages/*.html    one content file per page: a JSON header, then either
                     plain HTML or up to three register sections (brief, story,
                     technical), then an optional <ol data-sources>.
     assets/         the shared shell, the docs components, the tube, the fonts.

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

const order = [];
for (const g of nav.groups) {
  for (const p of g.pages) {
    if (p.slug) order.push({ ...p, group: g.title, live: files.has(p.slug) });
  }
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

function spine(current) {
  return nav.groups.map((g) => {
    const items = g.pages.map((p) => {
      if (p.href) return `    <li><a href="${esc(p.href)}">${esc(p.title)}</a></li>`;
      const page = order.find((o) => o.slug === p.slug);
      if (!page.live) return `    <li><span class="is-soon">${esc(p.title)}</span></li>`;
      return `    <li><a href="${url(p.slug)}"${p.slug === current ? ' aria-current="page"' : ''}>${esc(p.title)}</a></li>`;
    }).join('\n');
    return `  <div class="td-group"><h6>${esc(g.title)}</h6><ul>\n${items}\n  </ul></div>`;
  }).join('\n');
}

function map(metaBySlug) {
  return '<div class="map">' + nav.groups.map((g) => {
    const items = g.pages.map((p) => {
      if (p.href) return `<li><a href="${esc(p.href)}">${esc(p.title)}</a></li>`;
      const page = order.find((o) => o.slug === p.slug);
      if (!page.live) return `<li class="soon">${esc(p.title)}</li>`;
      const sum = metaBySlug[p.slug] && metaBySlug[p.slug].summary;
      return `<li><a href="${url(p.slug)}">${esc(p.title)}</a>${sum ? `<small>${esc(sum)}</small>` : ''}</li>`;
    }).join('');
    return `<section><h3>${esc(g.title)}</h3><ul>${items}</ul></section>`;
  }).join('') + '</div>';
}

function pager(slug) {
  const i = live.findIndex((p) => p.slug === slug);
  const prev = i > 0 ? live[i - 1] : null, next = i >= 0 && i < live.length - 1 ? live[i + 1] : null;
  return (prev ? `<a href="${url(prev.slug)}"><small>Previous</small><span>← ${esc(prev.title)}</span></a>` : '<span></span>')
    + (next ? `<a href="${url(next.slug)}"><small>Next</small><span>${esc(next.title)} →</span></a>` : '');
}

function registers(regs) {
  const panel = (r, single) => {
    const head = `<div class="reg-head"><span class="reg-kicker">${LABEL[r.name]}</span><button class="td-copy" data-copy>${COPY_SVG} Copy</button></div>`;
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
    if (/data-docs-map/.test(body)) body = body.replace(/<div data-docs-map><\/div>/, map(metaBySlug));
    search.push({ u: url(p.slug), t: doc.meta.title || p.title, g: p.group, r: '', s: '', a: '', x: text(body).slice(0, 2400) });
  }
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
  const fill = {
    TITLE_TAG: p.slug === 'index' ? 'Terminal Delight docs' : `${title} · Terminal Delight docs`,
    DESCRIPTION: esc(doc.meta.description || ''),
    UPDATED: esc(`${doc.meta.updated} · true of ${doc.meta.trueOf}`),
    CANONICAL: nav.site + url(p.slug),
    OG_TYPE: p.slug === 'index' ? 'website' : 'article',
    PAGE_STYLE: doc.style ? `\n  <style>${doc.style}</style>` : '',
    SPINE: spine(p.slug),
    KICKER: esc(p.group),
    H1: esc(title),
    META_ROW: esc(`Updated ${doc.meta.updated} · true of ${doc.meta.trueOf}`),
    BODY: body,
    SOURCES: sources,
    PAGER: pager(p.slug),
    EDIT_URL: `${nav.repo}/blob/main/docsite/pages/${p.slug}.html`,
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
  plain: '<p class="lede">That address is not a page in these docs. It may be one that is still being written; everything that exists is listed here.</p>\n<div data-docs-map></div>',
}).replace('<link rel="canonical" href="' + nav.site + '/404" />', '');

/* ------------------------------------------------------------- the checks */

const assets = new Set(['/assets/td-shell.css', '/assets/td-shell.js', '/assets/td-glass.js', '/assets/td-docs.css', '/assets/fonts/fonts.css', '/favicon.ico', '/favicon.svg', '/search.json', '/sitemap.xml']);
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
for (const f of ['td-shell.css', 'td-shell.js', 'td-glass.js', 'td-docs.css']) await copyFile(join(ROOT, 'assets', f), join(OUT, 'assets', f));
for (const f of await readdir(join(ROOT, 'assets', 'fonts'))) await copyFile(join(ROOT, 'assets', 'fonts', f), join(OUT, 'assets', 'fonts', f));
for (const f of ['favicon.ico', 'favicon.svg']) await copyFile(join(ROOT, f), join(OUT, f));

console.log(`built ${live.length} pages (${order.length - live.length} still to write) and ${search.length} search entries into ${OUT}`);
