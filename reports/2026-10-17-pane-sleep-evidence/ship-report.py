"""Build the human reading only after measurement, checks and live delivery exist."""
import html
import json
import os
from pathlib import Path
from string import Template

OUT = Path(__file__).resolve().parent
preview = os.environ.get("TD_SLEEP_REPORT_PREVIEW") == "1"
REPORT = OUT.parent / ("2026-10-18-pane-sleep-preview.html" if preview else "2026-10-18-pane-sleep-shipped.html")
SKILL = Path("/home/parker/.codex/skills/decision-brief")
summary = json.loads((OUT / "summary.json").read_text())
control = json.loads((OUT / "control-summary.json").read_text())
repeat = json.loads((OUT / "reversed-order/summary.json").read_text())
builds = json.loads((OUT / "build-pair.json").read_text())
checks = json.loads((OUT / "validation.json").read_text())
delivery = None if preview else json.loads((OUT / "delivery.json").read_text())
mutations = json.loads((OUT / "mutations.json").read_text())
guard = json.loads((OUT / "guard-control.json").read_text())
assert len(guard) == 1 and guard[0]["rejected_by_behavior"]
assert guard[0]["required_assertion_text"] == "on_key looks at `key.as_str()` itself"
assert preview or delivery["stage"] == "passed"
installation = ("Candidate validated; live installation is pending." if preview else f'Installed and running in the live GUI. All {delivery["terminals_preserved"]} open terminal identities and start times survived the reconnect; host {delivery["host_pid"]} stayed running. INHERIT, Terminal State and the revised palette remain included.')
assert checks["default"]["failed"] == checks["fallback"]["failed"] == 0
assert checks["format"] and checks["clippy_default"] and checks["clippy_fallback"] and checks["native_smoke"]
assert len(mutations) == 5 and all(m["rejected_by_behavior"] for m in mutations)
quiet, working = [summary["conditions"][c] for c in ("idle", "working")]
repeat_quiet, repeat_working = [repeat["conditions"][c] for c in ("idle", "working")]
def change(row):
    r = row["relative_reduction_percent"]
    return "relative change unavailable" if r is None else f"{abs(r):.1f}% {'lower' if r >= 0 else 'higher'}"
def chart():
    max_cpu = max(r[k] for r in (quiet, repeat_quiet, working, repeat_working) for k in ("baseline_window_cpu_median", "candidate_window_cpu_median"))
    scale = 440 / max(max_cpu, 0.1)
    pieces = ['<svg viewBox="0 0 760 480" role="img" aria-label="Window CPU medians before and after timer cleanup, in percent of one logical CPU">']
    for label, row, top in (("Quiet · first", quiet, 44), ("Quiet · reverse", repeat_quiet, 148), ("Working · first", working, 252), ("Working · reverse", repeat_working, 356)):
        pieces += [f'<text x="20" y="{top}" class="ink">{label}</text>']
        for name, value, y, color in (("Before", row["baseline_window_cpu_median"], top + 10, "struct"),
                                       ("After", row["candidate_window_cpu_median"], top + 46, "ready")):
            pieces += [f'<text x="20" y="{y + 20}" class="ink2">{name}</text>',
                       f'<rect x="160" y="{y}" width="{value * scale:.1f}" height="25" rx="3" class="{color}"/>',
                       f'<text x="{170 + value * scale:.1f}" y="{y + 19}" class="ink">{value:.1f}%</text>']
    pieces.append('<text x="20" y="472" class="unk">Whole-machine speedup: unmeasured</text>')
    return "".join(pieces) + "</svg>"
pairs = "".join(f'<tr><td>{label}</td><td>{p["pair"]}</td><td>{p["baseline"]:.1f}%</td><td>{p["candidate"]:.1f}%</td></tr>'
                for label, row in (("Quiet · first", quiet), ("Working · first", working), ("Quiet · reverse", repeat_quiet), ("Working · reverse", repeat_working)) for p in row["pairs"])
fault_names = {
    "old-hidden-callbacks": "Restore 150 ms hidden ticks and the redundant hosted watcher",
    "lost-visibility-wake": "Lose the wake when a hidden pane becomes visible",
    "lost-resize-wake": "Lose the wake for a newly staged resize",
    "lost-completion-deadline": "Make a completed turn wait for the slow fallback",
    "completion-ignores-scroll-settle": "Allow a completion bell while scrolling",
}
faults = "".join(f'<tr><td>{fault_names[m["mutation"]]}</td><td class="good">Assertion failed</td></tr>' for m in mutations)
body = Template(r'''
<main>
<header><p class="eyebrow">TERMINAL DELIGHT · $PHASE</p>
<h1>Hidden panes wake less often.<br>CPU savings remain uncertain.</h1>
<p class="lead">Terminal Delight keeps your agent terminals running when their panes are off screen. We reduced how often those hidden panes check their status. The behavior tests confirm fewer callbacks; the CPU measurements show a small quiet saving and no clear benefit while agents print.</p></header>
<section><h2>Hidden panes were checking even when nothing changed.</h2>
<p>Each pane had a timer for routine status checks and pending window-size changes. It woke every 150 ms while hidden. A second 800 ms timer also woke in panes whose programs belong to TD’s separate terminal host, even though those panes had no local process to inspect.</p>
<figure><span class="lbl">01 · One hidden pane, two clocks</span>
<div class="scroller"><svg viewBox="0 0 760 220" role="img" aria-label="Nominal per-pane timer periods before and after, with one second between hidden checks">
<text x="22" y="34" class="ink">Routine housekeeping</text><text x="22" y="69" class="ink2">Before · every 150 ms</text>
<path d="M235 62H705" class="wire"/>
<g class="struct"><circle cx="260" cy="62" r="6"/><circle cx="326" cy="62" r="6"/><circle cx="392" cy="62" r="6"/><circle cx="458" cy="62" r="6"/><circle cx="524" cy="62" r="6"/><circle cx="590" cy="62" r="6"/><circle cx="656" cy="62" r="6"/></g>
<text x="22" y="110" class="ink2">After · every 1 s</text><path d="M235 103H705" class="wire"/><circle cx="690" cy="103" r="7" class="ready"/>
<text x="235" y="143" class="ink2">85% fewer scheduled callbacks at rest</text>
<text x="22" y="184" class="ink">Local process watcher</text><text x="310" y="184" class="ink2">800 ms → absent for hosted panes</text>
</svg></div><figcaption><b>A hosted pane no longer asks a local watcher a question only its host can answer.</b> The periods are drawn from the code; resize and completion deadlines can interrupt the quiet period.</figcaption></figure>
<p>Visible panes retain their ordinary 150 ms clock and their 33 ms CRT clock. Keyboard focus does not decide whether a pane is visible.</p></section>
<section><h2>We gave real events their own wakeups.</h2>
<p>The routine timer also delivers resizes and recognizes when an agent finishes. Slowing it without preserving those deadlines could leave a new tab asleep or delay a resize. The change therefore wakes immediately on pane activation and waits only until the next real deadline.</p>
<figure><span class="lbl">02 · Work continues while housekeeping sleeps</span>
<div class="flow"><div class="node"><b>Agent + terminal host</b><span>Process keeps running</span></div><div class="node"><b>Terminal event pump</b><span>Output and colour replies continue</span></div><div class="node ready"><b>Hidden pane</b><span>Parsed screen stays current</span></div></div>
<div class="deadlines"><span>Become visible → wake now</span><span>Hidden status fallback · 1 s</span><span>Resize settle · 140 ms</span><span>Detected stop debounce · 300 ms</span></div>
<figcaption><b>The sleep applies to housekeeping.</b> Output does not wait on that clock. A real stopped turn receives a separate 300 ms debounce deadline after the status scan detects it; scroll-settle still guards the bell.</figcaption></figure>
<p>The real-pane tests answered a hidden colour query without advancing housekeeping, detected a hidden permission prompt, preserved the latest resize during a drag, and returned to the fast visible clock. The native GUI check switched tabs, displayed fresh streamed output, and reconnected to the old host with both test terminals intact.</p></section>
<section><h2>That removed callbacks, but the CPU saving stayed small.</h2>
<p>We then compared normal production binaries with thirty hidden test programs, first quiet and then printing Codex-style working status twenty times per second. In the first run, window CPU was <b>$QUIET_CHANGE while quiet</b> and <b>$WORK_CHANGE while printing</b>. The baseline already includes reel removal.</p>
<figure><span class="lbl">03 · Measured window CPU</span><div class="scroller">$CPU_CHART</div>
<figcaption><b>Both launch orders are retained.</b> Each run has five simultaneous ten-second pairs per condition; bars show medians in percent of one logical CPU. The visible pane was quiet, CRT was off, and both windows measured 1000 × 700 on the same monitor.</figcaption></figure>
<p>The observed median differences were $QUIET_POINTS percentage points of one CPU while quiet and $WORK_POINTS while working. The identical-binary control differed by up to $CONTROL_QUIET and $CONTROL_WORK points respectively. Whole-machine speed, battery life and input-latency changes remain unmeasured.</p>
<p>Reversing launch order gave <b>$REPEAT_QUIET_CHANGE while quiet</b> and <b>$REPEAT_WORK_CHANGE while working</b>. Both complete runs are shown above; none of their twenty pairs was excluded.</p>
<button data-open="d-evidence">Open the raw pairs and method</button></section>
<section><h2>We kept the bounded change after trying to break it.</h2>
<p>A small or uncertain CPU gain calls for a limited change. Hidden status checks retain their one-second fallback; agent execution and output stay live. Before shipping, we deliberately removed each protection to see whether the tests would catch the resulting fault.</p>
<figure><span class="lbl">04 · Faults the tests caught</span><div class="scroller"><table class="wide"><thead><tr><th>Production fault inserted</th><th>Result</th></tr></thead><tbody>$FAULTS</tbody></table></div>
<figcaption><b>All five fault variants failed their intended behavioral assertions.</b> Source bytes were restored after every variant. The restored application passed $DEFAULT_PASS tests on the default core and $FALLBACK_PASS on the fallback core, plus strict Clippy and formatting.</figcaption></figure>
<div class="ask shipped-choice"><h3>Implemented: bounded hidden sleep</h3><p>Hidden housekeeping keeps a one-second fallback. Pane activation and resize changes wake it. Agent execution and the terminal byte pump continue independently.</p></div>
<p class="callout">$INSTALLATION</p>
<p>The measurements do not justify making status updates slower than this. Further CPU work needs to identify where the remaining time is spent.</p>
<p><a href="$PR">Read the change and its source</a> · <a href="2026-10-17-pane-sleep-evidence/summary.json">Measured results</a> · <a href="2026-10-17-pane-sleep-evidence/validation.json">Validation record</a></p>
</section></main>
<dialog id="d-evidence"><div class="dlg-head"><h3>Pairs, controls and limits</h3><button class="x" data-close>×</button></div><div class="dlg-body">
<p>Each build ran in its own process with thirty hidden synthetic Codex workers and one visible quiet pane. Both windows shared one private hidden workspace, monitor and size. Both hosts used the same production baseline binary. The native compatibility check separately used the executable of the still-running older host.</p>
<p>All five quiet pairs precede the five working pairs: pausing a worker leaves its old status footer on screen, so alternating conditions would mislabel the quiet case. Both builds were sampled simultaneously. Five seconds of settling preceded each ten-second observation; working output rates had to match within 5%. Startup, executable identity, geometry and stream counts were checked before accepting a pair. No CPU pair was excluded.</p>
<p>Both normal production binaries were compiled sequentially from the same checkout path, with no test features. Before comparing them, two identical-baseline-binary pairs per condition measured control variation: the largest absolute window difference was $CONTROL_QUIET CPU percentage points while quiet and $CONTROL_WORK while working. This small control describes the observed variation; it is not a statistical confidence interval.</p>
<p>Every working interval also read the parsed counter from all thirty hidden panes after sampling. Fresh writer output alone would not prove that the terminal still processes it.</p>
<table class="wide"><thead><tr><th>Condition</th><th>Pair</th><th>Before</th><th>After</th></tr></thead><tbody>$PAIRS</tbody></table>
<p>The result is a controlled window-process measurement. It does not measure a whole-machine speedup or a cumulative improvement over every earlier TD fix. Earlier percentages cannot be added to these. Timer periods and test deadlines describe scheduling rules, not a measured keyboard-latency distribution.</p>
<p>The first full suite failed: placing the new test-module declaration at the top cut existing source guards short, producing22 failures. Moving it into the established bottom test block restored those guards. Eight other source readers now recognize the top-level test boundary rather than an indented test-only field; their assertions remain unchanged. A passing unmodified routing guard and a deliberately rejected F1 bypass verify that repair. The first F1 control was discarded because it hit the reader cutoff instead of the routing assertion; the accepted control requires the specific routing assertion. The same run hit the unchanged <a href="https://github.com/parker-brown-family/terminal-delight/issues/933#issuecomment-6246691310">known intermittent host identity failure</a>. Its isolated result and the fresh full-suite results are retained beside the original failure; the fork/exec explanation remains inferred.</p>
<p>Rejected fixture preflights remain in the evidence directory. They caught an incomplete theme, exec-replaced shell classification, unfinished startup and unequal monitor geometry before sampling. The separate <a href="https://github.com/parker-brown-family/terminal-delight/issues/991">exec classification follow-up</a> carries a reproduction and invalidation test.</p>
<p><a href="2026-10-17-pane-sleep-evidence/samples.jsonl">Every CPU observation</a> · <a href="2026-10-17-pane-sleep-evidence/mutations.json">Fault-control records</a> · <a href="2026-10-17-pane-sleep-evidence/delivery.json">Live delivery proof</a></p>
</div></dialog>
''').substitute(
    QUIET_CHANGE=change(quiet), WORK_CHANGE=change(working), CPU_CHART=chart(),
    REPEAT_QUIET_CHANGE=change(repeat_quiet), REPEAT_WORK_CHANGE=change(repeat_working),
    QUIET_PAIR=quiet["pairs_with_lower_window_cpu"], WORK_PAIR=working["pairs_with_lower_window_cpu"],
    QUIET_POINTS=f'{quiet["saved_core_percentage_points"]:.1f}', WORK_POINTS=f'{working["saved_core_percentage_points"]:.1f}',
    FAULTS=faults, DEFAULT_PASS=checks["default"]["passed"], FALLBACK_PASS=checks["fallback"]["passed"],
    PHASE="VALIDATED CANDIDATE · INSTALLATION PENDING" if preview else "SHIPPED PERFORMANCE WORK",
    INSTALLATION=html.escape(installation), PR="https://github.com/parker-brown-family/terminal-delight/pull/993" if preview else html.escape(delivery["pr_url"]), PAIRS=pairs,
    CONTROL_QUIET=f'{control["conditions"]["idle"]["largest_absolute_core_point_difference"]:.1f}',
    CONTROL_WORK=f'{control["conditions"]["working"]["largest_absolute_core_point_difference"]:.1f}',
)
css = """
main{max-width:940px;margin:auto;padding:54px 26px 100px}header{margin-bottom:44px}h1{font-size:clamp(38px,6vw,65px);line-height:1.05;max-width:800px}h2{font-size:27px;margin-top:48px}.lead{font-size:21px;line-height:1.55;max-width:800px}p{line-height:1.65}.eyebrow{font:12px var(--mono);letter-spacing:.14em;color:var(--acc)}figure > .scroller > svg text{font:16px ui-monospace,monospace}figure > .scroller > svg{display:block;min-width:660px}.flow{display:grid;grid-template-columns:repeat(3,1fr);gap:14px;padding:20px 0}.node{position:relative;border:1px solid var(--s1);border-radius:10px;background:color-mix(in srgb,var(--s1) 12%,transparent);padding:20px}.node:not(:last-child)::after{content:"→";position:absolute;right:-17px;top:40%;color:var(--s1)}.node b,.node span{display:block}.node span{font-size:14px;margin-top:9px;color:var(--dim)}.node.ready{border-color:var(--good)}.deadlines{display:flex;gap:12px;flex-wrap:wrap;font:13px var(--mono);padding:0 0 20px}.deadlines span{padding:10px;border:1px solid var(--line);border-radius:8px}.good{color:var(--good)}table{width:100%;border-collapse:collapse}td,th{text-align:left;padding:12px;border-bottom:1px solid var(--line)}button{cursor:pointer}.shipped-choice{margin-top:25px;padding:20px 55px 20px 22px;border:1px solid var(--good);border-radius:12px}dialog{max-width:min(850px,94vw)}.callout{margin-top:24px}@media(max-width:600px){main{padding:30px 17px 90px}.flow{grid-template-columns:1fr}.node:not(:last-child)::after{content:"↓";right:48%;top:100%}td,th{padding:9px;font-size:14px}.lead{font-size:19px}}
"""
base = (SKILL / "assets/base.css").read_text()
wall = (OUT / "brief-wall.html").read_text()
notes_css = (SKILL / "assets/notes.css").read_text()
notes_markup = (SKILL / "reference/notes-markup.html").read_text().replace("<YYYY-MM-DD>-<topic-slug>.html", REPORT.name)
notes_js = (SKILL / "assets/notes.js").read_text().replace("</script", "<\\/script")
modal_js = "document.querySelectorAll('[data-open]').forEach(b=>b.onclick=()=>document.getElementById(b.dataset.open).showModal());document.querySelectorAll('[data-close]').forEach(b=>b.addEventListener('click',()=>b.closest('dialog').close()));"
page = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>Hidden pane housekeeping — measured delivery</title><style>' + base + '</style>' + wall + '<style>' + css + '</style><style>' + notes_css + '</style><body>' + body + notes_markup + '<script>' + notes_js + '</script><script>' + modal_js + '</script></body></html>'
REPORT.write_text(page)
print(REPORT)
