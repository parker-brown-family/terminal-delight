import os, re, collections, sys
def read(p):
    try: return open(p).read()
    except Exception: return None
procs={}
for d in os.listdir('/proc'):
    if not d.isdigit(): continue
    st=read(f'/proc/{d}/stat')
    if not st: continue
    m=re.match(r'(\d+) \((.*)\) (\S) (\d+)',st)
    if not m: continue
    pid,comm,state,ppid=int(m[1]),m[2],m[3],int(m[4])
    cmd=(read(f'/proc/{d}/cmdline') or '').replace('\0',' ').strip()
    sm=read(f'/proc/{d}/smaps_rollup') or ''
    def kb(k):
        mm=re.search(rf'^{k}:\s+(\d+)',sm,re.M); return int(mm[1]) if mm else 0
    procs[pid]=dict(pid=pid,comm=comm,ppid=ppid,cmd=cmd,rss=kb('Rss'),pss=kb('Pss'),swap=kb('Swap'),swappss=kb('SwapPss'),anon=kb('Anonymous'))
kids=collections.defaultdict(list)
for p in procs.values(): kids[p['ppid']].append(p['pid'])
def tree(pid):
    out=[pid]
    for k in kids[pid]: out+=tree(k)
    return out
def label(cmd):
    c=cmd
    for pat,lab in [('lean-ctx','lean-ctx'),('context-delight','context-delight'),('garrison','garrison'),('gmail','gmail'),('workspace-mcp','gmail'),('terminal-delight','td-mcp'),('chrome-native','chrome-mcp'),('claude-in-chrome','chrome-mcp'),('apes','apes'),('playwright','playwright'),('npx','npx'),('uvx','uvx'),('python','python'),('node','node'),('bash','bash'),('codex','codex'),('claude','claude')]:
        if pat in c: return lab
    return c.split(' ')[0].split('/')[-1][:20]
agents=[p for p in procs.values() if p['comm'] in ('claude','codex') and procs.get(p['ppid'],{}).get('comm') in ('bash','zsh','fish','sh')]
tot=collections.Counter()
rows=[]
for a in agents:
    t=tree(a['pid'])
    pss=sum(procs[x]['pss'] for x in t); sw=sum(procs[x]['swappss'] for x in t)
    own_pss=a['pss']; own_sw=a['swappss']
    kids_l=collections.Counter()
    for x in t[1:]:
        kids_l[label(procs[x]['cmd'])]+= procs[x]['pss']+procs[x]['swappss']
    rows.append((pss+sw,a,len(t)-1,own_pss,own_sw,pss-own_pss,sw-own_sw,kids_l))
rows.sort(key=lambda r:-r[0])
print(f"{'pid':>8} {'kind':6} {'n':>3} {'self_pss':>9} {'self_swp':>9} {'kids_pss':>9} {'kids_swp':>9} {'total':>8}  children")
T=collections.Counter()
for total,a,n,op,osw,kp,ksw,kl in rows:
    T['self_pss']+=op;T['self_swp']+=osw;T['kids_pss']+=kp;T['kids_swp']+=ksw
    print(f"{a['pid']:>8} {a['comm']:6} {n:>3} {op//1024:>8}M {osw//1024:>8}M {kp//1024:>8}M {ksw//1024:>8}M {total//1024:>7}M  "+', '.join(f"{k}:{v//1024}M" for k,v in kl.most_common(6)))
print('TOTAL', {k:f"{v//1024}M" for k,v in T.items()}, 'agents',len(rows), 'sum', (sum(T.values()))//1024,'M')
