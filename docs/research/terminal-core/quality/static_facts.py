"""Static facts about the code behind rio-vt and libghostty-vt, counted the same way for both.

    python3 docs/research/terminal-core/quality/static_facts.py → quality/static-facts.json

rio-vt is one crate, and its source is what TD would link. libghostty-vt is two layers:
Ghostty's terminal core in Zig (src/terminal at the commit libghostty-vt-sys 0.2.1 pins) and
the third-party Rust bindings over its C API (crate libghostty-vt 0.2.1). Both layers are
counted, separately, because a surprise can come from either.
"""
import glob
import json
import re
from pathlib import Path

HOME = Path.home()
RIO = Path(glob.glob(str(HOME / ".cargo/registry/src/*/rio-vt-0.5.28"))[0])
BIND = Path(glob.glob(str(HOME / ".cargo/registry/src/*/libghostty-vt-0.2.1"))[0])
GH = Path(glob.glob(str(HOME / ".cache/td-core-research/targets/ghostbake/release/build/libghostty-vt-sys-*/out/ghostty-src"))[0])


def lines(files):
    n = 0
    for f in files:
        n += sum(1 for l in Path(f).read_text(errors="replace").splitlines() if l.strip())
    return n


def grep_count(files, rx):
    r = re.compile(rx)
    return sum(len(r.findall(Path(f).read_text(errors="replace"))) for f in files)


def rust_crate(root):
    src = glob.glob(str(root / "src/**/*.rs"), recursive=True)
    tests = glob.glob(str(root / "tests/**/*.rs"), recursive=True)
    return {
        "source_files": len(src), "source_lines": lines(src),
        "test_fns_in_src": grep_count(src, r"#\[test\]"), "test_files": len(tests), "test_fns_in_tests": grep_count(tests, r"#\[test\]"),
        "examples": len(glob.glob(str(root / "examples/*.rs"))), "benches": len(glob.glob(str(root / "benches/*.rs"))),
        "unsafe_blocks_or_fns": grep_count(src, r"\bunsafe\b"),
        "unwrap_or_expect_calls": grep_count(src, r"\.(unwrap|expect)\("),
        "doc_comment_lines": grep_count(src, r"(?m)^\s*///"),
        "todo_fixme": grep_count(src, r"\b(TODO|FIXME|XXX)\b"),
    }


term = glob.glob(str(GH / "src/terminal/**/*.zig"), recursive=True)
capi = glob.glob(str(GH / "include/ghostty/vt/**/*.h"), recursive=True) + glob.glob(str(GH / "include/ghostty/vt.h"))
fuzz = sorted(p.name for p in (GH / "test").glob("*")) if (GH / "test").exists() else []
out = {
    "rio-vt 0.5.28 (crate TD would link)": rust_crate(RIO),
    "libghostty-vt 0.2.1 (third-party Rust bindings)": rust_crate(BIND),
    "ghostty src/terminal (the Zig core under the bindings)": {
        "source_files": len(term), "source_lines": lines(term),
        "zig_test_blocks": grep_count(term, r'(?m)^\s*test\s+"'),
        "doc_comment_lines": grep_count(term, r"(?m)^\s*///"),
        "todo_fixme": grep_count(term, r"\b(TODO|FIXME|XXX)\b"),
        "c_api_header_lines": lines(capi), "c_api_doc_comment_lines": grep_count(capi, r"(?m)^\s*(\*|/\*\*|///)"),
        "test_dir_entries": fuzz,
    },
}
dest = Path(__file__).parent / "static-facts.json"
dest.write_text(json.dumps(out, indent=1))
for k, v in out.items():
    print(k)
    for kk, vv in v.items():
        print(f"   {kk}: {vv}")
