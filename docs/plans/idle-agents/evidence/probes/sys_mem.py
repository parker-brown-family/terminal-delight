import os,re,collections
def rd(p):
    try: return open(p).read()
    except: return ''
C=collections.Counter(); S=collections.Counter(); N=collections.Counter()
for d in os.listdir('/proc'):
    if not d.isdigit(): continue
    sm=rd(f'/proc/{d}/smaps_rollup')
    if not sm: continue
    pss=int((re.search(r'^Pss:\s+(\d+)',sm,re.M) or [0,0])[1]); sw=int((re.search(r'^SwapPss:\s+(\d+)',sm,re.M) or [0,0])[1])
    cmd=rd(f'/proc/{d}/cmdline').split('\0')[0]
    comm=rd(f'/proc/{d}/comm').strip()
    k=os.path.basename(cmd) or comm
    if 'chatgpt' in cmd.lower() or comm=='ChatGPT': k='ChatGPT desktop (+its codex/lean-ctx)'
    elif 'chrome' in cmd and 'chromium' not in cmd: k='chrome'
    elif 'chromium' in cmd: k='chromium'
    elif k.startswith('td-') or k=='terminal-delight': k='terminal-delight '+('mcp relay' if 'mcp' in rd(f'/proc/{d}/cmdline') else 'window/host/other')
    C[k]+=pss; S[k]+=sw; N[k]+=1
rows=sorted(C, key=lambda k:-(C[k]+S[k]))[:22]
tot_p=sum(C.values()); tot_s=sum(S.values())
print(f"{'group':45} {'n':>4} {'resident':>9} {'swapped':>9} {'swap@2.6:1':>10} {'~RAM':>7}")
for k in rows: print(f"{k[:45]:45} {N[k]:>4} {C[k]//1024:>8}M {S[k]//1024:>8}M {int(S[k]/2.6)//1024:>9}M {(C[k]+int(S[k]/2.6))//1024:>6}M")
print(f"{'ALL':45} {sum(N.values()):>4} {tot_p//1024:>8}M {tot_s//1024:>8}M")
