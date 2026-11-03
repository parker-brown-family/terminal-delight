#!/usr/bin/env python3
"""Build matching readable reports; keep local reader annotations private."""
import argparse
from datetime import datetime, timezone
import hashlib
import html
import json
from pathlib import Path
import re

p = argparse.ArgumentParser()
p.add_argument("--input", type=Path, required=True)
p.add_argument("--local", type=Path, required=True)
a = p.parse_args()
root = Path(__file__).resolve().parent
data = json.loads(a.input.read_text())
original = (root / "history-initial/repository-report.html").read_text()
body = (root / "report-body.html").read_text()
for token, value in {
    "DEFAULT": data["default"]["passed"], "DEFAULT_IGNORED": data["default"]["ignored"],
    "FALLBACK": data["fallback"]["passed"], "FALLBACK_IGNORED": data["fallback"]["ignored"],
    "MUTATIONS": data["mutations"], "CI_LINK": data["ci_link"],
    "SOURCE": data["source_fingerprint"], "BINARY": data["binary_sha256"],
    "RECEIPT_LINK": data["receipt_link"], "INSTALL": data["install_command"],
    "CANDIDATE_SOURCE": data["candidate_source"], "CANDIDATE_BINARY": data["candidate_binary"],
    "CANDIDATE_DEFAULT": data["candidate_default"], "CANDIDATE_FALLBACK": data["candidate_fallback"],
}.items():
    body = body.replace("@@" + token + "@@", html.escape(str(value), quote=True))
assert "@@" not in body, "unfilled report field"
body = body.replace('07 · One proposal', '06 · One proposal').replace('06 · Actual evidence', '07 · Actual evidence')
body = body.replace('Development is integrated; installation remains your choice', 'Qualified for development; installation remains your choice')
body = body.replace('Merged into 0.5 development.', 'One review completion for 0.5 development.')
head = original[:original.index("<main>")]
head = re.sub(r"<title>.*?</title>", "<title>Restricted commands</title>", head)
style = '''<style>
main{max-width:1060px;margin:auto;padding:36px 24px 110px}section{margin:38px 0}.lead{font-size:1.15rem;line-height:1.6}
.review-grid{display:grid;gap:14px;margin:14px 0}.review-grid.three{grid-template-columns:repeat(3,minmax(0,1fr))}.review-grid.two{grid-template-columns:repeat(2,minmax(0,1fr))}
.review-box{border:1px solid;border-radius:9px;padding:17px}.review-box p{margin:9px 0 0;line-height:1.55}.review-box ol{padding-left:22px;line-height:1.8}
.review-box.ready,.scope-tree .ready{border-color:var(--s3);background:color-mix(in srgb,var(--s3) 9%,transparent)}
.review-box.struct{border-color:var(--s1);background:color-mix(in srgb,var(--s1) 9%,transparent)}
.review-box.fail,.scope-tree .fail{border-color:var(--serious);background:color-mix(in srgb,var(--serious) 9%,transparent)}
.review-box.unk{border-color:var(--dim);border-style:dashed;color:var(--dim)}
td.ready{color:var(--s3)}td.fail{color:var(--serious)}.ask{border-left:3px solid var(--critical);padding-left:14px}
.scope-tree{display:grid;gap:9px}.scope-tree>div{padding:11px;border:1px solid;border-radius:7px}.scope-tree code{overflow-wrap:anywhere}
svg.review-flow{display:block;min-width:650px;font:13px ui-monospace,monospace}.review-flow text{font:13px ui-monospace,monospace}.review-terminal{background:#090c0a;color:#e7ede5;padding:20px;border-radius:7px;font:14px/1.6 ui-monospace,monospace}.review-terminal p{font:inherit}.review-terminal code{color:inherit}
table.wide{width:100%;table-layout:fixed}table.wide th:first-child{width:72%}table.wide td,table.wide th{overflow-wrap:anywhere}pre,details code{white-space:pre-wrap;overflow-wrap:anywhere}svg{max-width:100%}
@media(max-width:650px){main{padding:22px 16px 110px}.review-grid.three,.review-grid.two{grid-template-columns:1fr}.review-terminal{font-size:12px}.review-box{padding:14px}}
</style>'''
head = head.replace("</head>", style + "</head>")
footer = original[original.index("</main>") + len("</main>"):]
footer = re.sub(r"window.NOTES_FILE\s*=\s*'[^']*'", "window.NOTES_FILE = '2026-10-21-restricted-security-review.html'", footer)
public = head + body + footer
local = public
preserved = {}
private_maps = {}
revision = datetime.now(timezone.utc).isoformat()
if a.local.exists():
    before = a.local.read_text()
    for island in ["report-notes", "report-concurs"]:
        pattern = r'(<script[^>]*id="' + island + r'"[^>]*>)(.*?)(</script>)'
        found = re.search(pattern, before, re.S)
        if found:
            values = json.loads(found.group(2))
            # Keep the reader's evidence-corpus question on that same figure,
            # whose number moves when the proposal now precedes the evidence.
            old, new = "fig-06-actual-evidence-and", "fig-07-actual-evidence-and"
            if old in values:
                values[new] = values.pop(old)
                if isinstance(values[new], list):
                    for note in values[new]:
                        note["title"] = "07 · Actual evidence and limits"
            preserved[island] = len(values)
            private_maps[island] = values
            encoded = json.dumps(values, ensure_ascii=False, indent=1).replace("</", "<\\/").replace("<!", "\\u003c!")
            def replace_island(match):
                opening = re.sub(r'\sdata-rev="[^"]*"', '', match.group(1))
                opening = opening[:-1] + ' data-rev="' + revision + '">'
                return opening + encoded + match.group(3)
            local = re.sub(pattern, replace_island, local, count=1, flags=re.S)
if private_maps:
    notes = private_maps.get("report-notes", {})
    concurs = private_maps.get("report-concurs", {})
    count = sum(len(entries) for entries in notes.values())
    lines = ["\nREADER NOTES —", "NOTES — 2026-10-21-restricted-security-review.html",
             f"{count} notes on {len(notes)} elements.", "Each [anchor] is an element id in that file — search it to find the passage.", ""]
    for anchor in sorted(set(notes) | set(concurs)):
        entries = notes.get(anchor, [])
        title = entries[0].get("title", anchor) if entries else anchor
        lines.append(f"[{anchor}] {title}" + ("  ✓ concur" if anchor in concurs else ""))
        lines.extend("  · " + entry["text"] for entry in entries)
        lines.append("")
    mirror = "\n".join(lines).replace("--!>", "--! >").replace("-->", "-- >")
    # Only the real terminal mirror, never strings in notes.js.
    local = re.sub(r'<!--\s*READER NOTES —.*?-->', '', local, flags=re.S)
    local = local.replace("</body>", "<!--" + mirror + "\n--></body>")
public_path = root.parent / "2026-10-21-restricted-commands.html"
public_path.write_text(public)
a.local.write_text(local)
assert re.search(r"<main>.*?</main>", public, re.S).group() == re.search(r"<main>.*?</main>", local, re.S).group()
print(json.dumps({"main_content_sha256": hashlib.sha256(body.encode()).hexdigest(), "matching_reading": True,
    "public": str(public_path), "local": str(a.local), "private_annotation_anchors_preserved": preserved}, indent=2))
