import {spawn} from 'node:child_process';
import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';

const file=path.resolve(process.argv[2]);
const out=path.resolve(process.argv[3]);
fs.mkdirSync(out,{recursive:true});
const profile=fs.mkdtempSync('/tmp/td-redraw-qa-');
const downloads=path.join(profile,'downloads');fs.mkdirSync(downloads);
let saved;
const server=http.createServer((req,res)=>{
  res.setHeader('Content-Type','text/html; charset=utf-8');
  res.end(fs.readFileSync(req.url.startsWith('/saved')&&saved?saved:file));
});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const base=`http://127.0.0.1:${server.address().port}`;
const proc=spawn('chromium',['--headless','--disable-gpu','--disable-dev-shm-usage','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank']);
let ws;
try {
  const endpoint=await new Promise((resolve,reject)=>{
    let text='';proc.stderr.on('data',d=>{text+=d;const m=text.match(/DevTools listening on (ws:\/\/\S+)/);if(m)resolve(m[1]);});
    proc.once('exit',code=>reject(Error(`Chromium exited ${code}: ${text.slice(-1500)}`)));
    setTimeout(()=>reject(Error('No CDP endpoint')),15000).unref();
  });
  ws=new WebSocket(endpoint);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
  let id=0;const pending=new Map();
  ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id&&pending.has(m.id)){const {resolve,reject,timer}=pending.get(m.id);clearTimeout(timer);pending.delete(m.id);m.error?reject(Error(JSON.stringify(m.error))):resolve(m);}});
  const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const i=++id;const timer=setTimeout(()=>{pending.delete(i);reject(Error(`CDP timeout: ${method}`));},20000);pending.set(i,{resolve,reject,timer});ws.send(JSON.stringify({id:i,method,params,...(sessionId?{sessionId}:{})}));});
  const target=await call('Target.createTarget',{url:'about:blank'});
  const attach=await call('Target.attachToTarget',{targetId:target.result.targetId,flatten:true});
  const sid=attach.result.sessionId;
  const evaluate=async expression=>{const r=await call('Runtime.evaluate',{expression,returnByValue:true},sid);if(r.result.exceptionDetails)throw Error(JSON.stringify(r.result.exceptionDetails));return r.result.result.value;};
  await call('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:downloads});
  const receipts=[];
  for(const width of [1100,720,390]) {
    await call('Emulation.setDeviceMetricsOverride',{width,height:1300,deviceScaleFactor:1,mobile:false},sid);
    await call('Page.navigate',{url:base+'/brief.html?width='+width},sid);
    await new Promise(r=>setTimeout(r,600));
    const data=await evaluate(`(()=>{
      const selectors=['figure','.grill .ask','table.wide tbody tr'];
      return {width:innerWidth,overflow:document.documentElement.scrollWidth>document.documentElement.clientWidth+1,
        figures:document.querySelectorAll('figure').length,
        coverage:selectors.map(s=>[s,document.querySelectorAll(s).length,[...document.querySelectorAll(s)].filter(e=>e.classList.contains('notable')).length]),
        anchors:[...document.querySelectorAll('.notable')].every(e=>e.id===e.dataset.nid),
        labels:[...document.querySelectorAll('figure .lbl')].map(e=>parseInt(e.textContent)),
        wallpaper:getComputedStyle(document.documentElement).getPropertyValue('--wall-label'),
        missingModals:[...document.querySelectorAll('[data-open]')].filter(b=>!document.getElementById(b.dataset.open)).length,
        tableNotes:[...document.querySelectorAll('tr.notable .note-btn')].every(b=>b.parentElement.tagName==='TD'),
        placeholders:/@@[A-Z_]+@@/.test(document.body.innerText)};
    })()`);
    if(data.overflow||data.figures!==7||!data.anchors||!data.tableNotes||data.placeholders||!data.wallpaper||data.missingModals||data.labels.join(',')!=='1,2,3,4,5,6,7'||data.coverage.some(([,n,k])=>n!==k||n===0))throw Error(JSON.stringify(data));
    data.notes=await evaluate(`(()=>{
      for(const selector of ['figure .note-btn','.grill .ask .note-btn']){
        document.querySelector(selector).click();document.getElementById('note-text').value='QA saved note '+selector;
        document.querySelector('[data-note-action="add"]').click();document.getElementById('d-note').close();
      }
      document.querySelector('.concur-zone').click();document.getElementById('btn-export').click();
      const map=document.getElementById('export-out').value;document.getElementById('d-export').close();
      document.querySelector('[data-open]').click();const modal=document.getElementById('d-evidence').open;document.getElementById('d-evidence').close();
      return {map,modal,concur:map.includes('concur'),figure:map.includes('fig-01-cpu-for'),ask:map.includes('ask-keep-the-measured-reuse')};
    })()`);
    if(!data.notes.modal||!data.notes.concur||!data.notes.figure||!data.notes.ask)throw Error('Notes/export failed');
    if(width===1100){
      await evaluate(`document.getElementById('btn-embed').click()`);
      for(let i=0;i<60;i++){const names=fs.readdirSync(downloads).filter(n=>n.endsWith('.html'));if(names.length){saved=path.join(downloads,names[0]);break;}await new Promise(r=>setTimeout(r,100));}
      if(!saved)throw Error('No saved annotated HTML');
      const html=fs.readFileSync(saved,'utf8');
      for(const name of ['report-notes','report-concurs'])if((html.match(new RegExp('id="'+name+'"','g'))||[]).length!==1)throw Error('Missing/duplicate note island');
      await evaluate('localStorage.clear()');
      await call('Page.navigate',{url:base+'/saved.html'},sid);await new Promise(r=>setTimeout(r,500));
      data.saved=await evaluate(`(()=>{document.getElementById('btn-export').click();const map=document.getElementById('export-out').value;document.getElementById('d-export').close();return map.includes('QA saved note')&&map.includes('concur')})()`);
      if(!data.saved)throw Error('Saved notes did not survive with empty localStorage');
      await evaluate('localStorage.clear()');
      await call('Page.navigate',{url:base+'/brief.html'},sid);await new Promise(r=>setTimeout(r,500));
    }
    await evaluate('localStorage.clear()');
    await call('Page.navigate',{url:base+'/brief.html?clean='+width},sid);
    await new Promise(r=>setTimeout(r,500));
    await evaluate('window.scrollTo({top:0,behavior:"instant"})');
    const top=await call('Page.captureScreenshot',{format:'png'},sid);fs.writeFileSync(path.join(out,`brief-${width}.png`),Buffer.from(top.result.data,'base64'));
    if(width===1100){await evaluate(`document.getElementById('tradeoffs').scrollIntoView({behavior:'instant'})`);const shot=await call('Page.captureScreenshot',{format:'png'},sid);fs.writeFileSync(path.join(out,'tradeoffs-1100.png'),Buffer.from(shot.result.data,'base64'));}
    receipts.push(data);
  }
  fs.writeFileSync(path.join(out,'qa.json'),JSON.stringify(receipts,null,2)+'\n');
  console.log(JSON.stringify(receipts.map(r=>({width:r.width,coverage:r.coverage,overflow:r.overflow,saved:r.saved,notes:!!r.notes.concur}))));
} finally {
  ws?.close();proc.kill();await new Promise(r=>{if(proc.exitCode!==null)r();else{proc.once('exit',r);setTimeout(r,5000).unref();}});server.close();fs.rmSync(profile,{recursive:true,force:true});
}
