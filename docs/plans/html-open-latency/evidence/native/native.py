#!/usr/bin/env python3
"""Native HTML timing on an isolated X server; never touches the live seat."""
import argparse
import json
import os
from pathlib import Path
import signal
import shutil
import socket
import subprocess
import time
import xml.etree.ElementTree as ET

parser = argparse.ArgumentParser()
parser.add_argument('--bin', required=True)
parser.add_argument('--brief', required=True)
parser.add_argument('--out', required=True)
parser.add_argument('--cycles', type=int, default=10)
parser.add_argument('--mode', choices=['square', 'reader'], default='reader')
parser.add_argument('--prepare-seconds', type=float, default=0)
parser.add_argument('--panes', type=int, default=1)
parser.add_argument('--baseline', action='store_true')
parser.add_argument('--empty-tabs', action='store_true')
parser.add_argument('--cold', action='store_true')
parser.add_argument('--idle-seconds', type=int)
parser.add_argument('--exercise', action='store_true')
args = parser.parse_args()
out = Path(args.out).resolve()
out.mkdir(parents=True, exist_ok=True)
if args.exercise:
    copied = out / 'exercise.html'
    shutil.copyfile(args.brief, copied)
    args.brief = str(copied)
env = dict(os.environ)
for key in list(env):
    if key.startswith(('TD_', 'CLAUDE_', 'CODEX_')) or key in ('WAYLAND_DISPLAY', 'HYPRLAND_INSTANCE_SIGNATURE'):
        env.pop(key)
for name in ('config', 'state', 'cache', 'runtime'):
    (out / name).mkdir(exist_ok=True, mode=0o700)
env.update(DISPLAY='', XDG_CONFIG_HOME=str(out/'config'), XDG_STATE_HOME=str(out/'state'),
           XDG_CACHE_HOME=str(out/'cache'), XDG_RUNTIME_DIR=str(out/'runtime'),
           XDG_SESSION_TYPE='x11', TD_SESSION=f'html-perf-{os.getpid()}', TD_DOCDEBUG='1',
           TD_MCP_SCRIPTED='1', SHELL='/bin/sh')
if args.idle_seconds is not None:
    env['TD_PAGE_IDLE_SECS'] = str(args.idle_seconds)
xlog = (out/'xvfb.log').open('w')
xvfb = subprocess.Popen(['Xvfb', '-displayfd', '1', '-screen', '0', '1600x1000x24', '-nolisten', 'tcp'],
                        stdout=subprocess.PIPE, stderr=xlog, text=True)
env['DISPLAY'] = ':' + xvfb.stdout.readline().strip()
log = (out/'window.log').open('w')
window = subprocess.Popen([args.bin, '-e', '/bin/sh', '-c', 'printf "ready\\n"; exec cat'],
                          env=env, stdout=log, stderr=log, start_new_session=True)
sock = out/'runtime'/'terminal-delight'/f'ctl-{window.pid}.sock'

def ctl(command):
    with socket.socket(socket.AF_UNIX) as s:
        s.settimeout(10)
        s.connect(str(sock))
        s.sendall((command+'\n').encode())
        return s.makefile().readline().strip()

def wait_until(predicate, seconds=40):
    until = time.monotonic()+seconds
    while time.monotonic() < until:
        if window.poll() is not None:
            raise RuntimeError(f'window exited {window.returncode}: {(out/"window.log").read_text()[-3000:]}')
        got = predicate()
        if got:
            return got
        time.sleep(.005)
    raise TimeoutError((out/'window.log').read_text()[-3000:])

def records():
    rows = []
    for line in (out/'window.log').read_text(errors='replace').splitlines():
        if line.startswith('[doc-timing] '):
            try:
                row = json.loads(line[len('[doc-timing] '):])
            except json.JSONDecodeError:
                continue  # stderr's final line can still be in flight.
            if row['ms']['viewport_paint'] is not None:
                rows.append(row)
        elif args.baseline and line.startswith('[doc] drew '):
            rows.append({'path':args.brief, 'ms':{}})
    return rows

def memory(label):
    descendants = {window.pid}
    todo = [window.pid]
    while todo:
        pid = todo.pop()
        try:
            children = []
            for thread in Path(f'/proc/{pid}/task').iterdir():
                try:
                    children.extend((thread/'children').read_text().split())
                except OSError:
                    pass
        except OSError:
            continue
        for child in map(int, children):
            if child not in descendants:
                descendants.add(child)
                todo.append(child)
    rows = []
    for pid in sorted(descendants):
        try:
            cmd = Path(f'/proc/{pid}/cmdline').read_bytes().replace(b'\0', b' ').decode(errors='replace')
            smaps = Path(f'/proc/{pid}/smaps_rollup').read_text()
            pss = next(int(l.split()[1]) for l in smaps.splitlines() if l.startswith('Pss:'))
            stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
            rows.append({'pid':pid, 'pss_kib':pss, 'cpu_ticks':int(stat[11])+int(stat[12]),
                         'browser':'chromium' in cmd, 'command':cmd})
        except (OSError, StopIteration):
            pass
    gpu = subprocess.run(['nvidia-smi', '-q', '-x'],
                         capture_output=True, text=True)
    gpu_memory = None
    if gpu.returncode == 0:
        gpu_memory = [{'pid':int(p.findtext('pid')), 'memory':p.findtext('used_memory')}
                      for p in ET.fromstring(gpu.stdout).findall('.//process_info')
                      if int(p.findtext('pid')) in descendants]
    record = {'label':label, 'monotonic':time.monotonic(), 'ticks_per_second':os.sysconf('SC_CLK_TCK'),
              'processes':rows, 'gpu_memory':gpu_memory}
    with (out/'memory.jsonl').open('a') as f:
        f.write(json.dumps(record)+'\n')
    return record

def xdo(*args):
    return subprocess.check_output(['xdotool', *map(str, args)], env=env, text=True).strip()

def rpc(name, arguments):
    reply = ctl('mcp rpc '+json.dumps({'jsonrpc':'2.0', 'id':1, 'method':'tools/call',
                                     'params':{'name':name, 'arguments':arguments}}))
    data = json.loads(reply.removeprefix('ok '))
    if 'error' in data or data.get('result', {}).get('isError'):
        raise RuntimeError(data)
    return data['result']['structuredContent']

try:
    wait_until(sock.exists)
    wid = wait_until(lambda: xdo('search', '--pid', window.pid).splitlines()[0])
    xdo('windowsize', wid, 1600, 1000)
    xdo('windowmove', wid, 0, 0)
    xdo('windowfocus', wid)
    time.sleep(1)
    memory('empty')
    for command in ('mcp on', 'mcp writes on', 'mcp expose all'):
        assert ctl(command).startswith('ok')
    time.sleep(1)
    # Keep one real HTML square in each hidden tab, using the same shared
    # document. Each view still has its own decoded pixels and reading state.
    for index in range(1, args.panes):
        if not args.empty_tabs:
            previous = len(records())
            assert ctl('doc here '+args.brief).startswith('ok')
            wait_until(lambda: records()[previous:])
            if index == 1:
                time.sleep(8)
        xdo('key', '--clearmodifiers', 'ctrl+shift+t')
        wait_until(lambda: len(rpc('list_panes', {})['panes']) == index+1)
        xdo('key', '--clearmodifiers', 'Escape')
        time.sleep(.15)
        if index in (8, 29, 49):
            time.sleep(1)
            memory(f'{index+1}-panes-with-{0 if args.empty_tabs else index}-hidden-documents')
    if args.mode == 'reader':
        pane = rpc('list_panes', {})['panes'][-1]
        rpc('declare_deliverable', {'pid':pane['pid'], 'label':'HTML performance specimen',
                                    'href':args.brief})
        time.sleep(args.prepare_seconds)
        if args.panes > 1:
            census = rpc('list_panes', {})
            (out/'panes.json').write_text(json.dumps(census, indent=2))
            assert len(census['panes']) == args.panes, len(census['panes'])
    samples = []
    for cycle in range(args.cycles):
        if args.cold:
            shutil.rmtree(out/'cache'/'terminal-delight'/'pages', ignore_errors=True)
        previous = len(records())
        begin = time.monotonic()
        if args.mode == 'reader':
            xdo('key', '--clearmodifiers', 'alt+o')
        else:
            reply = ctl('doc here '+args.brief)
            if not reply.startswith('ok'):
                raise RuntimeError(reply)
        got = wait_until(lambda: records()[previous:] or None)
        elapsed = (time.monotonic()-begin)*1000
        samples.append(dict(cycle=cycle, wall_ms=elapsed, **got[0]))
        print(json.dumps(samples[-1]), flush=True)
        # Allow the complete cache to commit before testing a warm reopen.
        if cycle == 0:
            time.sleep(8)
            memory('visible-reader')
            subprocess.run(['import', '-window', wid, str(out/'visible.png')], env=env, check=True)
            (out/'notes.json').write_text(ctl('doc notes'))
            if args.exercise:
                def notes():
                    reply = ctl('doc notes')
                    with (out/'note-replies.jsonl').open('a') as stream:
                        stream.write(json.dumps(reply)+'\n')
                    if '{' not in reply:
                        return {}  # Resize/reopen may still be adopting layout.
                    return json.loads(reply[reply.index('{'):])
                layout = next(json.loads(p.read_text()) for p in
                              (out/'cache'/'terminal-delight'/'pages').glob('*/layout.json')
                              if json.loads(p.read_text())['anchors'])
                anchor = layout['anchors'][0]['nid']
                marker = 'Native resize and save verification'
                assert ctl(f'doc note add {anchor} {marker}').startswith('ok')
                wait_until(lambda: marker in notes().get('map', ''))
                before = notes()
                xdo('windowsize', wid, 1400, 900)
                time.sleep(4)
                after = wait_until(lambda: (n if marker in (n := notes()).get('map', '') else None))
                assert marker in after['map'], after
                # Control-socket edits save immediately; Save may correctly
                # refuse when the asynchronous write has already finished.
                if after['unsaved']:
                    reply = ctl('doc save')
                    assert reply.startswith('ok'), reply
                wait_until(lambda: notes().get('unsaved') == 0 and marker in Path(args.brief).read_text())
                saved = notes()
                xdo('key', '--clearmodifiers', 'Escape')
                xdo('key', '--clearmodifiers', 'alt+o')
                wait_until(lambda: marker in notes().get('map', ''))
                reopened = notes()
                (out/'exercise.json').write_text(json.dumps(
                    dict(before=before, resized=after, saved=saved, reopened=reopened), indent=2))
        if args.mode == 'reader':
            xdo('key', '--clearmodifiers', 'Escape')
        else:
            assert ctl('doc close').startswith('ok')
        time.sleep(.1)
    (out/'samples.json').write_text(json.dumps(samples, indent=2))
    time.sleep(2)
    memory('closed-reader')
    time.sleep(3)
    memory('closed-reader-idle')
    if args.idle_seconds is not None:
        time.sleep(args.idle_seconds+2)
        memory('idle-browser-shutdown')
finally:
    # The window has its own session/process group. Its Chromium uses parent
    # death signals; the diagnostic cannot kill a browser in Parker's session.
    try:
        os.killpg(window.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        window.wait(timeout=10)
    except subprocess.TimeoutExpired:
        os.killpg(window.pid, signal.SIGKILL)
        window.wait()
    xvfb.terminate()
    xvfb.wait(timeout=5)
    log.close()
    xlog.close()
