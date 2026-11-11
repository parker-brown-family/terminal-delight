#!/usr/bin/env python3
"""Measure the real TD renderer with nine retained pages; no live TD interaction."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import xml.etree.ElementTree as ET

HERE = Path(__file__).resolve().parent
OUT = Path(sys.argv[1])
GPU = sys.argv[2]
FILES = sys.argv[3:]

def proc_table():
    out = {}
    for d in Path('/proc').iterdir():
        if not d.name.isdigit():
            continue
        try:
            s = (d / 'stat').read_text().rsplit(')', 1)[1].split()
            out[int(d.name)] = {'ppid': int(s[1]), 'ticks': int(s[11]) + int(s[12])}
        except (OSError, ValueError):
            pass
    return out

def members(pid, table):
    found = {pid}
    while True:
        more = found | {p for p, x in table.items() if x['ppid'] in found}
        if more == found:
            return found - {pid}
        found = more

def gpu_memory(pids):
    try:
        run = subprocess.run(['nvidia-smi', '-q', '-x'], capture_output=True, text=True, timeout=5, check=True)
        values = []
        for p in ET.fromstring(run.stdout).findall('.//process_info'):
            if int(p.findtext('pid')) in pids:
                text = p.findtext('used_memory')
                if text is None or not text.endswith(' MiB'):
                    return None
                values.append({'pid': int(p.findtext('pid')), 'bytes': int(text.split()[0])*1024**2})
        # Successful query with no matching process means zero NVIDIA allocation reported.
        return {'bytes': sum(p['bytes'] for p in values), 'processes': values}
    except (OSError, ValueError, ET.ParseError, subprocess.SubprocessError):
        return None

def snapshot(pid, runtime):
    table = proc_table()
    owned = members(pid, table)
    details = []
    for p in sorted(owned):
        try:
            vals = {}
            for line in Path(f'/proc/{p}/smaps_rollup').read_text().splitlines():
                s = line.split()
                if len(s) == 3 and s[2] == 'kB':
                    vals[s[0].rstrip(':')] = int(s[1])*1024
            cmd = Path(f'/proc/{p}/cmdline').read_bytes().split(b'\0')
            role = next((v.decode().split('=', 1)[1] for v in cmd if v.startswith(b'--type=')), 'browser')
            details.append({'pid': p, 'role': role, 'pss_bytes': vals['Pss'], 'rss_bytes': vals['Rss'], 'swap_bytes': vals['Swap']})
        except OSError:
            pass
    profile_files = list(runtime.rglob('*')) if runtime.exists() else []
    profile_bytes = sum(p.stat().st_size for p in profile_files if p.is_file())
    return {'pss_bytes': sum(p['pss_bytes'] for p in details), 'rss_bytes': sum(p['rss_bytes'] for p in details),
            'swap_bytes': sum(p['swap_bytes'] for p in details), 'processes': details,
            'nvidia': gpu_memory(owned), 'profile_file_bytes': profile_bytes,
            'ticks': {str(p): table[p]['ticks'] for p in owned if p in table}}

env = os.environ.copy()
if GPU == 'vulkan':
    env['PROFILE_GPU'] = 'vulkan'
else:
    env.pop('PROFILE_GPU', None)
OUT.parent.mkdir(parents=True, exist_ok=True)
with OUT.with_suffix('.stderr').open('w') as err, OUT.open('w') as out:
    binary = os.environ.get('RESOURCE_PROBE', str(HERE/'target/release/td-html-resource-profile'))
    proc = subprocess.Popen([binary, *FILES], env=env,
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=err, text=True, bufsize=1)
    runtime = Path(f'/tmp/td-html-resource-profile-{proc.pid}')
    try:
        for line in proc.stdout:
            stage = json.loads(line)
            time.sleep(0.5)
            first = snapshot(proc.pid, runtime)
            t = time.monotonic()
            time.sleep(1)
            second = snapshot(proc.pid, runtime)
            seconds = time.monotonic() - t
            common = set(first['ticks']) & set(second['ticks'])
            cpu = sum(second['ticks'][p]-first['ticks'][p] for p in common)/os.sysconf('SC_CLK_TCK')
            record = dict(stage, gpu=GPU, resources=second, idle_cpu_one_core_percent=100*cpu/seconds)
            out.write(json.dumps(record)+'\n'); out.flush()
            print(json.dumps({'stage': stage['stage'], 'pages': stage.get('pages'), 'pss_mib': round(second['pss_bytes']/1024**2, 1),
                              'gpu_mib': None if second['nvidia'] is None else second['nvidia']['bytes']/1024**2,
                              'cpu_percent': round(record['idle_cpu_one_core_percent'], 1)}), flush=True)
            # This is a diagnostic ceiling, not a proposed product budget.
            stop = second['pss_bytes'] > 3*1024**3
            proc.stdin.write('stop\n' if stop else 'next\n'); proc.stdin.flush()
        code = proc.wait(timeout=10)
        if code:
            raise SystemExit(code)
    finally:
        if proc.poll() is None:
            proc.terminate()
            proc.wait(timeout=10)
