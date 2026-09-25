#!/usr/bin/env node
/* ==========================================================================
   Builds docsite/ into docsite/dist/, a site rooted at / for
   docs.terminal-delight.brownfamilysports.com.

     node docsite/build.mjs        (docsite/deploy.sh runs this, then ships it)

   The pages are written to work in place first: GitHub Pages serves this
   repository's root, so /docsite/workbench.html is live on the kiosk domain
   the moment it merges, with the shared shell at /assets/td-shell.*. This
   build is the move to the docs subdomain, and it does exactly three things:

     1. /docsite/…  becomes  /…            (the docs are the root there)
     2. the shell, the curved-tube engine, the self-hosted fonts and the
        favicon are copied in, because /assets/ and /favicon.* do not exist
        on the subdomain
     3. links to the kiosks (/info, /omarchy, /global, /tv, /gamba,
        /start-crawl) become absolute links to the kiosk domain

   No dependencies, so a fresh clone can deploy without an install.
   ========================================================================== */
import { readdir, readFile, writeFile, mkdir, copyFile, rm } from 'node:fs/promises';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..');
const OUT = join(HERE, 'dist');
const KIOSK = 'https://terminal-delight.brownfamilysports.com';
const KIOSK_PATHS = ['info', 'omarchy', 'global', 'tv', 'gamba', 'start-crawl', 'agents'];

await rm(OUT, { recursive: true, force: true });
await mkdir(join(OUT, 'assets'), { recursive: true });

for (const f of ['td-shell.css', 'td-shell.js', 'td-glass.js']) await copyFile(join(ROOT, 'assets', f), join(OUT, 'assets', f));
await mkdir(join(OUT, 'assets', 'fonts'), { recursive: true });
for (const f of await readdir(join(ROOT, 'assets', 'fonts'))) await copyFile(join(ROOT, 'assets', 'fonts', f), join(OUT, 'assets', 'fonts', f));
for (const f of ['favicon.ico', 'favicon.svg']) await copyFile(join(ROOT, f), join(OUT, f));

const kioskLink = new RegExp(`href="/(${KIOSK_PATHS.join('|')})(\\.html)?(#[^"]*)?"`, 'g');
let pages = 0;
for (const name of await readdir(HERE)) {
  if (!name.endsWith('.html')) continue;
  let html = await readFile(join(HERE, name), 'utf8');
  html = html
    .replace(/href="\/docsite\/"/g, 'href="/"')
    .replace(/href="\/docsite\//g, 'href="/')
    .replace(kioskLink, (_, p, _ext, hash) => `href="${KIOSK}/${p}${hash || ''}"`);
  if (/(href|src)="\/docsite\//.test(html)) throw new Error(`${name}: a /docsite/ path survived the rewrite`);
  await writeFile(join(OUT, name), html);
  pages++;
}
console.log(`built ${pages} pages into ${OUT}`);
