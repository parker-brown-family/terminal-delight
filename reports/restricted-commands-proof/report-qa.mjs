import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
const report=process.argv[2],out=process.argv[3];
if(!report||!out)throw new Error('Pass report and fresh output directory');
await fs.mkdir(out,{recursive:false});await fs.mkdir(out+'/downloads');
const child=spawn('/usr/bin/chromium',['--headless=new','--disable-gpu','--remote-debugging-port=0',`--user-data-dir=${out}/profile`,'about:blank'],{stdio:['ignore','ignore','pipe']});
let log='';const endpoint=await new Promise((resolve,reject)=>{child.stderr.on('data',b=>{log+=b;const m=log.match(/DevTools listening on (ws:\/\/[^\s]+)/);if(m)resolve(m[1]);});child.on('exit',c=>reject(new Error('browser exited '+c+': '+log)));});
const port=new URL(endpoint).port,pages=await(await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws=new WebSocket(pages.find(p=>p.type==='page').webSocketDebuggerUrl);await new Promise(r=>ws.addEventListener('open',r));
let seq=0;const pending=new Map(),errors=[],requests=[];
const call=(method,params={})=>new Promise((resolve,reject)=>{const id=++seq;pending.set(id,{resolve,reject});ws.send(JSON.stringify({id,method,params}));});
ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.reject(new Error(JSON.stringify(m.error))):p.resolve(m.result);}else if(m.method==='Runtime.exceptionThrown')errors.push(m.params);else if(m.method==='Network.requestWillBeSent')requests.push(m.params.request.url);});
const ev=async expression=>(await call('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true})).result.value;
const assert=(v,msg)=>{if(!v)throw new Error(msg);};
const pause=ms=>new Promise(r=>setTimeout(r,ms));
try{
 await call('Page.enable');await call('Runtime.enable');await call('Network.enable');await call('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:out+'/downloads'});
 await call('Page.navigate',{url:'file://'+report});
 for(let i=0;i<100;i++){if(await ev(`document.readyState==='complete'&&document.querySelectorAll('figure.notable').length>0`))break;await pause(100);}
 const initialNotes=Number(await ev(`document.querySelector('#note-count').textContent`));
 const originalNotes=await ev(`document.querySelector('#report-notes').textContent`);
 const layouts=[];
 for(const width of [390,968,1100]){
  await call('Emulation.setDeviceMetricsOverride',{width,height:1000,deviceScaleFactor:1,mobile:false});await pause(100);
  const layout=await ev(`({width:innerWidth,scrollWidth:document.documentElement.scrollWidth,figures:document.querySelectorAll('figure').length,annotated:document.querySelectorAll('figure.notable').length,svg:[...document.querySelectorAll('main figure svg')].every(s=>s.hasAttribute('viewBox')&&!s.hasAttribute('width')&&s.hasAttribute('aria-label')&&s.getBoundingClientRect().height>0&&getComputedStyle(s).display!=='none'),external:[...document.querySelectorAll('[src],link[href]')].map(e=>e.src||e.href).filter(x=>/^https?:/.test(x))})`);
  assert(layout.scrollWidth<=width,'horizontal overflow at '+width);assert(layout.figures>0&&layout.annotated===layout.figures,'annotation coverage');if(!layout.svg)console.log(await ev(`JSON.stringify([...document.querySelectorAll('svg')].map(s=>({cls:s.getAttribute('class'),height:s.getBoundingClientRect().height,display:getComputedStyle(s).display,viewBox:s.hasAttribute('viewBox'),width:s.hasAttribute('width'),aria:s.hasAttribute('aria-label')})))`));assert(layout.svg&&layout.external.length===0,'self-contained figures');layouts.push(layout);
  await ev("scrollTo({top:0,behavior:'instant'})");await pause(60);const shot=await call('Page.captureScreenshot',{format:'png'});await fs.writeFile(out+'/'+width+'.png',Buffer.from(shot.data,'base64'));
  for(const index of [3,5,6,7]){await ev(`document.querySelectorAll('figure')[${index}].scrollIntoView({block:'start',behavior:'instant'})`);await pause(60);const frame=await call('Page.captureScreenshot',{format:'png'});await fs.writeFile(out+'/'+width+'-figure-'+(index+1)+'.png',Buffer.from(frame.data,'base64'));}
 }
 await ev(`document.querySelector('figure .note-btn').click()`);assert(await ev(`document.querySelector('#d-note').open`),'note dialog did not open');
 await ev(`document.querySelector('#note-text').value='QA: review annotation persists';document.querySelector('[data-note-action="add"]').click();document.querySelector('#d-note').close();document.querySelector('[data-note-action="export"]').click()`);
 const map=await ev(`document.querySelector('#export-out').value`);assert(map.includes('QA: review annotation persists')&&map.includes('[fig-'),'anchored map missing');
 await ev(`document.querySelector('#d-export').close();document.querySelector('[data-note-action="embed"]').click()`);
 const savedName=await ev(`window.NOTES_FILE||location.pathname.split('/').pop()`);
 const saved=out+'/downloads/'+savedName;
 for(let i=0;i<60;i++){try{await fs.access(saved);break;}catch{await pause(100);}}
 const text=await fs.readFile(saved,'utf8');assert(text.includes('QA: review annotation persists')&&text.includes('READER NOTES'),'notes not embedded');
 await ev('localStorage.clear()');await call('Page.navigate',{url:'file://'+saved});
 for(let i=0;i<100;i++){if(await ev(`Number(document.querySelector('#note-count')?.textContent)===${initialNotes+1}`))break;await pause(100);}
 assert(await ev(`Number(document.querySelector('#note-count').textContent)===${initialNotes+1}`),'saved note not restored');
 for(const values of Object.values(JSON.parse(originalNotes)))for(const note of values)assert(text.includes(note.text),'original reader note was lost');
 assert(errors.length===0,'browser errors');assert(requests.every(x=>!/^https?:/.test(x)),'external network request');
 await fs.writeFile(out+'/result.json',JSON.stringify({passed:true,initialNotes,originalNotesPreserved:true,layouts,notes:'add / anchored export / embed / clear local state / reopen passed',errors,external_requests:requests.filter(x=>/^https?:/.test(x)),sandbox:'Chromium sandbox enabled'},null,2));
 console.log('Report responsive, self-contained, annotated and saved/reopened successfully');
}finally{await call('Browser.close').catch(()=>{});child.kill();}
