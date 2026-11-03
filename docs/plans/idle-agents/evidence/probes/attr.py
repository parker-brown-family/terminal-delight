import os,re,collections
def rd(p):
    try: return open(p).read()
    except: return ''
P={}
for d in os.listdir('/proc'):
    if not d.isdigit(): continue
    st=rd(f'/proc/{d}/stat')
    if not st: continue
    r=st[st.rfind(')')+2:].split(); sm=rd(f'/proc/{d}/smaps_rollup')
    g=lambda k:int((re.search(rf'^{k}:\s+(\d+)',sm,re.M) or [0,0])[1])
    P[int(d)]=dict(ppid=int(r[1]),comm=rd(f'/proc/{d}/comm').strip(),cmd=rd(f'/proc/{d}/cmdline').replace('\0',' ')[:110],pss=g('Pss'),sw=g('SwapPss'))
def owner(pid):
    seen=0
    p=P[pid]['ppid']
    while p in P and seen<30:
        c=P[p]['comm']
        if c in('claude','codex'): return f"{c} agent"
        if c=='ChatGPT' or 'chatgpt' in P[p]['cmd']: return 'ChatGPT desktop'
        if 'app-server' in P[p]['cmd']: return 'codex app-server daemon'
        p=P[p]['ppid']; seen+=1
    return 'not under an agent'
C=collections.defaultdict(lambda:[0,0,0])
for pid,p in P.items():
    if p['comm'] in ('python','python3','bun','node','lean-ctx','uv','uvx','npm','npx','MainThread','node-MainThread') or 'mcp' in p['cmd']:
        o=owner(pid); k=(o, re.sub(r'/home/parker/','~/',p['cmd'])[:70])
        C[k][0]+=1;C[k][1]+=p['pss'];C[k][2]+=p['sw']
rows=sorted(C.items(), key=lambda kv:-(kv[1][1]+kv[1][2]))[:30]
for (o,cmd),(n,pss,sw) in rows: print(f"{o:24} n={n:<3} res={pss//1024:>5}M swp={sw//1024:>5}M  {cmd}")
