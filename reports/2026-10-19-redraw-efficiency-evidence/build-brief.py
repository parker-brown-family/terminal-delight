"""Assemble the local brief from captured pairs and the managed notes assets."""
from pathlib import Path
import html
import json
import re
import statistics
import sys

HERE = Path(__file__).resolve().parent
SKILL = Path('/home/parker/.codex/skills/decision-brief')
NAME = '2026-10-19-td-redraw-efficiency.html'
out = Path(sys.argv[1]) if len(sys.argv) > 1 else HERE.parent / NAME
meta = json.loads((HERE / 'release.json').read_text())

def rows(name):
    return [json.loads(line) for line in (HERE / name / 'samples.jsonl').read_text().splitlines()]

def visible(name):
    return [(r['percent_one_cpu'][0], r['percent_one_cpu'][2]) for r in rows(name)]

def hidden(name, condition):
    return [(r['cpu']['baseline_window'], r['cpu']['candidate_window'])
            for r in rows(name) if r['condition'] == condition]

def stats(pairs):
    assert pairs
    a, b = zip(*pairs)
    return dict(before=statistics.median(a), after=statistics.median(b),
                saved=statistics.median(x-y for x,y in pairs),
                reduction=statistics.median(100*(x-y)/x for x,y in pairs),
                favorable=sum(y<x for x,y in pairs), count=len(pairs))

mixed = visible('mixed') + visible('mixed-reverse')
agent = hidden('hidden', 'working') + hidden('hidden-reverse', 'working')
plain = hidden('plain', 'working')
cases = [('One busy + three quiet', mixed),
         ('Four busy panes · 20 Hz each', visible('all-busy')),
         ('Four busy panes · 100 Hz each', visible('saturated')),
         ('Thirty hidden plain programs', plain),
         ('Thirty hidden synthetic agents · variable result', agent)]
chart = []
for title, pairs in cases:
    chart.append(f'<div class="cpu-case"><h3>{html.escape(title)}</h3>')
    for index, label in enumerate(['Before', 'After']):
        values = [p[index] for p in pairs]
        median = statistics.median(values)
        klass = 'after' if index else 'before'
        dots = ''.join(f'<span class="dot" style="left:{v*2:.3f}%;top:{4+(i%3)*4}px" title="Pair {i+1}: {v:.2f}%"></span>' for i,v in enumerate(values))
        assert max(values) <= 50, 'expand the shared chart axis instead of clipping a measurement'
        chart.append(f'<div class="cpu-row {klass}"><span>{label}</span><div class="track"><span class="bar" style="width:{median*2:.3f}%"></span>{dots}</div><b>{median:.1f}%</b></div>')
    chart.append('<div class="axis"><span>0</span><span>25</span><span>50% of one core</span></div></div>')

all_cases = [('Mixed',visible('mixed')),('Mixed, reverse launch',visible('mixed-reverse')),
             ('All busy, 20 Hz',visible('all-busy')),('All busy, 100 Hz',visible('saturated')),
             ('Identical old builds',visible('aa')),
             ('Synthetic agents idle',hidden('hidden','idle')),
             ('Synthetic agents working',hidden('hidden','working')),
             ('Synthetic agents idle, reverse',hidden('hidden-reverse','idle')),
             ('Synthetic agents working, reverse',hidden('hidden-reverse','working')),
             ('Plain programs idle',hidden('plain','idle')),
             ('Plain programs working',plain)]
table = ['<table class="pair-table"><thead><tr><th>Workload / pair</th><th>Before %</th><th>After %</th><th>Saved points</th></tr></thead><tbody>']
for title,pairs in all_cases:
    for i,(a,b) in enumerate(pairs,1):
        table.append(f'<tr><td>{html.escape(title)} · {i}</td><td>{a:.2f}</td><td>{b:.2f}</td><td>{a-b:+.2f}</td></tr>')
table.append('</tbody></table>')

m, a, p, sat = map(stats,[mixed,agent,plain,visible('saturated')])
aa_delta=[a-b for a,b in visible('aa')]
reverse=stats(visible('mixed-reverse'))
summary={name:stats(pairs) for name,pairs in all_cases}
(HERE/'measurements-summary.json').write_text(json.dumps(summary,indent=2)+'\n')

replacements = {
 'MIXED_REDUCTION': f"{m['reduction']:.0f}",
 'SHIP_STATUS': html.escape(meta['status']),
 'CPU_CHART': ''.join(chart),
 'HIDDEN_CONCLUSION': f"Plain hidden programs saved {p['saved']:.1f} points at the median. Synthetic agents improved in {a['favorable']} of {a['count']} pairs; that noisier result does not establish a dependable agent-workload gain.",
 'FINAL_TESTS': html.escape(meta['tests']),
 'MEMORY': 'One mixed run showed +12.5 MiB for the new window; reversing launch order showed −1.2 MiB. Two identical old builds differed by +47.4 MiB. The added storage is one retained text viewport per rendered pane; its resident-memory cost remains unresolved.',
 'SATURATED': f"At 100 updates per second in each of four panes, window CPU went from {sat['before']:.1f}% to {sat['after']:.1f}% of one core, with improvement in all five pairs.",
 'AA': f"Two identical old builds differed by {min(aa_delta):+.2f} to {max(aa_delta):+.2f} CPU points across five pairs.",
 'REVERSE': f"a {reverse['reduction']:.1f}% median paired reduction",
 'KEEP_REASON': html.escape(meta['keep_reason']),
 'DEPLOYMENT': html.escape(meta['deployment']),
 'PAIR_TABLE': ''.join(table),
 'FINAL_VERIFICATION': html.escape(meta['verification']),
 'EVIDENCE_LINKS': ''.join(f'<p><a href="{html.escape(url,quote=True)}">{html.escape(label)}</a></p>' for label,url in meta['links']),
}
body=(HERE/'brief-body.html').read_text()
for key,value in replacements.items(): body=body.replace('@@'+key+'@@',value)
assert not re.search(r'@@\w+@@',body)
def anchor(prefix, title):
    # Match the managed notes asset's title slug, so its exported anchors are
    # also literal IDs an agent can find in the original file before saving.
    slug=re.sub(r'[^a-z0-9]+','-',html.unescape(title).lower()).strip('-')[:44]
    return prefix+'-'+'-'.join(slug.split('-')[:4])
def figure_title(match):
    number,title=match.group(1,2)
    return f'<figure id="{anchor("fig",number+" "+title)}"><div class="fig-head"><span class="lbl"><b>{number}</b> {title}</span>'
body=re.sub(r'<figure id="[^"]+">\s*<div class="fig-head"><span class="lbl">(\d+)</span><strong>(.*?)</strong>',figure_title,body)
body=re.sub(r'<div class="ask" id="[^"]+"><h3>(.*?)</h3>',lambda m:f'<div class="ask" id="{anchor("ask",m[1])}"><h3>{m[1]}</h3>',body)
notes=(SKILL/'reference/notes-markup.html').read_text().replace('<YYYY-MM-DD>-<topic-slug>.html',NAME)
notes=re.sub(r'<!--.*?-->','',notes,flags=re.S)
js=(SKILL/'assets/notes.js').read_text()
assert '</script' not in js.lower(), 'unsafe inline JavaScript boundary'
interaction="""function wireReport(){document.querySelectorAll('tr.notable').forEach(row=>{const button=row.querySelector(':scope > .note-btn');const cell=row.querySelector('td:last-of-type');if(button&&cell)cell.append(button);});document.querySelectorAll('[data-open]').forEach(b=>b.addEventListener('click',()=>document.getElementById(b.dataset.open).showModal()));document.querySelectorAll('[data-close]').forEach(b=>b.addEventListener('click',()=>b.closest('dialog').close()));}if(document.readyState==='loading')document.addEventListener('DOMContentLoaded',wireReport);else wireReport();"""
page='<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Terminal Delight · Less work between changes</title>'
page+='<style>'+(SKILL/'assets/base.css').read_text()+'</style>'
page+=(HERE/'brief-wall.html').read_text()
page+='<style>'+(HERE/'brief.css').read_text()+'</style><style>'+(SKILL/'assets/notes.css').read_text()+'</style></head><body>'
page+=body+notes+'<script>'+js+'</script><script>'+interaction+'</script></body></html>'
out.write_text(page)
print(out)
