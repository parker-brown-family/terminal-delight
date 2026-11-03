#!/usr/bin/env python3
"""Owned native fixture and synthetic MCP attackers; never calls a model."""
import argparse, ctypes, hashlib, json, os, shlex, socket, subprocess, sys, time
from pathlib import Path

def save(path, value):
    tmp = path.with_suffix('.tmp')
    tmp.write_text(json.dumps(value, indent=2)); tmp.replace(path)

def wait(fn, seconds=30):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        result = fn()
        if result: return result
        time.sleep(.1)
    raise TimeoutError(str(fn))

def raw(window, line):
    path = Path(os.environ.get('XDG_RUNTIME_DIR', '/run/user/1000')) / 'terminal-delight' / f'ctl-{window}.sock'
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(12); s.connect(str(path)); s.sendall((line + '\n').encode())
        return s.makefile().readline().strip()

def packet(name, args=None):
    return {'jsonrpc':'2.0','id':1,'method':'tools/call','params':{'name':name,'arguments':args or {}}}

def actor(root, label, binary):
    ctypes.CDLL(None).prctl(15, b'claude', 0, 0, 0)
    save(root / f'{label}-identity.json', {'pid':os.getpid(),'shell_pid':os.getppid(),'pane':int(os.environ['TD_PANE_ID']),'session':os.environ['TD_SESSION']})
    print(f'\033]0;MOCK {label}\007QA_CANARY_{label}', flush=True)
    seen = set()
    while not (root / 'stop').exists():
        for path in sorted(root.glob(f'{label}-request-*.json')):
            if path.name in seen: continue
            seen.add(path.name); req = json.loads(path.read_text())
            try:
                if 'raw' in req: reply = {'raw':raw(req['window'], req['raw'])}
                else:
                    env = dict(os.environ); env.update(req.get('env', {}))
                    r = subprocess.run([binary,'mcp'], input=json.dumps(req['rpc'])+'\n', capture_output=True, text=True, timeout=15, env=env)
                    reply = {'exit':r.returncode,'stdout':r.stdout,'stderr':r.stderr}
                    if r.stdout.strip(): reply['rpc'] = json.loads(r.stdout)
            except Exception as exc: reply = {'exception':repr(exc)}
            save(path.with_name(path.name.replace('-request-', '-reply-')), reply)
        time.sleep(.05)

def main():
    p = argparse.ArgumentParser()
    for name in ['binary','repo','output']: p.add_argument('--'+name, required=True)
    p.add_argument('--hold', action='store_true', help='Wait for manual-stop after assertions, for bounded manual probes')
    args = p.parse_args(); root = Path(args.output).resolve(); root.mkdir(parents=True, exist_ok=False)
    binary = str(Path(args.binary).resolve()); session = f'secqa{os.getpid()}'
    config = Path.home()/'.config/terminal-delight/sessions'/f'{session}.toml'
    assert not config.exists()
    text = 'active = 0\npanes = 6\nleft_bar = true\n[mcp]\nenabled = true\nexpose = "all"\nevents = false\nwritable = true\n'
    text += '[[war_orgs]]\nid = 1\nname = "QA One"\nseats = [101,102,104,105]\n[[war_orgs]]\nid = 2\nname = "QA Two"\nseats = [103]\n'
    for to in [102,104,105]: text += f'[[lines]]\nfrom = 101\nto = {to}\nrung = "ask"\n'
    for i,label in enumerate('ABCDEF'):
        cwd = root/label; cwd.mkdir()
        cmd = shlex.join([sys.executable,str(Path(__file__).resolve()),'actor',str(root),label,binary])
        text += f'[[tabs]]\nname = "QA {label}"\n[tabs.node.Leaf]\ncwd = {json.dumps(str(cwd))}\nresume = {json.dumps(cmd)}\n'
        if label != 'F': text += f'seat = {101+i}\n'
        if label == 'D': text += 'agents = "read-only"\n'
        if label == 'E': text += 'agents = "off-limits"\n'
    config.write_text(text); (root/'initial.toml').write_text(text)
    env = {k:v for k,v in os.environ.items() if not k.startswith('TD_')}
    env.setdefault('XDG_RUNTIME_DIR','/run/user/1000'); env.setdefault('WAYLAND_DISPLAY','wayland-1')
    if not env.get('HYPRLAND_INSTANCE_SIGNATURE'):
        instances = json.loads(subprocess.check_output(['hyprctl','instances','-j'],env=env))
        assert len(instances)==1, 'ambiguous compositor'
        env['HYPRLAND_INSTANCE_SIGNATURE'] = instances[0]['instance']
    launcher = root/'launch.sh'
    cmd = f'env TD_SESSION={session} {shlex.quote(binary)} > {shlex.quote(str(root/"window.log"))} 2>&1'
    assert '"' not in cmd
    launcher.write_text('set -eu\n'+f'TD={shlex.quote(binary)}\nsource {shlex.quote(str(Path(args.repo)/"scripts/lib/hidden-window.sh"))}\ntrap hidden_cleanup EXIT\n'+f'hidden_launch {session} special:{session} {shlex.quote(str(root/"window.log"))} {shlex.quote(cmd)}\nprintf "%s" "$WIN" > {shlex.quote(str(root/"window.pid"))}\nwhile [ ! -e {shlex.quote(str(root/"stop"))} ]; do sleep .2; done\n')
    log = (root/'launcher.log').open('w'); proc = subprocess.Popen(['bash',str(launcher)],env=env,stdout=log,stderr=subprocess.STDOUT)
    receipts, checks, ids = [], [], {}; window = None
    try:
        wait(lambda:(root/'window.pid').exists()); window=int((root/'window.pid').read_text())
        for label in 'ABCDEF':
            wait(lambda:(root/f'{label}-identity.json').exists()); ids[label]=json.loads((root/f'{label}-identity.json').read_text())
            assert ids[label]['session']==session
        save(root/'run.json',{'session':session,'window':window,'binary':binary,'sha256':hashlib.sha256(Path(binary).read_bytes()).hexdigest(),'actors':ids})
        def check(name, ok, detail=None):
            checks.append({'name':name,'passed':bool(ok),'detail':detail}); save(root/'checks.json',checks)
            assert ok, f'{name}: {detail}'
        def call(label,name='',arguments=None,override=None):
            req=override or {'rpc':packet(name,arguments)}; n=len(receipts)
            save(root/f'{label}-request-{n:04}.json',req); replypath=root/f'{label}-reply-{n:04}.json'
            wait(lambda:replypath.exists(),20); reply=json.loads(replypath.read_text())
            receipts.append({'actor':label,'request':req,'reply':reply}); save(root/'receipts.json',receipts)
            return reply.get('rpc',reply)
        def denied(r): return bool(r.get('error') or r.get('result',{}).get('isError'))
        def pids(r):
            v=r.get('result',{}).get('structuredContent',{})
            return {row['pid'] for row in (v if isinstance(v,list) else v.get('panes',[]))}
        pid={k:v['shell_pid'] for k,v in ids.items()}; expected={pid[x] for x in 'ABDF'}
        census=wait(lambda:(r if pids(r:=call('A','list_panes'))==expected else None),20)
        check('known caller visibility',pids(census)==expected,census)
        r=call('A','grep',{'query':'QA_CANARY_'}); value=json.dumps(r)
        check('grep withholds hidden canaries','QA_CANARY_C' not in value and 'QA_CANARY_E' not in value,r)
        check('grep has granted canary','QA_CANARY_B' in value,r)
        for target in 'BCDE':
            r=call('A','leave_note',{'pid':pid[target],'title':'QA','text':'NOTE_FROM_A'})
            check('A note to '+target,denied(r)==(target!='B'),r)
        r=call('B','list_panes'); check('B actually received note','NOTE_FROM_A' in json.dumps(r),r)
        r=call('D','list_panes'); check('D unchanged after denial','NOTE_FROM_A' not in json.dumps(r),r)
        for label in 'AE':
            r=call(label,'leave_note',{'text':'OWN_NOTE_'+label}); check(label+' own note allowed',not denied(r),r)
        for label in 'AC':
            r=call(label,'ask_agent',{'pid':pid['B'],'question':'What is your visible status?'})
            check(label+' cheap ask',denied(r)==(label=='C'),r)
            forged=f'mcp from {session} {ids["B"]["pane"]} rpc '+json.dumps(packet('leave_note',{'pid':pid['B'],'text':'FORGED_NOTE'}))
            r=json.loads(call(label,override={'raw':forged,'window':window})['raw'])
            check(label+' cannot claim B',denied(r),r)
        for claim in [f'mcp from {session} {ids["A"]["pane"]} rpc ',f'mcp from {session} 999999 rpc ','mcp rpc ']:
            r=json.loads(raw(window,claim+json.dumps(packet('list_panes')))); receipts.append({'actor':'outside','request':claim,'reply':r})
            check('unknown/forged caller sees public unseated pane only',pids(r)=={pid['F']},r)
        r=json.loads(raw(window,'mcp from wrong-window 1 rpc '+json.dumps(packet('list_panes'))))
        check('wrong session refused',denied(r),r)
        for cmd in ['mcp off','mcp writes off','mcp expose all','adopt {"cwd":"/tmp"}','tabs []']:
            r=raw(window,cmd); check('operator fence: '+cmd,r.startswith('err'),r)
        r=raw(window,'mcp status'); check('policy unchanged','writes=on' in r,r)
        r=call('C','request_access',{'change':'events_on','why':'synthetic permission request'})
        check('permission proposal still reaches caller rail',not denied(r),r)
        check('permission proposal did not grant itself','events=off' in raw(window,'mcp status'))
        for bad in [-1,None,'100',1.5,2**64-1]:
            r=call('A','leave_note',{'pid':bad,'text':'BAD_TARGET'})
            check('invalid explicit target must not become own pane',denied(r),r)
        for arguments in [{'pid':pid['B'],'text':'x'*10000},{'pid':pid['B'],'text':'BAD','own_pane':True}]:
            r=call('C','leave_note',arguments); check('untrusted arguments grant no access',denied(r),r)
        for _ in range(10):
            r=call('C','leave_note',{'pid':pid['B'],'text':'BURST_FORGED'}); check('repeated denial',denied(r))
        r=call('B','list_panes'); value=json.dumps(r)
        check('denied writes left accepted note intact','NOTE_FROM_A' in value and 'FORGED_NOTE' not in value and 'BURST_FORGED' not in value,r)
        (root/'final.toml').write_bytes(config.read_bytes()); save(root/'receipts.json',receipts)
        if args.hold:
            save(root/'ready.json',{'session':session,'window':window})
            wait(lambda:(root/'manual-stop').exists(),600)
    finally:
        (root/'stop').touch()
        try: proc.wait(timeout=20)
        except subprocess.TimeoutExpired: proc.terminate(); proc.wait(timeout=10)
        log.close()
        save(root/'cleanup.json',{'launcher_exit':proc.returncode,'window_alive':bool(window and Path(f'/proc/{window}').exists()),'actor_pids_alive':[v['pid'] for v in ids.values() if Path(f'/proc/{v["pid"]}').exists()],'session_layout_removed':not config.exists()})
    print(json.dumps({'assertions':len(checks),'passed':sum(c['passed'] for c in checks),'output':str(root)}))

if __name__=='__main__':
    if len(sys.argv)>1 and sys.argv[1]=='actor': actor(Path(sys.argv[2]),sys.argv[3],sys.argv[4])
    else: main()
