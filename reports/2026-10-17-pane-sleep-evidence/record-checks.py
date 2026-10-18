"""Summarize complete raw Cargo runs; an absent native result stays unknown."""
import hashlib
import json
import re
from pathlib import Path

out = Path(__file__).resolve().parent
result = {}
for core in ("default", "fallback"):
    log = out / ("suite-" + core + ".log")
    text = log.read_text()
    rows = re.findall(r"test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;", text)
    assert len(rows) == 7 and all(r[0] == "ok" for r in rows), (core, rows)
    result[core] = {"passed": sum(int(r[1]) for r in rows),
                    "failed": sum(int(r[2]) for r in rows),
                    "ignored": sum(int(r[3]) for r in rows),
                    "log_sha256": hashlib.sha256(log.read_bytes()).hexdigest()}
    assert result[core]["failed"] == 0
    clippy = (out / ("clippy-" + core + ".log")).read_text()
    assert "Finished" in clippy and "error:" not in clippy
    result["clippy_" + core] = True
assert (out / "format.log").read_text() == ""
result["format"] = True
assert "advisories ok, bans ok, licenses ok, sources ok" in (out / "deny.log").read_text()
result["cargo_deny"] = True
assert (out / "js-syntax.log").read_text() == ""
result["javascript_syntax"] = True
smoke = out / "native-smoke.log"
result["native_smoke"] = None if not smoke.exists() else "PASS: new GUI with old host" in smoke.read_text()
result["native_smoke_preflight_used_baseline"] = True
result["initial_full_failures_retained"] = [
    "full-first-source-cutoff-and-identity.log", "full-second-indented-cfg-cutoff.log"]
result["host_identity_failure_followup"] = "https://github.com/parker-brown-family/terminal-delight/issues/933#issuecomment-6246691310"
(out / "validation.json").write_text(json.dumps(result, indent=2) + "\n")
print(json.dumps(result, indent=2))
