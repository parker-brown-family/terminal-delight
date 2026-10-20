import os,json,time,subprocess
from pathlib import Path
out=Path('/tmp/chalk-light-final')
for _ in range(100):
    if (out/'window.pid').exists():break
    time.sleep(.2)
pid=int((out/'window.pid').read_text())
assert os.readlink(f'/proc/{pid}/exe')=='/tmp/td-chalk-verified'
hz=os.sysconf('SC_CLK_TCK')
def ticks():
    fields=Path(f'/proc/{pid}/stat').read_text().split(') ',1)[1].split()
    return int(fields[11])+int(fields[12])
def ctl(*args):
    subprocess.run(['/tmp/td-chalk-verified','ctl','--pid',str(pid),*args],check=True,capture_output=True)
rows=[]
for busy in [False,True]:
    if busy:(out/'busy').touch()
    for skin in ['default','paper','default','paper']:
        ctl('skin',skin)
        time.sleep(.5)
        start=time.monotonic();first=ticks();time.sleep(5)
        elapsed=time.monotonic()-start;spent=(ticks()-first)/hz
        pss=next(int(line.split()[1]) for line in Path(f'/proc/{pid}/smaps_rollup').read_text().splitlines() if line.startswith('Pss:'))
        row={'busy_20hz':busy,'skin':skin,'duration_s':round(elapsed,3),'cpu_s':spent,'cpu_percent_one_core':round(spent/elapsed*100,2),'pss_kib':pss}
        rows.append(row);print(json.dumps(row),flush=True)
    if busy:(out/'busy').unlink()
ctl('skin','paper')
(out/'performance.json').write_text(json.dumps(rows,indent=2))
