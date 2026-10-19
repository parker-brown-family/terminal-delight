import sys,time,json,os
from pathlib import Path
pids=list(map(int,sys.argv[1:5]));condition,out=sys.argv[5:]
def snap(pid):
 p=Path('/proc')/str(pid);s=(p/'stat').read_text().rsplit(')',1)[1].split()
 return {'ticks':int(s[11])+int(s[12]),'start':int(s[19])}
a=[snap(p) for p in pids];start=time.monotonic();time.sleep(8);b=[snap(p) for p in pids];elapsed=time.monotonic()-start
r={'condition':condition,'elapsed_s':elapsed,'pids':pids,'before':a,'after':b,'percent_one_cpu':[],'pss_kib':[]}
for pid,x,y in zip(pids,a,b):
 if x['start']!=y['start']:raise RuntimeError('PID changed')
 r['percent_one_cpu'].append(100*(y['ticks']-x['ticks'])/os.sysconf('SC_CLK_TCK')/elapsed)
 try:r['pss_kib'].append(int(next(l.split()[1] for l in (Path('/proc')/str(pid)/'smaps_rollup').read_text().splitlines() if l.startswith('Pss:'))))
 except (OSError,StopIteration):r['pss_kib'].append(None)
with Path(out).open('a') as f:f.write(json.dumps(r)+'\n')
print(json.dumps(r),flush=True)
