from pathlib import Path
import json,subprocess,base64,hashlib,gzip,re,shutil
R=Path('/home/parker/Work/td-chalk-flower');E=R/'reports/2026-10-20-chalk-marker-evidence';E.mkdir(exist_ok=True)
scenes={'before':'/tmp/chalkmarker-baseline-landscape','current':'/tmp/chalkmarker-final-current-tray','workspace':'/tmp/chalkmarker-final-current-workspace','light':'/tmp/chalkmarker-final-light-tray','queue':'/tmp/chalkmarker-final-current-rail','light-workspace':'/tmp/chalkmarker-final-light-workspace','paint':'/tmp/chalkmarker-final-current-paint','warp':'/tmp/chalkmarker-final-warped-workspace','usage':'/tmp/chalkmarker-final-current-usage','usage-light':'/tmp/chalkmarker-final-light-usage'}
proof=json.loads(Path('/tmp/chalk-marker-manifest-final.json').read_text());proof['captures']={};proof['tests']={}
for name,folder in scenes.items():
 source=Path(folder)/'native.png';dest=E/f'{name}.webp'
 subprocess.run(['magick',str(source),'-define','webp:lossless=true',str(dest)],check=True)
 diff=subprocess.run(['magick','compare','-metric','AE',str(source),str(dest),'null:'],capture_output=True,text=True);assert diff.returncode==0,diff.stderr
 proof['captures'][name]={'native_png_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'webp_sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),'pixel_difference_ae':diff.stderr.strip()}
 if name!='before':
  state=Path(folder)/'restored-state.toml'
  import tomllib
  data=tomllib.loads(state.read_text());(E/f'{name}-profile.json').write_text(json.dumps({k:data.get(k) for k in ['skin','theme','win']},indent=2))
for key,log in [('rio','rio'),('alacritty','alacritty'),('clippy','clippy'),('release','release')]:
 b=Path('/tmp/chalk-marker-'+log+'-v8.log').read_bytes();(E/f'{key}.log.gz').write_bytes(gzip.compress(b,mtime=0))
 if key in ['rio','alacritty']:
  results=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored',b.decode());assert len(results)==7,(key,results)
  proof['tests'][key]={k:sum(int(row[i]) for row in results) for i,k in enumerate(['passed','failed','ignored'])}
(E/'proof.json').write_text(json.dumps(proof,indent=2))
# Keep the working recipe, including the explicit landscape geometry check.
s=Path('/tmp/chalk-marker-capture.sh').read_text().replace('ROOT=/home/parker/Work/td-chalk-flower','ROOT=$(git -C "$(dirname "${BASH_SOURCE[0]}")" rev-parse --show-toplevel)').replace('"/home/parker/Work/td-chalk-flower/reports/2026-10-19-chalk-surface-v02-evidence/emitter.py"','"$(dirname "${BASH_SOURCE[0]}")/emitter.py"')
(E/'capture.sh').write_text(s);shutil.copy2(R/'reports/2026-10-19-chalk-surface-v02-evidence/emitter.py',E/'emitter.py')
old=(R/'reports/2026-10-19-chalk-surface-second-pass.html').read_text();head=old[:old.index('<body>')].replace('Chalk — second native pass','Chalk — marker and depth')
notes=old[old.index('<div class="notebar"'):old.index('<dialog id="capture-zoom">')]
notes=notes.replace('2026-10-19-chalk-surface-second-pass.html','2026-10-20-chalk-marker-and-depth.html')
zoom=old[old.index('<dialog id="capture-zoom">'):]
# Inherit only the trusted notes engine, never someone else's saved annotations.
notes=re.sub(r'(<script type="application/json" id="report-notes"[^>]*>).*?(</script>)',r'\1{}\2',notes,flags=re.S)
notes=re.sub(r'(<script type="application/json" id="report-concurs"[^>]*>).*?(</script>)',r'\1{}\2',notes,flags=re.S)
def fig(name,n,title,claim,crop=None,id=None):
 data='data:image/webp;base64,'+base64.b64encode((E/f'{name}.webp').read_bytes()).decode()
 if crop:
  view=' '.join(map(str,crop));pic=f'<svg class="capture" viewBox="{view}" aria-label="{title}"><image href="{data}" width="2522" height="1520"/></svg>'
 else:pic=f'<img class="capture" src="{data}" alt="{title}">'
 return f'<figure id="{id or "fig-"+name}"><span class="lbl">{n:02} · {title}</span>{pic}<figcaption>{claim}</figcaption></figure>'
counts=proof['tests']
body='''<body><main><header><div class="eyebrow">Terminal Delight · third native pass · 20 October 2026</div><h1>Chalk · marker and depth</h1><p class="lead">The selection stroke now crosses the middle of the writing. Labels fit their controls, and the paper has a compressed edge and a soft cast shadow. This review includes your latest correction: one highlighter stroke, plus the missing Subscriptions surface.</p></header><section><h2>Centre the writing and its mark</h2><p>Your review found two linked problems: the marker underlined the word, and the larger handwriting overflowed the old tiles. Compact line spacing now fits the handwriting. The words sit in the middle of their hit targets, with small glyph stamps above them. The solid selector tiles are removed. Each caption carries one broad, translucent highlighter stroke. The rejected second line and flecks are removed; the mark extends beyond the writing like the agent-state counter.</p>'''
body+=fig('current',1,'After · the same saved theme','Hacker, tactical and agentic fit. Selector options have no filled tile behind them; only the selected word gets a broad highlighter stroke.',(1720,330,710,1050),id='fig-current')
body+='<details><summary>Previous lower-edge trial</summary>'+fig('before',2,'Before · the saved Last Horizon theme','The previous build puts the mark at the lower edge and leaves too little room beneath the captions.',(1720,330,710,1050))+'</details>'
body+=fig('workspace',3,'Tree and pane tabs','The selected tree row and TERM label use the same uneven centre stroke, with visual overhang beyond their hit targets.',(20,65,1370,600))
body+='''</section><section><h2>Give the paper thickness</h2><p>Once the labels fit, your cotton-paper reference sets the next requirement: visible depth. Controls now cast a soft shadow beneath a compressed cotton edge. Regions carry the same edge lighting, while the shared grain adds fibre relief. The pale palette makes those layers easier to judge.</p>'''
body+=fig('light',4,'Pale stock · palette, buttons and rules','The paper sits above its surroundings. Highlights and contact shadows supply depth without changing the palette or adding a bright frame.',(1380,330,1050,1050))
target=base64.b64encode((R/'docs/plans/chalk-flower/feedback-v03/13.png').read_bytes()).decode()
body+='<figure id="fig-reference"><span class="lbl">05 · Your cotton-stock reference</span><img style="display:block;max-width:375px;width:100%;margin:auto" src="data:image/png;base64,'+target+'" alt="Stacked cotton paper with soft cast shadows and irregular fibrous edges"><figcaption>Your photograph sets the depth target: visible stock thickness, soft shadows and fibrous edges. This is the supplied reference, not a Terminal Delight capture.</figcaption></figure>'
body+=fig('queue',6,'Attention spine and queue','The queue, spine and response controls use the same stock depth. The staged cases are fictional.',(1815,100,700,1330))
body+=fig('paint',7,'Paint cards','The selection is attached to the name area of the card. Paper relief also reaches the palette tiles.',(600,100,1900,1100))
body+='''</section><section><h2>Bring Subscriptions onto the same paper</h2><p>Your latest screenshot caught a separate modal shell that still drew the old flat background. Subscriptions and Savings now share the Chalk material: textured stock, a cotton edge and soft contact shadow. The chips, launch configuration and meter fills also follow the material. These two captures use fictional subscription records.</p>'''
body+=fig('usage',8,'Subscriptions · dark stock','The modal now has the same grain and edge as the cabinet. Provider and view choices have no filled backing; their selected words get one highlighter stroke.',(400,270,1700,1000))
body+=fig('usage-light',9,'Subscriptions · pale stock','The same shell follows the active theme, including pale paper. The numbers are staged, not this account’s usage.',(400,270,1700,1000))
body+='''</section><section><h2>Carry this into the next review</h2><p>The same material still follows the active theme. These final windows check pale terminal stock and the existing CRT warp. Your approved bottom-bar controls and messy marker character remain the reference for the next pass.</p>'''
body+=fig('light-workspace',10,'Pale terminal stock','The backing, pane rims and surrounding cabinet share the cotton material. Terminal type stays monospace.')
body+=fig('warp',11,'Last Horizon · CRT warp','The terminal stock remains inside its existing warp. The cabinet and its raised paper controls stay separate.')
body+='''<div class="ask" id="ask-pigment"><h3>Marker pigment</h3><p>Current choice: Complement blended 40% toward Human, drawn once as a broad translucent stroke with uneven ends. It remains distinct from the live-state colour in the built-in palettes.</p></div><div class="ask" id="ask-depth"><h3>Paper depth</h3><p>Current choice: a compressed lower edge, lit fibres and two soft contact-shadow layers. Annotate a capture if the stock should feel thicker, softer or less raised.</p></div><p>Your numbered screenshots, exact feedback and each corrective step are retained in the <a href="../docs/plans/chalk-flower/03-marker-and-depth.md">third-pass record</a>. Notes on these figures can start the next pass from the same surface.</p></section><details><summary>Evidence and source</summary><p>Native fixtures: 1576 × 950 logical pixels on the 1.6× landscape display. Current-theme scenes copy the saved Last Horizon palette and display grade; pale scenes use Flexoki Light with a neutral grade. Terminal output and attention cases are staged. WebP storage is lossless, checked pixel-for-pixel against each original native PNG.</p>'''
body+=f'<p>Rio: {counts["rio"]["passed"]:,} passed; Alacritty: {counts["alacritty"]["passed"]:,} passed. Each has {counts["rio"]["ignored"]} ignored tests. Strict Clippy, formatting and release build passed. Logs, source hashes and the reusable native recipe live in <code>reports/2026-10-20-chalk-marker-evidence/</code>.</p>'
body+='''<p>The renderer is in <code>app/src/paper.rs</code>; caption layout and material integration are in <code>skin.rs</code>, <code>main.rs</code> and <code>paint.rs</code>. The verified build is installed for the next GUI launch. The running GUI and session host were left untouched; reopen TD to load the update. Your Chalk selection is already saved.</p></details></main>'''
(R/'reports/2026-10-20-chalk-marker-and-depth.html').write_text(head+body+notes+zoom)
print(json.dumps(counts));print('review assembled')
