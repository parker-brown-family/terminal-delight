from pathlib import Path
import json,statistics,html
root=Path(__file__).resolve().parents[2]
evidence=Path(__file__).parent
profile=evidence/'candidate-profile.txt'
if profile.exists():profile.write_text('\n'.join(line.rstrip() for line in profile.read_text().splitlines())+'\n')
rows=[json.loads(x) for x in (evidence/'samples.jsonl').read_text().splitlines() if x.strip()]
stats={}
for kind in ['terminal','bench','document']:
    selected=[r for r in rows if r['condition'].startswith(kind+'-')]
    if len(selected)!=5:raise RuntimeError(f'{kind}: expected five complete pairs, got {len(selected)}')
    before=[r['percent_one_cpu'][0] for r in selected]
    after=[r['percent_one_cpu'][2] for r in selected]
    stats[kind]={'before':before,'after':after,'median_before':statistics.median(before),'median_after':statistics.median(after),'median_paired_saving':statistics.median(a-b for a,b in zip(before,after))}
(evidence/'summary.json').write_text(json.dumps(stats,indent=2)+'\n')
def svg(content,height=380,label=''):
    return f'<div class="scroller"><svg viewBox="0 0 760 {height}" role="img" aria-label="{html.escape(label)}">{content}</svg></div>'
maximum=max(max(v['before']+v['after']) for v in stats.values())*1.15
chart='<text x="22" y="28" class="ink2">WINDOW CPU · percent of one logical CPU</text>'
labels={'terminal':'Terminal visible','bench':'Workbench visible','document':'Document floating'}
for i,(k,v) in enumerate(stats.items()):
    y=75+i*100
    chart+=f'<text x="22" y="{y}" class="ink">{labels[k]}</text>'
    for j,(field,cls) in enumerate([('median_before','struct'),('median_after','ready')]):
        val=v[field]; yy=y+13+j*26; w=val/maximum*430
        chart+=f'<rect class="{cls}" x="220" y="{yy-13}" width="{w:.2f}" height="18" rx="3"/><text x="675" y="{yy+1}" class="ink">{val:.1f}%</text>'
        points=v["before" if field=="median_before" else "after"]
        for point in points:
            x=220+point/maximum*430
            chart+=f'<circle cx="{x:.2f}" cy="{yy-4}" r="3" style="fill:var(--fgb);stroke:var(--bg0);stroke-width:1"/>'
    chart+=f'<text x="22" y="{y+33}" class="ink2">5 paired samples</text>'
chart+='<text x="22" y="376" class="struct">Blue: baseline</text><text x="245" y="376" class="ready">Green: candidate</text>'
fig1=svg(chart,400,'Five paired CPU samples in each of three workloads')
flow=svg('''<rect class="struct" x="18" y="35" width="210" height="74" rx="10"/><text class="ink" x="34" y="63">Terminal output</text><text class="ink2" x="34" y="88">Emulation keeps running</text><path class="wire" d="M228 73 H280"/>
<rect class="struct" x="280" y="23" width="215" height="100" rx="10"/><text class="ink" x="298" y="52">Is the grid visible?</text><text class="ink2" x="298" y="79">Workbench / document</text><text class="ink2" x="298" y="102">covers it → skip styling</text>
<path class="wire" d="M388 123 V175"/><rect class="struct" x="280" y="175" width="215" height="110" rx="10"/><text class="ink" x="298" y="205">Compare the viewport</text><text class="ink2" x="298" y="232">Cells · selection · cursor</text><text class="ink2" x="298" y="255">Scroll · size · theme</text><path class="wire" d="M495 230 H550"/>
<rect class="ready" x="550" y="176" width="190" height="110" rx="10"/><text class="ink" x="568" y="207">Same → reuse styles</text><text class="ink2" x="568" y="236">Changed → rebuild</text><text class="ink2" x="568" y="260">One viewport retained</text>
<rect class="ready" x="18" y="333" width="722" height="73" rx="10"/><text class="ink" x="36" y="361">Document layout and paint stay cached until the view changes.</text><text class="ink2" x="36" y="386">Scrolling, edits, colours, position, size and the visible region still trigger a redraw.</text>''',430,'The terminal remains live while redundant styling and document layout are reused')
checks=svg('''<text x="22" y="35" class="ink2">BEHAVIOUR EXERCISED IN REAL PANE TESTS</text>
<rect class="ready" x="22" y="60" width="340" height="70" rx="8"/><text class="ink" x="40" y="87">Changed terminal output</text><text class="ink2" x="40" y="112">Matches a fresh styled viewport</text>
<rect class="ready" x="390" y="60" width="340" height="70" rx="8"/><text class="ink" x="408" y="87">Selection, cursor and scrolling</text><text class="ink2" x="408" y="112">Retire the old styled result</text>
<rect class="ready" x="22" y="150" width="340" height="70" rx="8"/><text class="ink" x="40" y="177">Quiet document redraws</text><text class="ink2" x="40" y="202">Reuse layout and paint</text>
<rect class="ready" x="390" y="150" width="340" height="70" rx="8"/><text class="ink" x="408" y="177">Document changes and geometry</text><text class="ink2" x="408" y="202">Scroll, palette, move and resize</text>
<rect class="unk" x="22" y="246" width="708" height="65" rx="8"/><text class="ink" x="40" y="274">Not measured here: end-to-end input latency and a wall of live agents</text><text class="ink2" x="40" y="297">Synthetic output is a declared workload, not proof about every agent.</text>''',335,'Verified behaviours and the limits of these measurements')
desktop=svg('''<text class="ink2" x="22" y="34">SEPARATE 15-SECOND DESKTOP-SERVICE OBSERVATIONS</text>
<text class="ink" x="22" y="82">Before removal</text><rect class="struct" x="280" y="62" width="396" height="26" rx="4"/><text class="ink" x="686" y="83">165.5%</text>
<text class="ink" x="22" y="137">After settling</text><rect class="struct" x="280" y="117" width="164" height="26" rx="4"/><text class="ink" x="454" y="138">68.7%</text>
<text class="ink" x="22" y="192">Later recheck</text><rect class="struct" x="280" y="172" width="376" height="26" rx="4"/><text class="ink" x="666" y="193">157.2%</text>
<rect class="unk" x="22" y="233" width="708" height="70" rx="8"/><text class="ink" x="40" y="261">The remaining cause is unresolved.</text><text class="ink2" x="40" y="285">Different workloads; includes compositor and shell descendants.</text>''',325,'Burnbar removal did not eliminate the desktop-service CPU activity')
s=stats
body=f'''<header><p class="kicker">TERMINAL DELIGHT · 17 OCTOBER 2026 · MEASURED CANDIDATE</p><h1>Terminal Delight<br>uses less CPU.</h1><p class="lede">In a four-pane test with a program printing beneath an unchanged document, Terminal Delight’s CPU use fell from 59% of one core to 40%. Reusing drawing work reduced the other tested views too. The desktop’s remaining CPU use is tracked separately.</p></header>
<main>
<section><h2>First, the hidden-pane work was already there.</h2><p>Hidden-pane throttling, screen-copy reuse and output limits were already shipped. The next waste was visible: quiet text and documents were prepared for drawing again whenever their parent pane redrew.</p>
<figure><span class="lbl">01 · The actual window comparison</span>{fig1}<figcaption><b>The effect depends on what is visible.</b> Bars show medians; dots show all five paired eight-second intervals per workload. Each isolated window had four visible panes: one targeting twenty updates a second, three quiet, CRT off. The document case adds the same 120-section Markdown file. Raw pairs remain available; these are CPU percentages of one logical CPU, not the whole machine.</figcaption></figure>
</section>
<section><h2>Quiet content can reuse the previous result.</h2><p>TD still reads the visible terminal. When its cells, colours and selection match, it keeps the colour and font choices it already calculated. It skips that calculation behind another pane face. A document also keeps its layout and drawing until something changes.</p>
<figure><span class="lbl">02 · What the implementation reuses</span>{flow}<figcaption><b>Output keeps flowing while redundant drawing preparation stops.</b> Drawn from the candidate’s styling key and GPUI cache boundary. Returning to the terminal reads current output; caller-side highlighting cannot modify the retained copy. This drawing describes the code, not a timing estimate.</figcaption></figure>
</section>
<section><h2>Reuse must not hide a real change.</h2><p>Tests print new text, move the cursor, select and scroll, then compare reused results with fresh styling. They also count document redraws. Existing pane tests exercise documents, notes and pointer actions. A forced GPUI refresh provides a control that bypasses document caching.</p>
<figure><span class="lbl">03 · What was checked</span>{checks}<figcaption><b>A quieter frame is useful only if the next change still appears.</b> The new tests cover reuse and invalidation; the complete validation record names the core suites and strict Clippy checks. Whole-window CPU and styling-only timing are separate measurements. Input response and real-agent scaling remain unmeasured.</figcaption></figure>
</section>
<section><h2>The desktop still has work of its own.</h2><p>Your “can we get more?” note holds: removing Burnbar did not remove the desktop-shell activity. A later interval was high again. Its repeated application scans remain under investigation, independently of TD’s rendering work.</p>
<figure><span class="lbl">04 · Burnbar was only part of the investigation</span>{desktop}<figcaption><b>The early drop was not a lasting desktop baseline.</b> These separate observations use the desktop service’s CPU counter and include other work. They do not establish Burnbar’s causal share. Reaped-child CPU is excluded from this drawing because it can include work accrued before an interval.</figcaption></figure>
</section>
<section><h2>The measured change is ready to review</h2><p>All three tested views use less CPU. The code and its evidence are ready for review together. Nothing was installed; the measurements used isolated copies.</p>
<div class="cards"><div class="card rec ask"><span class="badge">Measured candidate</span><h3>Keep the reuse changes</h3><p>Review the code and these measured results together. Terminal styling retains one visible screen per pane; document reuse uses the drawing framework already in TD.</p></div><div class="card"><span class="badge">Separate follow-up</span><h3>Trace the desktop scan trigger</h3><p>Burnbar is gone, but repeated desktop-entry scans remain. Their trigger and share of desktop CPU are still unknown.</p></div></div>
<details><summary>Exact method, validation and source</summary><p>The baseline was built from current main, a5fab29. Both windows ran concurrently on separate hidden workspaces of equal dimensions. Face order alternated by round after a three-second settling period. The host was measured separately. A baseline-versus-baseline control differed by less than 0.4 CPU percentage points in its terminal and workbench samples. Both binaries came from normal release builds. No profiling or build ran during these intervals. Actual update rate, cell dimensions and input latency are unmeasured; equal window geometry and pane layouts are retained. User workloads continued running.</p><p>Default core: 2,401 tests passed. Fallback core: 2,354 passed. Each suite has nine ignored tests; no failures. Formatting and strict Clippy passed for both cores. The cache-bypass control, document interaction checks and full suite logs are retained with the evidence.</p><p><a href="2026-10-17-td-styling-evidence/README.md">Reproduction and limits</a> · <a href="2026-10-17-td-styling-evidence/samples.jsonl">Raw paired counters</a> · <a href="2026-10-17-td-styling-evidence/provenance.json">Binary and source hashes</a> · <a href="../app/src/pane/styling.rs">Styling code and behaviour tests</a> · <a href="../app/src/pane/doc.rs">Document reuse</a> · <a href="https://github.com/parker-brown-family/omarchy-contrib/issues/5">Desktop scan investigation</a></p><p>The earlier brief used an old checkout. Its hidden-pane recommendation is superseded by fixes already on main; the original annotated file is preserved.</p></details>
</section></main><footer>Measured on this machine. Unknowns remain visible. Click any passage or figure to leave a note.</footer>'''
import re
sections=re.findall(r'<section>.*?</section>',body,re.S)
start=body.index('<section>');end=body.rindex('</section>')+len('</section>')
body=body[:start]+''.join(sections[i] for i in [3,0,1,2,4])+body[end:]
numbers=iter(range(1,5))
body=re.sub(r'(<span class="lbl">)\d{2} ·',lambda m:m.group(1)+f'{next(numbers):02d} ·',body)
body=body.replace('<section>','<section class="finding">')
depth=re.search(r'<details>.*?</details>',body,re.S).group()
inside=re.sub(r'<details><summary>.*?</summary>','',depth).removesuffix('</details>')
body=body.replace(depth,'<button class="drill" data-open="d-evidence">Method, checks and source →</button>')
body+='<dialog id="d-evidence"><div class="dlg-head"><h3>Method, checks and source</h3><button class="x" data-close>×</button></div><div class="dlg-body"><h4>The exact comparison</h4>'+inside+'</div></dialog>'
assets=Path('/home/parker/.codex/skills/decision-brief')
markup=(assets/'reference/notes-markup.html').read_text().replace('<YYYY-MM-DD>-<topic-slug>.html','2026-10-17-td-efficiency-results.html')
css=(assets/'assets/base.css').read_text()+'\n'+(assets/'assets/notes.css').read_text()
custom='''main,header,footer{max-width:1000px;margin:auto;padding-left:28px;padding-right:28px}header{padding-top:68px}h1{max-width:850px}.lede{max-width:780px}section{margin:54px 0}svg{display:block;min-width:600px;width:100%;height:auto;font:17px ui-monospace,monospace}svg text.ink2{font-size:15px}svg rect{stroke-width:1.5}svg path{fill:none;stroke-width:2}details{padding:18px;border:1px solid var(--line2);border-radius:14px}summary{cursor:pointer}a{overflow-wrap:anywhere}footer{padding-bottom:100px}@media(max-width:550px){main,header,footer{padding-left:18px;padding-right:18px}h1{font-size:46px}.cards{grid-template-columns:1fr}}'''
wall=Path('/tmp/td-efficiency-result-wall.html').read_text()
js=(assets/'assets/notes.js').read_text()+""";document.querySelectorAll('[data-open]').forEach(b=>b.addEventListener('click',()=>document.getElementById(b.dataset.open).showModal()));"""
js=js.replace('</script','<\\/script')
out=root/'reports/2026-10-17-td-efficiency-results.html'
out.write_text('<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Terminal Delight — measured efficiency changes</title><style>'+css+'</style>'+wall+'<style>'+custom+'</style></head><body>'+body+markup+'<script>'+js+'</script></body></html>')
print(out)
