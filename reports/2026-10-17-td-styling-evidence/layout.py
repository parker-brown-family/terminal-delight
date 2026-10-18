import sys,json
from pathlib import Path
session,out,script=sys.argv[1:]
header='active = 0\nagent_tint = false\nanchor_top = false\nleft_bar = true\nscale = 0.85\nrail_pinned = false\n[mcp]\nenabled = true\nexpose = "all"\nwritable = true\n[[tabs]]\n'
rows=[header]
def branch(path,direction):rows.extend([f'[{path}.Split]',f'dir = "{direction}"','ratio = 0.5'])
def leaf(path,num,kind):
 rows.extend([f'[{path}.Leaf]','cwd = "/tmp"',f'pane_id = {num}',f'name = "{kind} {num}"','resume = '+json.dumps(f'python3 {script} {kind}'),f'[{path}.Leaf.appearance]','pins = ["crt"]',f'[{path}.Leaf.appearance.grade]','crt = false'])
branch('tabs.node','Row');branch('tabs.node.Split.a','Col');branch('tabs.node.Split.b','Col')
for path,n,kind in [('tabs.node.Split.a.Split.a',1,'busy'),('tabs.node.Split.a.Split.b',2,'quiet'),('tabs.node.Split.b.Split.a',3,'quiet'),('tabs.node.Split.b.Split.b',4,'quiet')]:leaf(path,n,kind)
Path(out).write_text('\n'.join(rows)+'\n')
