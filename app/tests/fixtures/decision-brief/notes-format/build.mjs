// Rebuilds every case's brief.html and edits.json from cases.mjs, then derives the rest
// with check.mjs --update. Run it in the same commit as any change to notes.js, notes.css
// or the markup block: the cases built from this release inline them verbatim.
//
//   node build.mjs [--case <name>]
//
// A case built from an older release that "has been saved" is saved for real: its
// pristine brief is opened in Chromium with that release's notes.js, the prep notes are
// added through the page's buttons, and the download becomes brief.html.
import { writeFileSync, mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { execFileSync } from 'node:child_process';
import { HERE, release, buildBrief } from './brief.mjs';
import { CASES } from './cases.mjs';
import { launch, openPage, applyEdits, saveIntoFile, tsDate } from './browser.mjs';

const args = process.argv.slice(2);
const only = args.includes('--case') ? args[args.indexOf('--case') + 1] : null;
const browser = await launch();
for (const c of CASES.filter((x) => !only || x.name === only)) {
  const dir = join(HERE, 'cases', c.name);
  mkdirSync(dir, { recursive: true });
  let html;
  if (c.brief) html = c.brief();
  else {
    const scratch = mkdtempSync(join(tmpdir(), 'notes-format-build-'));
    const p = join(scratch, 'brief.html');
    writeFileSync(p, buildBrief(release(c.release)));
    const { ctx, page } = await openPage(browser, 'file://' + p, { clock: tsDate(c.prep[0].ts) });
    await applyEdits(page, c.prep);
    html = (await saveIntoFile(page, null)).bytes.toString('utf8');
    await ctx.close();
    rmSync(scratch, { recursive: true, force: true });
  }
  writeFileSync(join(dir, 'brief.html'), html);
  writeFileSync(join(dir, 'edits.json'), JSON.stringify({ rev: c.rev, edits: c.edits }, null, 1) + '\n');
  writeFileSync(join(dir, 'expect.json'), JSON.stringify({ release: c.release, pins: c.pins }, null, 1) + '\n');
  console.log('built', c.name);
}
await browser.close();
execFileSync(process.execPath, [join(HERE, 'check.mjs'), '--update', ...(only ? ['--case', only] : [])], { stdio: 'inherit' });
