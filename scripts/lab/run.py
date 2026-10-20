#!/usr/bin/env python3
"""The Terminal Lab's correctness lane: replay byte cases through both VT
cores, find where the screen depends on how bytes arrived or on which core
parsed them, and shrink each finding to a reproducer a person can read.

The harness itself is Rust (`app/src/vt/lab.rs`, test-only). This script
writes corpora, drives that harness's two test binaries — one per core — and
does everything that compares runs: chunk invariance within a core,
agreement between the cores, minimization, the planted-fault controls, and
the summary a person reads.

Every finding is a pair of runs of one case that should agree and do not:

    chunk     the same core, whole writes against cut ones
    cores     rio-vt against alacritty, whole writes
    control   a planted fault against a clean run (proves detection)

plus single runs that crash, hang, or break a named case's expectation.

Commands:

    run.py corpus  --out corpus.json [--cases N] [--seed-base S]
    run.py run     --out DIR [--shard I/N] [--cases N] [--seed-base S] [--no-record]
    run.py control --out DIR
    run.py replay  CASE.json --a rio:whole --b alacritty:whole

Shard 1 also runs real programs (record.py) and replays what they printed,
read by read, as `rec-1/...` cases; each recording is compared whole against
its real read boundaries as well as against seeded cuts. Every run writes
coverage.json and a summary section saying what it could not check
(unknowns.py, catalogue.json).

Binaries come from TD_LAB_BIN_RIO / TD_LAB_BIN_ALACRITTY if set (CI builds
them once and hands them to every shard), and are built with cargo
otherwise. Standard library only.

Plan: docs/plans/terminal-lab/plan.md, the correctness lane.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shlex
import subprocess
import sys
import tempfile
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))
import unknowns  # noqa: E402

ROOT = Path(__file__).resolve().parents[2]
APP = ROOT / "app"
NAMED = APP / "src" / "vt" / "lab" / "named.json"
KNOWN = Path(__file__).resolve().parent / "known.json"

FORMAT_VERSION = 1
# Bumped whenever the generator would turn a seed into different bytes: a
# seed names a case only within one corpus version.
CORPUS_VERSION = "gen-1"
CORES = {"rio": [], "alacritty": ["--features", "core-alacritty"]}
# Two cut plans per core and case; with whole writes, three runs per core.
CHUNK_SEEDS = (11, 23)
ARTIFACT_CAP = 10 * 1024 * 1024
# The least time one harness process is given, on top of each case's own.
HARNESS_FLOOR_S = 60
# Limits mirrored from lab.rs, so a generated case is refused here rather
# than by the harness.
MAX_COLUMNS, MAX_ROWS, MAX_BYTES, MAX_OPS = 400, 200, 1 << 20, 4096


class SetupError(Exception):
    """The lab could not measure. Never a pass."""


# --- deterministic numbers ---------------------------------------------------

MASK = (1 << 64) - 1


class SplitMix:
    """splitmix64, identical to `SplitMix` in lab.rs."""

    def __init__(self, seed: int) -> None:
        self.state = seed & MASK

    def next(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def below(self, n: int) -> int:
        return self.next() % n

    def pick(self, items):
        return items[self.below(len(items))]


# --- generating cases ----------------------------------------------------------

WORDS = ["a", "ok", "hello", "terminal", "delight", "x" * 9, "0123456789", "~!@#", "  "]
WIDE = ["你", "好", "界", "🙂", "ｗ"]
SGR = ["0", "1", "2", "3", "4", "7", "9", "22", "24", "27", "31", "32", "44", "97",
       "38;5;202", "48;5;17", "38;2;10;200;30", "4:3", "58;5;9", ""]


def token(rng: SplitMix, columns: int, rows: int) -> str:
    """One piece of plausible terminal output. The weights lean on text,
    because escapes acting on an empty screen prove little."""
    n = lambda top: str(1 + rng.below(top))  # noqa: E731
    roll = rng.below(100)
    if roll < 30:
        return rng.pick(WORDS)
    if roll < 36:
        return rng.pick(WIDE)
    if roll < 39:
        return rng.pick(["é", "ạ̈", "‍"])
    if roll < 47:
        return rng.pick(["\r\n", "\n", "\r", "\t", "\b"])
    if roll < 55:
        return f"\x1b[{rng.pick(SGR)}m"
    choices = [
        lambda: f"\x1b[{n(rows + 2)};{n(columns + 2)}H",
        lambda: f"\x1b[{n(4)}{rng.pick('ABCD')}",
        lambda: f"\x1b[{n(columns + 2)}G",
        lambda: f"\x1b[{n(rows + 2)}d",
        lambda: f"\x1b[{rng.pick(['', '0', '1', '2'])}K",
        lambda: f"\x1b[{rng.pick(['', '0', '1', '2', '3'])}J",
        lambda: f"\x1b[{n(4)}{rng.pick('@PXLMST')}",
        lambda: f"\x1b[{n(rows)};{n(rows + 1)}r",
        lambda: "\x1b[r",
        lambda: rng.pick(["\x1b7", "\x1b8", "\x1bM", "\x1bD", "\x1bE"]),
        lambda: f"\x1b[?1049{rng.pick('hl')}",
        lambda: f"\x1b[?7{rng.pick('hl')}",
        lambda: f"\x1b[4{rng.pick('hl')}",
        lambda: f"\x1b[?6{rng.pick('hl')}",
        lambda: f"\x1b[?25{rng.pick('hl')}",
        lambda: f"\x1b[{n(3)} q",
        lambda: f"\x1b[{n(5)}b",
    ]
    return rng.pick(choices)()


SIZES_COLUMNS = [2, 3, 7, 10, 20, 40, 80]
SIZES_ROWS = [1, 2, 3, 5, 10, 24]


def generate(seed: int) -> dict:
    """The case a seed names in this corpus version."""
    rng = SplitMix(seed)
    columns, rows = rng.pick(SIZES_COLUMNS), rng.pick(SIZES_ROWS)
    size = [columns, rows]
    ops: list = []
    for _ in range(4 + rng.below(36)):
        roll = rng.below(100)
        if roll < 6:
            columns, rows = rng.pick(SIZES_COLUMNS), rng.pick(SIZES_ROWS)
            ops.append({"resize": [columns, rows]})
        elif roll < 16:
            ops.append("checkpoint")
        else:
            text = "".join(token(rng, columns, rows) for _ in range(1 + rng.below(8)))
            ops.append({"write": text})
    return {"name": f"{CORPUS_VERSION}/{seed}", "seed": seed, "size": size, "ops": ops}


def named_cases() -> list[dict]:
    corpus = json.loads(NAMED.read_text())
    if corpus.get("format_version") != FORMAT_VERSION:
        raise SetupError(f"{NAMED} is format {corpus.get('format_version')}")
    return corpus["cases"]


def corpus(cases: list[dict], version: str = CORPUS_VERSION) -> dict:
    return {"format_version": FORMAT_VERSION, "corpus_version": version, "cases": cases}


def case_bytes(case: dict) -> int:
    total = 0
    for op in case["ops"]:
        if isinstance(op, dict) and "write" in op:
            total += len(op["write"].encode())
        elif isinstance(op, dict) and "write_hex" in op:
            total += len("".join(op["write_hex"].split())) // 2
    return total


def check_limits(case: dict) -> None:
    sizes = [case["size"]] + [op["resize"] for op in case["ops"] if isinstance(op, dict) and "resize" in op]
    for columns, rows in sizes:
        if columns > MAX_COLUMNS or rows > MAX_ROWS:
            raise SetupError(f"{case['name']}: {columns}x{rows} is over the limit")
    if len(case["ops"]) > MAX_OPS or case_bytes(case) > MAX_BYTES:
        raise SetupError(f"{case['name']}: over the operation or byte limit")


RECORDED_PREFIX = "rec-"


def select(cases: int, seed_base: int, shard: tuple[int, int],
           recorded: list[dict] | None = None) -> list[dict]:
    """This shard's cases: the named and recorded ones on shard 1, and every
    Nth seed."""
    index, count = shard
    chosen = named_cases() + list(recorded or []) if index == 1 else []
    chosen += [generate(seed_base + i) for i in range(cases) if i % count == index - 1]
    for case in chosen:
        check_limits(case)
    return chosen


# --- driving the harness ---------------------------------------------------------


def build(core: str) -> Path:
    """Build one core's test binary and return its path."""
    command = ["cargo", "test", "--locked", "--bin", "terminal-delight", "--no-run",
               "--message-format=json", *CORES[core]]
    done = subprocess.run(command, cwd=APP, capture_output=True, text=True)
    if done.returncode != 0:
        raise SetupError(f"building the {core} test binary failed:\n{done.stderr[-4000:]}")
    for line in done.stdout.splitlines():
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            continue
        if message.get("reason") == "compiler-artifact" and message.get("executable") \
                and message.get("profile", {}).get("test"):
            return Path(message["executable"])
    raise SetupError(f"cargo built no {core} test binary")


_binaries: dict[str, Path] = {}


def binary(core: str) -> Path:
    if core not in _binaries:
        given = os.environ.get(f"TD_LAB_BIN_{core.upper()}")
        path = Path(given) if given else build(core)
        if not path.is_file():
            raise SetupError(f"no {core} test binary at {path}")
        _binaries[core] = path
    return _binaries[core]


@dataclass(frozen=True)
class Spec:
    """One way to run a case: which core, how its writes are cut, and which
    fault, if any, is planted."""

    core: str
    chunks: str = "whole"
    mutation: str = ""

    def label(self) -> str:
        return ":".join(filter(None, [self.core, self.chunks, self.mutation]))

    @staticmethod
    def parse(text: str) -> "Spec":
        parts = text.split(":")
        core = parts[0]
        if core not in CORES:
            raise SetupError(f"unknown core {core!r}; one of {sorted(CORES)}")
        if len(parts) >= 3 and parts[1] == "seed":
            return Spec(core, f"seed:{parts[2]}", parts[3] if len(parts) > 3 else "")
        return Spec(core, parts[1] if len(parts) > 1 else "whole", parts[2] if len(parts) > 2 else "")


def parse_output(text: str, names: list[str], label: str) -> dict[str, dict]:
    """Read a harness output file into case name -> result. Each result is
    `{"observations": [...]}` or `{"failure": {"kind", ...}}`. A missing
    header, a malformed line, an unknown case or a missing case is a setup
    error: an output that measured nothing must never read as agreement."""
    lines = [line for line in text.splitlines() if line.strip()]
    if not lines:
        raise SetupError(f"{label}: the harness wrote nothing")
    try:
        header = json.loads(lines[0])
    except json.JSONDecodeError as error:
        raise SetupError(f"{label}: malformed header: {error}") from None
    if header.get("format_version") != FORMAT_VERSION or "core" not in header:
        raise SetupError(f"{label}: unexpected header {header!r}")
    results: dict[str, dict] = {}
    for number, line in enumerate(lines[1:], start=2):
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise SetupError(f"{label} line {number}: malformed: {error}") from None
        name = record.get("case")
        if name is None and "failure" in record:
            raise SetupError(f"{label}: the harness refused the corpus: {record['failure']}")
        if name not in names:
            raise SetupError(f"{label} line {number}: a case nobody asked for: {name!r}")
        if "observations" in record and isinstance(record["observations"], list) \
                and record["observations"] and all(isinstance(o, dict) for o in record["observations"]):
            results[name] = {"observations": record["observations"]}
        elif isinstance(record.get("failure"), dict) and "kind" in record["failure"]:
            results[name] = {"failure": record["failure"]}
        else:
            raise SetupError(f"{label} line {number}: neither observations nor a failure")
    missing = [name for name in names if name not in results]
    if missing:
        raise SetupError(f"{label}: no result for {len(missing)} case(s), first {missing[0]!r}")
    return results


def execute(spec: Spec, cases: list[dict], timeout_ms: int = 5000) -> dict[str, dict]:
    """Run every case under one spec in one harness process."""
    names = [case["name"] for case in cases]
    if len(set(names)) != len(names):
        raise SetupError("two cases share a name")
    with tempfile.TemporaryDirectory(prefix="td-lab-") as scratch:
        source, sink = Path(scratch, "in.json"), Path(scratch, "out.jsonl")
        source.write_text(json.dumps(corpus(cases)))
        env = dict(os.environ, TD_LAB_IN=str(source), TD_LAB_OUT=str(sink),
                   TD_LAB_CHUNKS=spec.chunks, TD_LAB_MUTATE=spec.mutation,
                   TD_LAB_TIMEOUT_MS=str(timeout_ms))
        command = [str(binary(spec.core)), "vt::lab::replay_from_env", "--exact",
                   "--ignored", "--test-threads=1"]
        budget = HARNESS_FLOOR_S + len(cases) * timeout_ms / 1000
        try:
            done = subprocess.run(command, env=env, capture_output=True, text=True, timeout=budget)
        except subprocess.TimeoutExpired:
            raise SetupError(f"{spec.label()}: the harness ran past {budget:.0f}s") from None
        if done.returncode != 0:
            raise SetupError(f"{spec.label()}: the harness failed:\n{done.stdout[-2000:]}{done.stderr[-2000:]}")
        if not sink.exists():
            raise SetupError(f"{spec.label()}: no output file; did the filter match no test?")
        return parse_output(sink.read_text(), names, spec.label())


# --- comparing -------------------------------------------------------------------

FIELDS = ("size", "cursor", "modes", "history_size", "screen", "history")


def first_difference(a: list[dict], b: list[dict]) -> tuple[int, str] | None:
    """Mirror of `first_difference` in lab.rs: where two runs part."""
    if len(a) != len(b):
        return min(len(a), len(b)), f"{len(a)} checkpoints against {len(b)}"
    for index, (x, y) in enumerate(zip(a, b)):
        if x != y:
            fields = [f for f in FIELDS if x.get(f) != y.get(f)]
            detail = "differs in " + ", ".join(fields or ["an unnamed field"])
            for line, (p, q) in enumerate(zip(x.get("screen", []), y.get("screen", []))):
                if p != q:
                    detail += f"; first at screen row {line}: {json.dumps(p)} vs {json.dumps(q)}"
                    break
            return index, detail
    return None


def signature(a: dict, b: dict) -> str | None:
    """What kind of disagreement two results are, or None if they agree. The
    minimizer keeps a candidate only while this stays the same, so a shrink
    cannot turn one fault into another."""
    fa, fb = a.get("failure"), b.get("failure")
    if fa or fb:
        if fa and fb and fa["kind"] == fb["kind"] and fa["kind"] != "state_mismatch":
            return f"both:{fa['kind']}"
        if fa == fb:
            return None
        return f"a:{fa['kind'] if fa else 'ok'}/b:{fb['kind'] if fb else 'ok'}"
    found = first_difference(a["observations"], b["observations"])
    if found is None:
        return None
    _, detail = found
    return detail.split(";")[0]


def single_signature(result: dict) -> str | None:
    failure = result.get("failure")
    return f"single:{failure['kind']}" if failure else None


@dataclass
class Finding:
    kind: str            # chunk | cores | crash | expectation | control
    case: dict
    a: Spec
    b: Spec | None
    signature: str
    detail: str
    classified: str | None = None
    minimized: dict | None = None
    notes: list[str] = field(default_factory=list)

    def fingerprint(self) -> str:
        seed = json.dumps([self.kind, self.signature, (self.minimized or self.case)["ops"]], sort_keys=True)
        return hashlib.sha256(seed.encode()).hexdigest()[:12]


def evaluate(cases: list[dict], a: Spec, b: Spec | None) -> list[str | None]:
    """Signatures for many candidate cases at once: one harness process per
    spec, however many candidates."""
    if not cases:
        return []
    named = [dict(case, name=f"candidate/{i}") for i, case in enumerate(cases)]
    ra = execute(a, named)
    if b is None:
        return [single_signature(ra[c["name"]]) for c in named]
    rb = execute(b, named)
    return [signature(ra[c["name"]], rb[c["name"]]) for c in named]


# --- minimizing --------------------------------------------------------------------


def _chunks(items: list, n: int) -> list[list]:
    size, extra = divmod(len(items), n)
    out, at = [], 0
    for i in range(n):
        step = size + (1 if i < extra else 0)
        out.append(items[at:at + step])
        at += step
    return [c for c in out if c]


def ddmin(items: list, still_fails: Callable[[list[list]], list[bool]], budget: dict) -> list:
    """Zeller's delta debugging, with every candidate of a round tested in one
    batch. `still_fails` takes candidates and says which keep the failure."""
    n = 2
    while len(items) >= 2 and budget["left"] > 0:
        parts = _chunks(items, n)
        complements = [[x for j, p in enumerate(parts) if j != i for x in p] for i in range(len(parts))]
        candidates = parts + complements
        budget["left"] -= len(candidates)
        verdicts = still_fails(candidates)
        hit = next((i for i, ok in enumerate(verdicts) if ok), None)
        if hit is None:
            if n >= len(items):
                break
            n = min(len(items), n * 2)
        elif hit < len(parts):
            items, n = parts[hit], 2
        else:
            items, n = complements[hit - len(parts)], max(n - 1, 2)
    return items


def write_payload(op) -> tuple[str, bytes] | None:
    if isinstance(op, dict) and "write" in op:
        return "write", op["write"].encode()
    if isinstance(op, dict) and "write_hex" in op:
        return "write_hex", bytes.fromhex("".join(op["write_hex"].split()))
    return None


def as_op(data: bytes) -> dict:
    try:
        return {"write": data.decode()}
    except UnicodeDecodeError:
        return {"write_hex": data.hex()}


def materialize(case: dict, seed: int) -> dict:
    """The case with a seeded cut plan written out: every write split where
    lab.rs's `ChunkPlan::Seeded(seed)` would cut it, each piece its own write.
    Fed whole, it is the seeded run; fed joined, it is the original."""
    rng = SplitMix(seed)
    ops = []
    for op in case["ops"]:
        payload = write_payload(op)
        if payload is None:
            ops.append(op)
            continue
        data, at = payload[1], 0
        while at < len(data):
            end = min(at + 1 + rng.next() % 16, len(data))
            ops.append(as_op(data[at:end]))
            at = end
    return dict(case, ops=ops)


def minimize(finding: Finding, max_candidates: int = 4000, max_seconds: float = 120) -> dict:
    """Shrink a finding's case while its signature holds: operations first,
    then the bytes of each write, then the starting size."""
    started = time.monotonic()
    budget = {"left": max_candidates}
    target = finding.signature
    # A seeded cut plan places its cuts by byte position, so deleting anything
    # moves every cut after it and the failure with it. Cut at every byte
    # instead, when that still shows the fault: every boundary then survives
    # any deletion. The reproducer records the plan it was shrunk under.
    if finding.kind == "chunk" and finding.b and finding.b.chunks.startswith("seed:"):
        written = materialize(finding.case, int(finding.b.chunks.split(":")[1]))
        joined = Spec(finding.b.core, "joined", finding.b.mutation)
        whole = Spec(finding.b.core, "whole", finding.b.mutation)
        found = evaluate([written], joined, whole)[0]
        if found:
            finding.notes.append(f"its {finding.b.chunks} cuts written out as separate writes, "
                                 "then shrunk comparing them joined against whole")
            finding.case, finding.a, finding.b = written, joined, whole
            finding.signature = target = found

    def keeps(cases: list[dict]) -> list[bool]:
        if time.monotonic() - started > max_seconds:
            budget["left"] = 0
            return [False] * len(cases)
        return [s == target for s in evaluate(cases, finding.a, finding.b)]

    case = dict(finding.case)
    ops = ddmin(list(case["ops"]), lambda cands: keeps([dict(case, ops=c) for c in cands]), budget)
    case = dict(case, ops=ops)

    for index in range(len(case["ops"])):
        payload = write_payload(case["ops"][index])
        if payload is None or len(payload[1]) < 2:
            continue
        data = list(payload[1])

        def rebuilt(candidates: list[list[int]], index=index) -> list[bool]:
            cases = []
            for c in candidates:
                ops = list(case["ops"])
                ops[index] = as_op(bytes(c))
                cases.append(dict(case, ops=ops))
            return keeps(cases)

        smaller = ddmin(data, rebuilt, budget)
        ops = list(case["ops"])
        ops[index] = as_op(bytes(smaller))
        case = dict(case, ops=ops)

    for columns, rows in [(2, 1), (4, 2), (10, 3), (20, 5)]:
        if (columns, rows) < tuple(case["size"]) and budget["left"] > 0:
            candidate = dict(case, size=[columns, rows])
            if keeps([candidate])[0]:
                case = candidate
                break

    case["name"] = f"minimized/{finding.kind}/{finding.case['name']}"
    # Checked outside the budget: a reproducer that does not reproduce is
    # worse than none.
    if evaluate([case], finding.a, finding.b)[0] != target:
        raise SetupError(f"the minimized case for {finding.case['name']} no longer fails")
    return case


# --- known differences -----------------------------------------------------------
#
# A cross-core difference is classified by a counterfactual, not by a pattern
# match: each entry in known.json says how to take its cause OUT of a case
# (strip these bytes, drop the resizes). The entry explains a finding when the
# case with that cause removed makes the two cores agree again. A case the
# entries explain only together is classified by all of them. Classified
# findings are counted and listed in every summary; nothing is dropped.

import re  # noqa: E402  (kept beside the only code that uses it)


def known_differences() -> list[dict]:
    if not KNOWN.exists():
        return []
    data = json.loads(KNOWN.read_text())
    for entry in data["differences"]:
        if not entry.get("id") or not entry.get("reason") or not entry.get("neutralize"):
            raise SetupError(f"known.json entry {entry.get('id')!r} needs an id, a reason and a neutralize rule")
        if entry.get("kind", "cores") not in ("cores", "chunk") or \
                (entry.get("kind") == "chunk" and entry.get("core") not in CORES):
            raise SetupError(f"known.json entry {entry['id']!r}: kind is cores, or chunk with a core")
        rule = entry["neutralize"]
        if set(rule) - {"strip", "drop_resize", "when"} or not set(rule) - {"when"}:
            raise SetupError(f"known.json entry {entry['id']!r}: neutralize takes strip and/or drop_resize, "
                             "and optionally when")
        for key in ("strip", "when"):
            if key in rule:
                try:
                    re.compile(rule[key].encode())
                except re.error as error:
                    raise SetupError(f"known.json entry {entry['id']!r}: bad {key} pattern: {error}") from None
    return data["differences"]


def all_bytes(case: dict) -> bytes:
    return b"".join(p[1] for p in map(write_payload, case["ops"]) if p)


def neutralize(case: dict, entries: list[dict]) -> dict:
    """The case with each entry's cause taken out. An entry with a `when`
    pattern applies only to a case whose bytes match it, so a cause can be
    narrowed to its circumstances (an erase, but only near a wide character)
    instead of taking out every erase from every case."""
    ops = list(case["ops"])
    for entry in entries:
        rule = entry["neutralize"]
        if "when" in rule and not re.search(rule["when"].encode(), all_bytes(dict(case, ops=ops))):
            continue
        if rule.get("drop_resize"):
            ops = [op for op in ops if not (isinstance(op, dict) and "resize" in op)]
        if "strip" in rule:
            pattern = re.compile(rule["strip"].encode())
            stripped = []
            for op in ops:
                payload = write_payload(op)
                stripped.append(as_op(pattern.sub(b"", payload[1])) if payload else op)
            ops = stripped
    return dict(case, ops=ops)


def fewest_causes(cases: list[dict], sets: list[list[dict]], a: Spec, b: Spec) -> list[list[dict]]:
    """Each case is explained by its whole set of causes; drop any cause the
    explanation survives without, one cause per batch across all cases."""
    sets = [list(s) for s in sets]
    order: list[dict] = []
    for entry in (e for s in sets for e in s):
        if all(entry is not seen for seen in order):
            order.append(entry)
    for entry in order:
        idx = [i for i, s in enumerate(sets) if entry in s and len(s) > 1]
        if not idx:
            continue
        verdicts = evaluate([neutralize(cases[i], [e for e in sets[i] if e is not entry]) for i in idx], a, b)
        for i, verdict in zip(idx, verdicts):
            if verdict is None:
                sets[i] = [e for e in sets[i] if e is not entry]
    return sets


def explain(case: dict, a: Spec, b: Spec, known: list[dict]) -> str | None:
    """Which known entry, alone or with all the others, makes one case's
    cores agree when its cause is removed. One harness batch."""
    entries = [e for e in known if e.get("kind", "cores") == "cores"
               and neutralize(case, [e])["ops"] != case["ops"]]
    if not entries:
        return None
    candidates = [neutralize(case, [e]) for e in entries] + [neutralize(case, entries)]
    verdicts = evaluate(candidates, a, b)
    for entry, verdict in zip(entries, verdicts):
        if verdict is None:
            return entry["id"]
    if verdicts[-1] is not None:
        return None
    return "+".join(e["id"] for e in fewest_causes([case], [entries], a, b)[0])


def classify_all(findings: list[Finding], known: list[dict]) -> None:
    """Classify every cross-core finding by counterfactual: one batch per
    known entry, then one with every entry at once."""
    # Chunk findings: only entries that say they describe one core's chunk
    # dependence, tried one at a time against that core's own two runs.
    for entry in [e for e in known if e.get("kind") == "chunk"]:
        chunked = [f for f in findings if f.kind == "chunk" and not f.classified
                   and f.a.core == entry.get("core")
                   and neutralize(f.case, [entry])["ops"] != f.case["ops"]]
        by_spec: dict[tuple, list[Finding]] = {}
        for f in chunked:
            by_spec.setdefault((f.a, f.b), []).append(f)
        for (a, b), group in by_spec.items():
            verdicts = evaluate([neutralize(f.case, [entry]) for f in group], a, b)
            for finding, verdict in zip(group, verdicts):
                if verdict is None:
                    finding.classified = entry["id"]
    known = [e for e in known if e.get("kind", "cores") == "cores"]
    pending = [f for f in findings if f.kind == "cores"]
    for entry in known:
        if not pending:
            return
        # Only a finding whose case the rule actually changes can be explained by it.
        touched = [f for f in pending if neutralize(f.case, [entry])["ops"] != f.case["ops"]]
        verdicts = evaluate([neutralize(f.case, [entry]) for f in touched], touched[0].a, touched[0].b) \
            if touched else []
        for finding, verdict in zip(touched, verdicts):
            if verdict is None:
                finding.classified = entry["id"]
        pending = [f for f in pending if not f.classified]
    if pending and len(known) > 1:
        verdicts = evaluate([neutralize(f.case, known) for f in pending], pending[0].a, pending[0].b)
        explained = [f for f, v in zip(pending, verdicts) if v is None]
        touching = [[e for e in known if neutralize(f.case, [e])["ops"] != f.case["ops"]] for f in explained]
        if explained:
            reduced = fewest_causes([f.case for f in explained], touching, explained[0].a, explained[0].b)
            for finding, causes in zip(explained, reduced):
                finding.classified = "+".join(e["id"] for e in causes)


# --- the run ------------------------------------------------------------------------


def find_differences(cases: list[dict], cores: list[str]) -> tuple[list[Finding], dict]:
    results: dict[Spec, dict] = {}
    counts = {"cases": len(cases), "runs": 0, "observations": 0}
    for core in cores:
        for chunks in ["whole"] + [f"seed:{s}" for s in CHUNK_SEEDS]:
            spec = Spec(core, chunks)
            results[spec] = execute(spec, cases)
            counts["runs"] += len(cases)
            counts["observations"] += sum(len(r.get("observations", [])) for r in results[spec].values())
    if counts["observations"] == 0:
        raise SetupError("no observation was recorded; a lab that measured nothing has not passed")
    recorded = [case for case in cases if case["name"].startswith(RECORDED_PREFIX)]
    joined_results: dict[str, dict] = {}
    if recorded:
        for core in cores:
            joined_results[core] = execute(Spec(core, "joined"), recorded)
            counts["runs"] += len(recorded)

    findings: list[Finding] = []
    by_name = {case["name"]: case for case in cases}
    for name, case in by_name.items():
        for core in cores:
            whole = results[Spec(core)][name]
            failure = whole.get("failure")
            if failure:
                kind = "expectation" if failure["kind"] == "state_mismatch" else "crash"
                findings.append(Finding(kind, case, Spec(core), None, f"single:{failure['kind']}",
                                        json.dumps(failure)))
                continue
            # A recording's writes are the reads a real terminal saw; joined
            # into one, they are the same bytes with no boundary at all.
            cuts = [Spec(core, "joined")] if name in joined_results.get(core, {}) else []
            for cut in cuts + [Spec(core, f"seed:{s}") for s in CHUNK_SEEDS]:
                result = joined_results[core][name] if cut.chunks == "joined" else results[cut][name]
                sig = signature(whole, result)
                if sig:
                    findings.append(Finding("chunk", case, Spec(core), cut, sig,
                                            str(first_difference(whole.get("observations", []),
                                                                 result.get("observations", [])))))
                    break
        if len(cores) == 2:
            a, b = Spec(cores[0]), Spec(cores[1])
            ra, rb = results[a][name], results[b][name]
            if not ra.get("failure") and not rb.get("failure"):
                sig = signature(ra, rb)
                if sig:
                    findings.append(Finding("cores", case, a, b, sig,
                                            str(first_difference(ra["observations"], rb["observations"]))))
    return findings, counts


def write_finding(finding: Finding, directory: Path, used: list[int]) -> bool:
    """Write one finding's reproducer if it fits under the artifact cap."""
    fingerprint = finding.fingerprint()
    shown = finding.minimized or finding.case
    case_text = json.dumps(corpus([shown], "reproducer"), indent=1, ensure_ascii=False)
    replay = replay_command(finding, Path("case.json"))
    body = {
        "kind": finding.kind, "signature": finding.signature, "detail": finding.detail,
        "classified": finding.classified, "a": finding.a.label(),
        "b": finding.b.label() if finding.b else None, "original_case": finding.case["name"],
        "original_ops": len(finding.case["ops"]), "original_bytes": case_bytes(finding.case),
        "minimized_ops": len(shown["ops"]), "minimized_bytes": case_bytes(shown),
        "replay": replay, "notes": finding.notes,
    }
    text = json.dumps(body, indent=1, ensure_ascii=False)
    size = len(case_text.encode()) + len(text.encode())
    if used[0] + size > ARTIFACT_CAP:
        return False
    used[0] += size
    target = directory / fingerprint
    target.mkdir(parents=True, exist_ok=True)
    (target / "case.json").write_text(case_text)
    (target / "finding.json").write_text(text)
    return True


def replay_command(finding: Finding, path: Path) -> str:
    parts = ["python3", "scripts/lab/run.py", "replay", str(path), "--a", finding.a.label()]
    if finding.b:
        parts += ["--b", finding.b.label()]
    return shlex.join(parts)


def summarize(out: Path, title: str, counts: dict, findings: list[Finding], extra: list[str],
              tail: list[str] | None = None) -> None:
    unclassified = [f for f in findings if not f.classified]
    lines = [f"# {title}", "",
             f"- cases: {counts['cases']}, case runs: {counts['runs']}"
             + (f", observations: {counts['observations']}" if counts.get("observations") else ""),
             f"- findings: {len(findings)} ({len(unclassified)} unclassified, "
             f"{len(findings) - len(unclassified)} known)"]
    lines += extra + [""]
    explained: dict[str, int] = {}
    for f in findings:
        if f.classified:
            explained[f.classified] = explained.get(f.classified, 0) + 1
    if explained:
        lines += ["Known differences (explained by removing their cause; see scripts/lab/known.json):", "",
                  "| known difference | findings |", "|---|---|"]
        lines += [f"| {name} | {count} |" for name, count in sorted(explained.items(), key=lambda kv: -kv[1])]
        lines.append("")
    if unclassified:
        lines += ["Unexplained — each needs a fix, a known.json entry with its reason, or an issue:", "",
                  "| kind | case | runs | signature | ops before → after | reproducer |", "|---|---|---|---|---|---|"]
        for f in unclassified:
            m = f.minimized or f.case
            lines.append(f"| {f.kind} | `{f.case['name']}` | {f.a.label()}{' vs ' + f.b.label() if f.b else ''} "
                         f"| {f.signature} | {len(f.case['ops'])} → {len(m['ops'])} | "
                         f"`failures/{f.fingerprint()}/` |")
        lines.append("")
    lines += tail or []
    (out / "summary.md").write_text("\n".join(lines) + "\n")
    (out / "summary.json").write_text(json.dumps({
        "counts": counts,
        "findings": [{"kind": f.kind, "case": f.case["name"], "signature": f.signature,
                      "classified": f.classified, "fingerprint": f.fingerprint()} for f in findings],
    }, indent=1))
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as handle:
            handle.write("\n".join(lines) + "\n")


def command_run(args) -> int:
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    started = time.monotonic()
    phases: dict[str, float] = {}
    mark = [started]

    def lap(name: str) -> None:
        now = time.monotonic()
        phases[name] = now - mark[0]
        mark[0] = now

    recorded, recordings = [], []
    if args.shard[0] == 1 and args.record:
        recorded, recordings = record_programs()
        (out / "recordings.json").write_text(json.dumps(recordings, indent=1))
    lap("record")
    cases = select(args.cases, args.seed_base, args.shard, recorded)
    cover = unknowns.coverage(sources(cases))
    (out / "coverage.json").write_text(json.dumps(cover, indent=1))
    findings, counts = find_differences(cases, list(CORES))
    lap("replay")
    known = known_differences()
    classify_all(findings, known)
    lap("classify")
    # Every unexplained finding is minimized, while the budget lasts: that is
    # the reproducer someone will read. A known one keeps its original case.
    used = [0]
    unwritten = 0
    for finding in findings:
        if not finding.classified and finding.kind in ("chunk", "cores", "crash"):
            if time.monotonic() - started < args.minimize_budget:
                try:
                    finding.minimized = minimize(finding)
                except SetupError as error:
                    finding.notes.append(f"not minimized: {error}")
                # The whole case can hold two causes where only one is known;
                # its minimized reproducer, which keeps the same signature,
                # usually holds one. Explained there, it is explained.
                if finding.minimized and finding.kind == "cores":
                    cause = explain(finding.minimized, finding.a, finding.b, known)
                    if cause:
                        finding.classified = f"{cause} (minimized)"
            else:
                finding.notes.append("not minimized: the run's minimizing budget was spent")
        if not finding.classified and not write_finding(finding, out / "failures", used):
            unwritten += 1
    lap("minimize")
    counts["seconds"] = {k: round(v, 1) for k, v in phases.items()}
    extra = [f"- shard {args.shard[0]}/{args.shard[1]}, seed base {args.seed_base}, "
             f"corpus {CORPUS_VERSION}, {time.monotonic() - started:.1f}s "
             f"(record {phases['record']:.0f}s, replay {phases['replay']:.0f}s, classify {phases['classify']:.0f}s, minimize {phases['minimize']:.0f}s)"]
    if unwritten:
        extra.append(f"- {unwritten} finding(s) not written: the {ARTIFACT_CAP // 1048576} MB artifact cap was reached")
    if args.shard[0] == 1:
        extra += recording_lines(recordings, args.record)
    summarize(out, "Terminal Lab — correctness run", counts, findings, extra, unknowns.summary_lines(cover))
    print((out / "summary.md").read_text())
    return 1 if any(not f.classified for f in findings) else 0


def sources(cases: list[dict]) -> dict[str, list[bytes]]:
    """Each case's bytes, by where the case came from."""
    out: dict[str, list[bytes]] = {"generated": [], "named": [], "recorded": []}
    for case in cases:
        name = case["name"]
        kind = "recorded" if name.startswith(RECORDED_PREFIX) else \
            "generated" if name.startswith(CORPUS_VERSION + "/") else "named"
        out[kind].append(all_bytes(case))
    return out


def record_programs() -> tuple[list[dict], list[dict]]:
    """Real program output, through record.py's leak gate. A recorder that
    cannot run at all is reported, never mistaken for having nothing to say."""
    try:
        import record
        return record.record_all()
    except Exception as error:  # noqa: BLE001 — reported in the summary, not swallowed
        return [], [{"program": "*", "recorded": False,
                     "reason": f"not recorded: the recorder failed ({type(error).__name__}: {error})"}]


def recording_lines(recordings: list[dict], asked: bool) -> list[str]:
    if not asked:
        return ["- recorded programs: none, recording was switched off (--no-record)"]
    done = [r for r in recordings if r.get("recorded")]
    lines = [f"- recorded programs: {len(done)} of {len(recordings)} recordings made, "
             f"{sum(r['bytes'] for r in done)} bytes"]
    for r in recordings:
        if not r.get("recorded"):
            size = "x".join(map(str, r.get("size", []))) or "every size"
            lines.append(f"  - {r['program']} {size}: {r['reason']}")
    return lines


def command_control(args) -> int:
    """Prove the lab detects, minimizes and replays a planted fault. A lab
    that cannot catch these has not earned a green run."""
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    # The named cases state expectations, so a planted fault that breaks one is
    # caught however rarely the generator happens to exercise it.
    cases = named_cases() + [generate(args.seed_base + i) for i in range(args.cases)]
    controls = [
        ("erase-line-ignored", Spec("rio"), Spec("rio", "whole", "erase-line-ignored")),
        ("escape-lost-at-seam", Spec("rio", "whole", "escape-lost-at-seam"),
         Spec("rio", "every-byte", "escape-lost-at-seam")),
    ]
    ok, findings, extra, used = True, [], [], [0]
    counts = {"cases": len(cases), "runs": 0, "observations": 0}
    for name, a, b in controls:
        signatures = evaluate(cases, a, b)
        counts["runs"] += 2 * len(cases)
        caught = [(c, s) for c, s in zip(cases, signatures) if s]
        if not caught:
            extra.append(f"- **{name}: NOT DETECTED** in {len(cases)} cases")
            ok = False
            continue
        case, sig = max(caught, key=lambda pair: case_bytes(pair[0]))
        finding = Finding("control", case, a, b, sig, f"planted fault {name}")
        finding.minimized = minimize(finding)
        before, after = case_bytes(case), case_bytes(finding.minimized)
        replayed = evaluate([finding.minimized], a, b)[0] == sig
        shrank = after < before
        ok = ok and replayed and shrank
        finding.classified = "planted"
        findings.append(finding)
        write_finding(finding, out / "controls", used)
        extra.append(f"- {name}: detected in {len(caught)}/{len(cases)} cases; "
                     f"shrank {before} → {after} bytes, {len(case['ops'])} → {len(finding.minimized['ops'])} ops; "
                     f"replays: {'yes' if replayed else 'NO'}; reproducer: `{json.dumps(all_bytes(finding.minimized).decode('utf-8', 'replace'))}`")
    summarize(out, "Terminal Lab — planted-fault controls", counts, findings, extra)
    print((out / "summary.md").read_text())
    return 0 if ok else 1


def command_replay(args) -> int:
    data = json.loads(Path(args.case).read_text())
    cases = data["cases"]
    a = Spec.parse(args.a)
    b = Spec.parse(args.b) if args.b else None
    ra = execute(a, cases)
    rb = execute(b, cases) if b else None
    failing = 0
    for case in cases:
        name = case["name"]
        if rb is None:
            sig = single_signature(ra[name])
            print(f"{name}: {a.label()} -> {sig or 'ok'}")
        else:
            sig = signature(ra[name], rb[name])
            where = None
            if sig and "observations" in ra[name] and "observations" in rb[name]:
                where = first_difference(ra[name]["observations"], rb[name]["observations"])
            print(f"{name}: {a.label()} vs {b.label()} -> {sig or 'agree'}" + (f"\n  {where}" if where else ""))
            if where and args.show:
                show(ra[name]["observations"][where[0]], rb[name]["observations"][where[0]], a, b)
        failing += bool(sig)
    return 1 if failing else 0


def show(x: dict, y: dict, a: "Spec", b: "Spec") -> None:
    """Both sides of every field that differs at one checkpoint, row by row
    for the screen and history, so a person can read the disagreement."""
    for f in FIELDS:
        if x.get(f) == y.get(f):
            continue
        if f in ("screen", "history"):
            rows_x, rows_y = x.get(f, []), y.get(f, [])
            for i in range(max(len(rows_x), len(rows_y))):
                p = rows_x[i] if i < len(rows_x) else None
                q = rows_y[i] if i < len(rows_y) else None
                if p != q:
                    label = f"{f}[{-(i + 1) if f == 'history' else i}]"
                    print(f"    {label:<12} {a.core:<10} {json.dumps(p, ensure_ascii=False)}")
                    print(f"    {'':<12} {b.core:<10} {json.dumps(q, ensure_ascii=False)}")
        else:
            print(f"    {f:<12} {a.core:<10} {json.dumps(x.get(f), ensure_ascii=False)}")
            print(f"    {'':<12} {b.core:<10} {json.dumps(y.get(f), ensure_ascii=False)}")


def command_corpus(args) -> int:
    cases = select(args.cases, args.seed_base, (1, 1))
    Path(args.out).write_text(json.dumps(corpus(cases), indent=1, ensure_ascii=False))
    print(f"{len(cases)} cases -> {args.out}")
    return 0


def shard(text: str) -> tuple[int, int]:
    index, count = (int(x) for x in text.split("/"))
    if not 1 <= index <= count:
        raise argparse.ArgumentTypeError("shard is I/N with 1 <= I <= N")
    return index, count


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    for name in ("corpus", "run", "control"):
        p = sub.add_parser(name)
        p.add_argument("--out", required=True)
        p.add_argument("--cases", type=int, default=200 if name != "control" else 60)
        p.add_argument("--seed-base", type=int, default=0)
        if name == "run":
            p.add_argument("--shard", type=shard, default=(1, 1))
            p.add_argument("--minimize-budget", type=float, default=300,
                           help="seconds this run may spend minimizing")
            p.add_argument("--no-record", dest="record", action="store_false",
                           help="skip real program recordings on shard 1")
    p = sub.add_parser("replay")
    p.add_argument("case")
    p.add_argument("--a", required=True)
    p.add_argument("--b")
    p.add_argument("--show", action="store_true", help="print both sides of what differs")
    args = parser.parse_args(argv)
    try:
        return {"corpus": command_corpus, "run": command_run, "control": command_control,
                "replay": command_replay}[args.command](args)
    except SetupError as error:
        print(f"SETUP FAILED: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
