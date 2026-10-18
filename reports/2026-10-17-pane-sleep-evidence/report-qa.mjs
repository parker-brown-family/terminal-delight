// Adapted from the Terminal State report's existing dependency-free CDP check.
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {readFileSync,writeFileSync,mkdtempSync,rmSync,readdirSync,mkdirSync} from 'node:fs';
import {resolve,dirname,basename} from 'node:path';
const path=resolve(process.argv[2]);
const out=resolve(process.argv[3]);
mkdirSync(out,{recursive:true});
const downloads=mkdtempSync('/tmp/td-sleep-downloads-');
const server=createServer((req,res)=>{res.setHeader('Content-Type','text/html');res.end(readFileSync(path));});
await new Promise(r=>server.listen(0,'127.0.0.1',r));
const profile=mkdtempSync('/tmp/td-sleep-browser-');
const browser=spawn('/usr/bin/chromium',['--headless','--no-sandbox','--disable-gpu','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank'],{stdio:['ignore','ignore','pipe']});
let ws;
try {
 const url=await new Promise((resolve,reject)=>{let s='';browser.stderr.on('data',d=>{s+=d;const m=s.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m)resolve(m[1])});browser.on('exit',()=>reject(Error('browser exited')));setTimeout(()=>reject(Error('browser startup deadline')),15000).unref();});
 ws=new WebSocket(url);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
 let id=0;const pending=new Map();
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(m.error):p.resolve(m.result)}});
 const call=(method,params={},sessionId)=>new Promise((resolve,reject)=>{const n=++id;pending.set(n,{resolve,reject});ws.send(JSON.stringify({id:n,method,params,sessionId}))});
 const {targetId}=await call('Target.createTarget',{url:'about:blank'});
 const {sessionId}=await call('Target.attachToTarget',{targetId,flatten:true});
 const c=(m,p)=>call(m,p,sessionId);
 const evaluate=async expression=>{const r=await c('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(JSON.stringify(r.exceptionDetails));return r.result.value};
 await c('Page.enable');await c('Runtime.enable');
 await call('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:downloads});
 const exceptions=[];
 ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.method==='Runtime.exceptionThrown')exceptions.push(m.params);});
 const results=[];
 for(const width of [1100,720,390]){
  await c('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});
  await c('Page.navigate',{url:'http://127.0.0.1:'+server.address().port+'/'});
  let ready=false;
  for(let n=0;n<50;n++){if(await evaluate("document.querySelectorAll('figure.notable').length===4")){ready=true;break;}await new Promise(r=>setTimeout(r,100));}
  if(!ready)throw Error('notes initialization did not finish '+JSON.stringify({exceptions,dom:await evaluate("({figures:document.querySelectorAll('figure').length,noted:document.querySelectorAll('figure.notable').length,scripts:document.scripts.length,body:document.body.innerText.slice(0,150)})")}));
  const result=await evaluate("(()=>{const figures=[...document.querySelectorAll('figure')];const errors=[];if(document.documentElement.scrollWidth>innerWidth)errors.push('page overflow');if(figures.length!==4||figures.some(f=>!f.classList.contains('notable')||!f.id||!f.querySelector('.note-btn')))errors.push('figure note coverage');if(!document.querySelector('.ask.notable .concur-zone'))errors.push('decision concur missing');if([...document.querySelectorAll('figure > .scroller > svg')].some(s=>!s.getAttribute('viewBox')||!s.getAttribute('aria-label')))errors.push('SVG accessibility');document.querySelector('[data-open=d-evidence]').click();if(!document.getElementById('d-evidence').open)errors.push('evidence dialog');document.querySelector('#d-evidence [data-close]').click();return {width:innerWidth,scrollWidth:document.documentElement.scrollWidth,figures:figures.length,notable:document.querySelectorAll('.notable').length,errors};})()");
  if(result.errors.length)throw Error(JSON.stringify(result));
  results.push(result);
  const {data}=await c('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});
  writeFileSync(out+'/report-'+width+'.png',Buffer.from(data,'base64'));
 }
 const notes=await evaluate("(()=>{const figure=document.querySelector('figure');figure.querySelector('.note-btn').click();document.getElementById('note-text').value='QA: measured CPU note';document.querySelector('[data-note-action=add]').click();document.querySelector('#d-note [data-close]').click();document.querySelector('.ask .concur-zone').click();document.getElementById('btn-export').click();const map=document.getElementById('export-out').value;document.querySelector('#d-export [data-close]').click();document.getElementById('btn-embed').click();return {map,anchor:figure.id};})()");
 if(!notes.map.includes('QA: measured CPU note')||!notes.map.includes('✓ concur')||!notes.map.includes('['+notes.anchor+']'))throw Error('notes/concur export failed');
 let saved;
 for(let i=0;i<50;i++){saved=readdirSync(downloads).find(f=>f.endsWith('.html'));if(saved)break;await new Promise(r=>setTimeout(r,100));}
 if(!saved)throw Error('annotated HTML did not download');
 const savedText=readFileSync(downloads+'/'+saved,'utf8');
 if(!savedText.includes('QA: measured CPU note')||!savedText.includes('report-concurs'))throw Error('saved HTML lost notes');
 await c('Page.navigate',{url:'file://'+downloads+'/'+saved});
 for(let i=0;i<50;i++){if(await evaluate("document.querySelectorAll('figure.notable').length===4"))break;await new Promise(r=>setTimeout(r,100));}
 const reopened=await evaluate("({notes:document.getElementById('note-count').textContent,concurs:JSON.parse(document.getElementById('report-concurs').textContent)})");
 if(reopened.notes!=='1'||Object.values(reopened.concurs).filter(Boolean).length!==1)throw Error('reopened saved notes/concur lost '+JSON.stringify(reopened));
 if(exceptions.length)throw Error('page exceptions '+JSON.stringify(exceptions));
 results.push({notes,save_and_reopen:reopened,page_exceptions:exceptions});
 writeFileSync(out+'/report-qa.json',JSON.stringify(results,null,2));
 console.log(JSON.stringify(results));
} finally {
 if(ws)ws.close();server.closeAllConnections();server.close();browser.kill('SIGTERM');if(browser.exitCode===null)await new Promise(r=>{browser.once('exit',r);setTimeout(r,2000)});rmSync(profile,{recursive:true,force:true});rmSync(downloads,{recursive:true,force:true});
}
