import os,re,time,sys
def stat(p):
    s=open(f'/proc/{p}/stat').read(); r=s[s.rfind(')')+2:].split()
    # r[0]=state; minflt=r[7], majflt=r[9], utime=r[11], stime=r[12]
    return int(r[7]),int(r[9]),int(r[11])+int(r[12])
def roll(p):
    d={}
    for l in open(f'/proc/{p}/smaps_rollup'):
        k,_,v=l.partition(':'); v=v.split()
        if v and v[0].isdigit(): d[k]=int(v[0])
    return d
pids=[int(x) for x in sys.argv[2:]]; dur=int(sys.argv[1])
a={p:(stat(p),roll(p)) for p in pids}
time.sleep(dur)
hz=os.sysconf('SC_CLK_TCK')
print(f"{'pid':>8} {'comm':7} {'cpu_s':>6} {'minflt':>8} {'majflt':>7} {'dRss_MB':>8} {'dSwap_MB':>8} {'Rss':>6} {'Swap':>6}")
for p in pids:
    try:
        (m0,M0,c0),r0=a[p]; (m1,M1,c1)=stat(p); r1=roll(p)
        print(f"{p:>8} {open(f'/proc/{p}/comm').read().strip():7} {(c1-c0)/hz:>6.2f} {m1-m0:>8} {M1-M0:>7} {(r1['Rss']-r0['Rss'])/1024:>8.1f} {(r1['Swap']-r0['Swap'])/1024:>8.1f} {r1['Rss']//1024:>6} {r1['Swap']//1024:>6}")
    except Exception as e: print(p,'gone',e)
