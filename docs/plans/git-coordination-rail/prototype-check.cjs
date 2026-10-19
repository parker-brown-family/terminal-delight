// Browser checks for the Git coordination decision brief; requires Playwright and Chromium.
// Run: node docs/plans/git-coordination-rail/prototype-check.cjs
const {createRequire}=require('node:module');
let pw;
for(const base of [__filename,'/home/parker/Work/td-film-kit/package.json','/tmp/td-brief-check/package.json']) {
  try { pw=createRequire(base)('playwright'); break; } catch {}
}
if(!pw) throw new Error('Playwright is required for browser verification.');
const {chromium}=pw;
const fs=require('fs'),http=require('http'),assert=require('assert/strict');
const root='/home/parker/Work/terminal-delight';
const path=root+'/reports/2026-10-18-git-coordination-rail.html';
(async()=>{
  let html=fs.readFileSync(path,'utf8');
  const server=http.createServer((q,r)=>{r.setHeader('Content-Type','text/html');r.end(html);});
  await new Promise(r=>server.listen(0,'127.0.0.1',r));
  const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium',args:['--no-sandbox']});
  const context=await browser.newContext({viewport:{width:968,height:1000},acceptDownloads:true});
  const p=await context.newPage(),errors=[];p.on('pageerror',e=>errors.push(e.message));p.setDefaultTimeout(5000);
  await p.goto('http://127.0.0.1:'+server.address().port+'/brief.html');
  const result={};const wait=()=>p.waitForTimeout(100);
  const active=()=>p.evaluate(()=>({tag:document.activeElement.tagName,id:document.activeElement.id,key:document.activeElement.dataset.focusKey,visible:!!document.activeElement.getBoundingClientRect().height}));
  const openTd=async()=>{await p.locator('[data-open-project="td"]').click();await wait();};
  const openHost=async()=>{await p.locator('[data-open-cluster="fc-host"]').click();await wait();};
  const assertHeading=async()=>assert.equal((await active()).id,'view-title');
  result.notesInitially=await p.locator('#note-count').innerText();assert.equal(result.notesInitially,'1');
  result.notePreserved=(await p.locator('#report-notes').textContent()).includes('squash it into nothing');assert(result.notePreserved);
  const order=await p.locator('.project-node').evaluateAll(es=>es.map(e=>e.dataset.projectId));
  assert.deepEqual(order,['bfs','td','next','music','portal']);result.overviewOrder=order;
  assert((await p.locator('.mock-scope').innerText()).includes('Client Portal (clear)'));
  assert((await p.locator('#closed-git-summary').innerText()).includes('ALERT'));
  assert((await p.locator('#closed-git-summary').innerText()).includes('3 affected projects'));
  result.clearSelectionStillShowsElsewhereAlert=true;
  result.projectAges=await p.locator('.project-node .project-age').allTextContents();assert.equal(result.projectAges.length,5);
  const styles=await p.evaluate(()=>['alert','warning','caution','clear','unknown'].map(s=>{let e=document.querySelector('.project-row .sev-'+s);let c=getComputedStyle(e);return {s,text:e.textContent,color:c.color,weight:c.fontWeight};}));
  assert.notEqual(styles[1].color,styles[2].color);assert.notEqual(styles[1].text,styles[2].text);result.styles=styles;
  await p.locator('[data-expand="td"]').click();assert.equal(await p.locator('.preview-row').count(),0);
  await p.locator('[data-expand="td"]').click();assert.equal(await p.locator('.preview-row').count(),2);
  await p.locator('.preview-row[data-open-cluster="fc-palette"]').click();await assertHeading();assert((await p.locator('#view-title').innerText()).includes('palette'));
  await p.keyboard.press('Escape');await wait();assert.equal((await active()).key,'preview:fc-palette');result.previewShortcutRestoresSelection=true;
  await openTd();await assertHeading();
  await p.locator('[data-tab="checkouts"]').click();assert.equal(await p.locator('[data-panel="checkouts"] tbody tr').count(),7);assert((await p.locator('[data-panel="checkouts"]').innerText()).includes('+12 −4'));
  await p.locator('[data-tab="clusters"]').click();await openHost();await assertHeading();
  result.relations=await p.locator('.relations tbody tr').allTextContents();assert.equal(result.relations.length,3);
  for(const [i,word]of [[0,'feature/render'],[1,'feature/host'],[2,'feature/cli']])assert(result.relations[i].includes(word));
  assert(result.relations.some(r=>r.includes('B, C')&&r.includes('unstaged')));
  assert((await p.locator('.scope-note').innerText()).includes('Known-member paths complete · project graph incomplete'));
  await p.locator('[data-focus-pane="B"]').click();await wait();
  result.keyboard=[];
  for(const expected of ['cluster:fc-host','project:td','closed']){
    await p.keyboard.press('Escape');await wait();const a=await active();result.keyboard.push(a);
    assert(a.visible);assert.notEqual(a.tag,'BODY');
    if(expected==='closed'){assert.equal(a.id,'closed-git-summary');assert(await p.locator('#demo-content').isHidden());}
    else assert.equal(a.key,expected);
  }
  // Open/enter/back using only the keyboard, without a compensating focus call.
  await p.keyboard.press('Enter');await wait();await assertHeading();
  await p.keyboard.press('Tab');await p.keyboard.press('Tab');assert.equal((await active()).key,'project:td');await p.keyboard.press('Enter');await wait();await assertHeading();
  await p.keyboard.press('Tab');await p.keyboard.press('Tab');await p.keyboard.press('Tab');assert.equal((await active()).key,'cluster:fc-host');await p.keyboard.press('Enter');await wait();await assertHeading();
  await p.keyboard.press('Escape');await wait();assert.equal((await active()).key,'cluster:fc-host');
  result.keyboardOnlyEntryAndBack=true;
  await openHost();
  await p.locator('#fixture-known').check();assert((await p.locator('#demo-content').innerText()).includes('3 known agent sessions'));
  const host=await p.locator('.relations tbody tr').filter({hasText:'feature/host'}).innerText();assert(host.includes('session:host-02')&&host.includes('panes B, C'));
  result.knownSessionsNotPanes=true;
  result.targetChecks={};
  for(const [value,text]of Object.entries({not_checked:'Not checked',checked_none:'Checked · no conflict reported',conflict:'Checked · conflict reported',unavailable:'Unavailable · command timed out',stale:'Stale · previous result retained'})){
    await p.locator('#fixture-target').selectOption(value);const t=await p.locator('.evidence-locus').innerText();assert(t.includes(text));if(value==='checked_none'||value==='conflict')assert(t.includes('fixture-main-91')&&t.includes('fixture-head-host'));result.targetChecks[value]=t;
  }
  await p.locator('#fixture-target').selectOption('not_checked');await p.locator('#fixture-known').uncheck();
  await p.locator('#fixture-mode').selectOption('bridge');
  assert((await p.locator('#view-title').innerText()).includes('connected'));assert((await p.locator('.topology-change').innerText()).includes('Warning remains'));
  assert((await p.locator('.scope-note').innerText()).includes('project graph complete'));
  let projection=JSON.parse(await p.locator('#demo-content pre').textContent());
  assert.deepEqual(projection.supersedes,['fc-host','fc-palette']);assert.equal(projection.dimensions.shared_file,2);assert.equal(projection.dimensions.shared_checkout,1);
  result.bridgeProjection=projection;
  assert((await p.locator('#demo-content .project-summary .status').innerText()).includes('WARNING'));
  await p.locator('#fixture-mode').selectOption('dense');
  const hotspots=new Set();
  for(let page=0;page<3;page++){
    const rows=await p.locator('.hotspot-row code').allTextContents();assert(rows.length<=5);rows.forEach(r=>hotspots.add(r));
    if(page<2)await p.locator('[data-focus-key="hotspots-next"]').click();
  }
  assert.equal(hotspots.size,11);assert(hotspots.has('app/src/common.rs'));result.denseHotspots=[...hotspots];
  const contexts=new Set();
  for(let page=0;page<4;page++){
    const rows=await p.locator('.relations tbody tr td:nth-child(2)>b').allTextContents();assert(rows.length<=10);rows.forEach(r=>contexts.add(r));
    if(page<3)await p.locator('[data-focus-key="relations-next"]').click();
  }
  assert.equal(contexts.size,40);result.denseContexts=contexts.size;
  // Restore the small fixture for visual checks.
  await p.locator('#fixture-mode').selectOption('missing');
  await p.locator('[data-crumb="all"]').click();
  result.layout={};
  for(const width of [390,720,968,1100,1600]){
    await p.setViewportSize({width,height:1000});
    let overflow=await p.evaluate(()=>document.documentElement.scrollWidth>document.documentElement.clientWidth);assert(!overflow);result.layout[width]={overview:!overflow};
    await openTd();await openHost();
    overflow=await p.evaluate(()=>document.documentElement.scrollWidth>document.documentElement.clientWidth);assert(!overflow);result.layout[width].cluster=!overflow;
    if(width===968)await p.locator('#demo').screenshot({path:'/tmp/git-rail-v2-relationships.png'});
    await p.locator('[data-crumb="all"]').click();
    if(width===390)await p.locator('#demo').screenshot({path:'/tmp/git-rail-v2-mobile.png'});
  }
  await p.setViewportSize({width:968,height:1000});await p.evaluate(()=>scrollTo(0,0));await p.screenshot({path:'/tmp/git-rail-v2-top.png'});
  await p.locator('#demo').screenshot({path:'/tmp/git-rail-v2-overview.png'});
  const coverage=await p.evaluate(()=>({
    notes:['figure','.grill .ask'].map(s=>[s,document.querySelectorAll(s).length,document.querySelectorAll(s+'.notable').length]),
    badAnchors:[...document.querySelectorAll('.notable')].filter(e=>e.id!==e.dataset.nid).map(e=>[e.id,e.dataset.nid]),
    svgs:[...document.querySelectorAll('svg:not([aria-hidden="true"])')].map(s=>!!s.getAttribute('viewBox')&&!s.getAttribute('width')&&!!s.getAttribute('aria-label')),
    figures:[...document.querySelectorAll('figure .lbl')].map(e=>e.textContent.slice(0,2)),
    external:document.querySelectorAll('script[src],link[href],img[src]:not([src^="data:"])').length,
    wallpaper:getComputedStyle(document.documentElement).getPropertyValue('--wall-label').trim()
  }));
  assert(coverage.notes.every(([,n,t])=>n===t));assert.equal(coverage.badAnchors.length,0);assert(coverage.svgs.every(Boolean));assert.equal(coverage.external,0);assert(coverage.wallpaper);result.coverage=coverage;
  for(const id of ['d-evidence','d-contract']){
    await p.locator(`[data-dlg="${id}"]`).first().click();assert(await p.locator('#'+id).isVisible());await p.keyboard.press('Escape');assert(await p.locator('#'+id).isHidden());
  }
  await p.locator('.ask').first().locator('.note-btn').click();await p.locator('#note-text').fill('Revision verification note.');await p.locator('[data-note-action="add"]').click();await p.locator('#d-note [data-close]').last().click();
  await p.locator('.ask').nth(1).locator('.concur-zone').click();await p.locator('#btn-export').click();result.noteMap=await p.locator('#export-out').inputValue();assert(result.noteMap.includes('squash it into nothing'));assert(result.noteMap.includes('Revision verification note.'));assert(result.noteMap.includes('✓ concur'));await p.locator('#d-export [data-close]').last().click();
  const dl=p.waitForEvent('download');await p.locator('#btn-embed').click();const file=await dl;await file.saveAs('/tmp/git-rail-v2-saved.html');
  html=fs.readFileSync('/tmp/git-rail-v2-saved.html','utf8');await p.evaluate(()=>localStorage.clear());await p.goto('http://127.0.0.1:'+server.address().port+'/saved.html');
  assert.equal(await p.locator('#note-count').innerText(),'2');assert.equal(await p.locator('#concur-count').innerText(),'1');result.savedReopened=true;
  // No native focus injection before repeating the reproduction on saved HTML.
  await openTd();await openHost();await p.locator('[data-focus-pane="B"]').click();await p.keyboard.press('Escape');await wait();assert.equal((await active()).key,'cluster:fc-host');await p.keyboard.press('Escape');await wait();assert.equal((await active()).key,'project:td');result.savedKeyboard=true;
  assert.equal(errors.length,0);result.errors=errors;
  fs.writeFileSync(root+'/docs/plans/git-coordination-rail/verification-v2.json',JSON.stringify(result,null,2));
  console.log(JSON.stringify(result,null,2));await browser.close();server.close();
})().catch(e=>{console.error(e);process.exit(1)});
