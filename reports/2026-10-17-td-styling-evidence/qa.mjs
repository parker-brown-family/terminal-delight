import {spawn} from 'node:child_process';
import fs from 'node:fs';
const file=process.argv[2];
const proc=spawn('chromium',['--headless','--no-sandbox','--disable-gpu','--disable-dev-shm-usage','--remote-debugging-port=0','--user-data-dir=/tmp/td-efficiency-results-cdp','about:blank']);
const endpoint=await new Promise((resolve,reject)=>{let s='';proc.stderr.on('data',d=>{s+=d;const m=s.match(/DevTools listening on (ws:\/\/\S+)/);if(m)resolve(m[1])});setTimeout(()=>reject(Error('No CDP')),10000).unref()});
const ws=new WebSocket(endpoint);await new Promise(r=>ws.addEventListener('open',r,{once:true}));
let id=0;const pending=new Map();ws.addEventListener('message',e=>{const m=JSON.parse(e.data);if(m.id&&pending.has(m.id)){pending.get(m.id)(m);pending.delete(m.id)}});
const call=(method,params={},sessionId)=>new Promise(resolve=>{const i=++id;pending.set(i,resolve);ws.send(JSON.stringify({id:i,method,params,...(sessionId?{sessionId}:{})}))});
try{
 const t=await call('Target.createTarget',{url:'about:blank'});const a=await call('Target.attachToTarget',{targetId:t.result.targetId,flatten:true});const sid=a.result.sessionId;
 for(const width of [1100,720,390]){
  await call('Emulation.setDeviceMetricsOverride',{width,height:1300,deviceScaleFactor:1,mobile:false},sid);
  await call('Page.navigate',{url:'file://'+file+'?qa='+width},sid);
  await new Promise(r=>setTimeout(r,700));
  const expression=`(()=>{
   const result={width:innerWidth,clientWidth:document.documentElement.clientWidth,scrollWidth:document.documentElement.scrollWidth,overflow:document.documentElement.scrollWidth>document.documentElement.clientWidth+1,figures:document.querySelectorAll('figure').length,notable:document.querySelectorAll('.notable').length,notedFigures:document.querySelectorAll('figure.notable').length,concur:document.querySelectorAll('.concur-zone').length,labels:[...document.querySelectorAll('figure .lbl')].map(e=>parseInt(e.textContent)),anchors:[...document.querySelectorAll('.notable')].every(e=>e.id===e.dataset.nid),svg:[...document.querySelectorAll('svg')].every(x=>x.hasAttribute('viewBox')&&x.hasAttribute('aria-label')),pending:/VALIDATION_PENDING|PROVENANCE_LINKS/.test(document.body.innerText)};
   document.querySelector('figure .note-btn').click();document.getElementById('note-text').value='QA note on measured CPU';document.querySelector('[data-note-action="add"]').click();document.getElementById('d-note').close();
   document.querySelector('.concur-zone').click();document.getElementById('btn-export').click();result.map=document.getElementById('export-out').value;result.mapOk=result.map.includes('QA note on measured CPU')&&result.map.includes('concur');
   document.getElementById('d-export').close();document.querySelector('[data-open]').click();result.modal=document.getElementById('d-evidence').open;document.getElementById('d-evidence').close();localStorage.clear();return result;
  })()`;
  const q=await call('Runtime.evaluate',{expression,returnByValue:true},sid);
  if(q.result?.exceptionDetails)throw Error(JSON.stringify(q.result.exceptionDetails));
  const data=q.result.result.value;console.log(JSON.stringify(data));
  if(data.overflow||data.figures!==4||data.notedFigures!==4||!data.concur||!data.svg||data.pending||!data.mapOk||!data.anchors||!data.modal||data.labels.join(',')!=='1,2,3,4')throw Error('QA failed');
  await call('Runtime.evaluate',{expression:'window.scrollTo({top:0,behavior:"instant"})'},sid);
  const shot=await call('Page.captureScreenshot',{format:'png'},sid);fs.writeFileSync('/tmp/td-efficiency-results-'+width+'.png',Buffer.from(shot.result.data,'base64'));
 }
}finally{ws.close();proc.kill();}
