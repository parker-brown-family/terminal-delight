import sys,time,json,os,subprocess
from pathlib import Path
pids=list(map(int,sys.argv[1:5]));condition,out=sys.argv[5:]
def snap(pid):
 p=Path('/proc')/str(pid);s=(p/'stat').read_text().rsplit(')',1)[1].split()
 return {'ticks':int(s[11])+int(s[12]),'start':int(s[19])}
def geometry():
 clients=json.loads(subprocess.check_output(['hyprctl','clients','-j']))
 selected={c['pid']:{k:c[k] for k in ['size','monitor','workspace','floating']} for c in clients if c['pid'] in [pids[0],pids[2]]}
 assert len(selected)==2
 a,b=selected[pids[0]],selected[pids[2]]
 assert a==b and a['size']==[1000,700] and a['floating'],selected
 return selected
g=geometry()
a=[snap(p) for p in pids];start=time.monotonic();time.sleep(8);b=[snap(p) for p in pids];elapsed=time.monotonic()-start
assert geometry()==g
r={'geometry':g,'condition':condition,'elapsed_s':elapsed,'pids':pids,'before':a,'after':b,'percent_one_cpu':[],'pss_kib':[]}
for pid,x,y in zip(pids,a,b):
 if x['start']!=y['start']:raise RuntimeError('PID changed')
 r['percent_one_cpu'].append(100*(y['ticks']-x['ticks'])/os.sysconf('SC_CLK_TCK')/elapsed)
 try:r['pss_kib'].append(int(next(l.split()[1] for l in (Path('/proc')/str(pid)/'smaps_rollup').read_text().splitlines() if l.startswith('Pss:'))))
 except (OSError,StopIteration):r['pss_kib'].append(None)
with Path(out).open('a') as f:f.write(json.dumps(r)+'\n')
print(json.dumps(r),flush=True)
