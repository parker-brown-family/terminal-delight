import json,sys,glob,statistics as st,collections,os
tag=sys.argv[1]; R=os.path.dirname(os.path.abspath(__file__))
g=collections.defaultdict(list); fails=[]
for f in sorted(glob.glob(f'{R}/{tag}/*/samples.json')):
    v,brief,_=os.path.basename(os.path.dirname(f)).split('-',2)[0], *os.path.basename(os.path.dirname(f)).split('-')[1:3]
    name=os.path.basename(os.path.dirname(f)); v=name.split('-')[0]; brief='-'.join(name.split('-')[1:3])
    for s in json.load(open(f)):
        g[(v,brief,'first' if s['cycle']==0 else 'warm')].append(s)
for d in glob.glob(f'{R}/{tag}/*/'):
    if not os.path.exists(d+'samples.json'): fails.append(d)
def m(rows,k): 
    v=[r['ms'][k] for r in rows if r['ms'].get(k) is not None]
    return st.median(v) if v else float('nan')
print(f"{'variant':6} {'brief':8} {'kind':5} {'n':>3} {'wall':>7} {'p90':>7} {'lookup':>7} {'layout':>7} {'tile':>7} {'decode':>7} {'paint':>7}  hits")
for k in sorted(g):
    rows=g[k]; walls=sorted(r['wall_ms'] for r in rows)
    print(f"{k[0]:6} {k[1]:8} {k[2]:5} {len(rows):3} {st.median(walls):7.0f} {walls[max(0,int(len(walls)*.9)-1)]:7.0f} {m(rows,'cache_lookup'):7.0f} {m(rows,'layout'):7.0f} {m(rows,'first_tile'):7.0f} {m(rows,'first_decode'):7.0f} {m(rows,'viewport_paint'):7.0f}  {sum(1 for r in rows if r['ms'].get('cache_hit'))}")
if fails: print('missing samples:',fails)
