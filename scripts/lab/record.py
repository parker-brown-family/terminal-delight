#!/usr/bin/env python3
"""Real program output for the Terminal Lab: run ordinary programs (an editor,
a pager, git, a shell's line editor, a progress bar) on a pseudo-terminal of a
fixed size, drive them with scripted keystrokes, and keep what they printed
exactly as it was read. Each read becomes one write in a lab case, so the
cases carry the read boundaries a real terminal saw.

Recordings are made at run time and never committed. The repository and its
CI artifacts are public, so two rules hold however the recorder is run:

    sterile    every program runs under `env -i` with a fixed, short
               environment, a fresh home directory and working directory,
               and content that this script writes itself (a text file, a
               Python file, a git history by "Lab <lab@example.invalid>").
               No process listers (top, htop, ps): they would show other
               people's command lines.

    gated      every recording passes `leak_check` before it is used or
               written anywhere. The check fails closed: a recording that
               contains this machine's home directory, user or host name,
               machine id, an email address outside example.invalid, a
               non-loopback IP address, a secret-looking environment value
               or a token-shaped string is dropped, and the reason is
               reported without the matched text.

A program missing on this machine is reported as not recorded, never left out
of the count silently.

    record.py list                      what would be recorded, and what is missing
    record.py corpus --out FILE         record everything into a lab corpus

Standard library only.
"""

from __future__ import annotations

import argparse
import fcntl
import json
import os
import pwd
import re
import select
import shutil
import signal
import socket
import struct
import subprocess
import sys
import tempfile
import termios
import time
from dataclasses import dataclass, field
from pathlib import Path

RECORDED_VERSION = "rec-1"
SIZES = [(80, 24), (40, 12)]
# Within lab.rs's MAX_BYTES and MAX_OPS, with room to spare.
MAX_RECORDED_BYTES = 512 * 1024
MAX_RECORDED_OPS = 2000
QUIET_S = 0.2        # output this long quiet means the program is waiting
STEP_LIMIT_S = 3.0   # the most one step waits for quiet
EXIT_LIMIT_S = 3.0


# --- the programs ------------------------------------------------------------------


@dataclass
class Program:
    """One recording: what runs, the keys it is sent once its output goes
    quiet, and which executables it needs."""

    name: str
    argv: list[str]
    needs: list[str]
    keys: list[str] = field(default_factory=list)
    env: dict[str, str] = field(default_factory=dict)


PROGRESS = r"""
import sys, time
w = sys.stdout.write
w("\x1b[?25l")
for i in range(0, 101, 5):
    bar = "█" * (i // 5) + "░" * (20 - i // 5)
    w(f"\r\x1b[1;32m{bar}\x1b[0m {i:3d}% \x1b[2mfetching lab.tar\x1b[0m\x1b[K")
    sys.stdout.flush(); time.sleep(0.02)
w("\n")
for i, c in enumerate("|/-\\" * 4):
    w(f"\r\x1b[33m{c}\x1b[0m working … {i}"); sys.stdout.flush(); time.sleep(0.02)
w("\r\x1b[2K\x1b[1;32m✔\x1b[0m done — 3 files, 你好\n\x1b[?25h")
"""

TPUT = ("tput setaf 1; echo red; tput sgr0; tput bold; echo bold; tput sgr0; "
        "tput cup 5 10; echo moved; tput smcup; echo on the alternate screen; "
        "tput cup 2 4; tput el; echo erased; tput rmcup; echo back; clear; echo after clear")

PS1 = r"\[\e[1;32m\]lab\[\e[0m\]:\[\e[34m\]~\[\e[0m\]\$ "

LEFT = "\x1b[D"
# Not keys: the pane is resized here, as TD resizes a pane when it is split
# or its split closes. NARROW takes a third off the width; WIDEN puts it back.
NARROW, WIDEN = "<narrow>", "<widen>"


def programs() -> list[Program]:
    """Every recording, in order. Keys are sent one entry at a time, each
    after the program's output has gone quiet."""
    vim_keys = [":syntax on\r", ":set number\r", "G", "o# a line the lab typed, 你好, é\x1b",
                "gg", "dd", "u", ":%s/lab/LAB/g\r", "/LAB\r", NARROW, ":set nonumber\r", WIDEN, "\x0c", ":q!\r"]
    return [
        Program("vim", ["vim", "-u", "NONE", "-N", "-i", "NONE", "-n", "sample.py"], ["vim"], vim_keys),
        Program("nvim", ["nvim", "--clean", "-n", "-i", "NONE", "sample.py"], ["nvim"], vim_keys),
        Program("less", ["less", "-R", "notes.txt"], ["less"],
                [" ", " ", "b", NARROW, "/wide\r", "n", WIDEN, "G", "g", "q"], {"LESSHISTFILE": "-"}),
        Program("git-log", ["git", "--no-pager", "log", "--graph", "--color=always", "--decorate",
                            "--all", "--format=%C(yellow)%h%C(reset) %C(bold)%s%C(reset) %C(auto)%d"],
                ["git"]),
        Program("git-diff", ["git", "--no-pager", "-c", "color.ui=always", "log", "-p", "-n", "2",
                             "--format=%C(yellow)%h%C(reset) %s"], ["git"]),
        Program("ls", ["ls", "--color=always", "-F", "listing"], ["ls"]),
        Program("bash-readline", ["bash", "--norc", "--noprofile", "-i"], ["bash"],
                ["echo hello wide 你好 world", LEFT * 6, "X", "\x01", "# ", "\x05", "\r",
                 "printf '\\e[31mred\\e[0m \\e[4munder\\e[0m tab\\there\\n'\r",
                 "echo " + "a long line that wraps past the edge of the pane " * 2 + "\r",
                 "echo " + "宽字符 wide " * 6 + "\r",
                 NARROW, "echo narrowed\r", WIDEN, "\x0c", "exit\r"],
                {"PS1": PS1, "HISTFILE": "/dev/null"}),
        Program("progress", ["python3", "-c", PROGRESS], ["python3"]),
        Program("tput", ["sh", "-c", TPUT], ["sh", "tput", "clear"]),
        Program("man", ["man", "-l", "lab.1"], ["man", "groff"], [],
                {"MANPAGER": "cat", "MAN_KEEP_FORMATTING": "1", "MANWIDTH": ""}),
        Program("whiptail", ["whiptail", "--title", "Terminal Lab", "--yesno",
                             "Replay this case through both cores?", "10", "34"], ["whiptail"],
                ["\t", "\t", "\r"]),
    ]


# --- fixtures ------------------------------------------------------------------------

NOTES = "\n".join(
    [f"line {i:3d}: the lab pages through this text" for i in range(1, 41)]
    + ["a wide line: 你好世界 and an accent: café, a mark: é",
       "a\ttab\tseparated\tline", "x" * 120]
    + [f"line {i:3d}: more text after the wide line" for i in range(41, 121)]) + "\n"

SAMPLE = '''"""A small Python file for the editor to colour."""

import sys


def greet(name: str) -> str:
    # a comment the lab will edit
    return f"hello, {name}"


if __name__ == "__main__":
    for word in ("lab", "terminal", "delight"):
        print(greet(word), file=sys.stdout)
'''

MANPAGE = r""".TH LAB 1 "2026-10-19" "lab" "Terminal Lab"
.SH NAME
lab \- replay byte streams through two terminal cores
.SH SYNOPSIS
.B lab
[\fB\-\-cases\fR \fIN\fR]
.SH DESCRIPTION
The lab feeds \fBgenerated\fR and \fIrecorded\fR output to both cores and
compares what they draw.
.TP
.B \-\-cases
How many generated cases to run.
"""


def write_fixtures(work: Path, env: dict[str, str]) -> None:
    (work / "notes.txt").write_text(NOTES)
    (work / "sample.py").write_text(SAMPLE)
    (work / "lab.1").write_text(MANPAGE)
    listing = work / "listing"
    listing.mkdir()
    for name in ["README.md", "notes.txt", "archive.tar.gz", "photo.png", "Makefile"]:
        (listing / name).write_text("lab\n")
    (listing / "src").mkdir()
    (listing / "build").mkdir()
    run = listing / "run.sh"
    run.write_text("#!/bin/sh\n")
    run.chmod(0o755)
    (listing / "link-to-notes").symlink_to("notes.txt")
    (listing / "broken-link").symlink_to("missing")
    if shutil.which("git"):
        git_history(work, env)


def git_history(work: Path, env: dict[str, str]) -> None:
    """A tiny repository with a branch and a merge, so the graph has edges."""
    def git(*args: str, date: str = "2026-01-01T00:00:00Z") -> None:
        subprocess.run(["git", *args], cwd=work, check=True, capture_output=True,
                       env=dict(env, GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date))

    git("init", "-q", "-b", "main")
    git("add", "notes.txt")
    git("commit", "-q", "-m", "Notes the lab pages through")
    git("checkout", "-q", "-b", "feature")
    (work / "sample.py").write_text(SAMPLE.replace("hello", "hi"))
    git("add", "sample.py")
    git("commit", "-q", "-m", "A sample for the editor", date="2026-01-02T00:00:00Z")
    git("checkout", "-q", "main")
    (work / "notes.txt").write_text(NOTES.replace("line   1", "line one"))
    git("commit", "-q", "-am", "The first line, in words", date="2026-01-03T00:00:00Z")
    git("merge", "-q", "--no-ff", "-m", "Merge the editor sample", "feature", date="2026-01-04T00:00:00Z")
    git("tag", "v0.1")
    (work / "sample.py").write_text(SAMPLE)


def sterile_env(home: Path) -> dict[str, str]:
    """The whole environment a recorded program sees. Nothing is inherited."""
    return {
        "TERM": "xterm-256color", "LANG": "C.UTF-8", "LC_ALL": "C.UTF-8",
        "PATH": "/usr/bin:/bin", "HOME": str(home), "USER": "lab", "LOGNAME": "lab",
        "SHELL": "/bin/sh", "TZ": "UTC",
        "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null",
        "GIT_AUTHOR_NAME": "Lab", "GIT_AUTHOR_EMAIL": "lab@example.invalid",
        "GIT_COMMITTER_NAME": "Lab", "GIT_COMMITTER_EMAIL": "lab@example.invalid",
        "XDG_CONFIG_HOME": str(home / ".config"), "XDG_DATA_HOME": str(home / ".local/share"),
        "XDG_STATE_HOME": str(home / ".local/state"), "XDG_CACHE_HOME": str(home / ".cache"),
    }


# --- the leak gate ---------------------------------------------------------------------

ESCAPES = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b[P_^X][^\x1b]*\x1b\\|\x1b.")
EMAIL = re.compile(rb"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+)")
IPV4 = re.compile(rb"(?<![\d.])(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?![\d.])")
IPV6 = re.compile(rb"(?<![0-9A-Fa-f:])(?:[0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}(?![0-9A-Fa-f:])"
                  rb"|(?<![0-9A-Fa-f:])(?:[0-9A-Fa-f]{1,4}:)+:(?:[0-9A-Fa-f]{1,4}:)*[0-9A-Fa-f]{1,4}(?![0-9A-Fa-f:])")
TOKENS = re.compile(rb"gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|sk-[A-Za-z0-9_-]{20,}"
                    rb"|AKIA[0-9A-Z]{16}|xox[abposr]-[A-Za-z0-9-]{10,}|-----BEGIN [A-Z ]*PRIVATE KEY")
HEX_RUN = re.compile(rb"(?<![0-9a-fA-F])[0-9a-fA-F]{32,}(?![0-9a-fA-F])")
B64_RUN = re.compile(rb"[A-Za-z0-9+/_-]{40,}={0,2}")
SECRET_NAMES = re.compile(r"TOKEN|SECRET|PASS|KEY|AUTH|COOKIE|SESSION|CREDENTIAL", re.I)


def machine_needles() -> list[tuple[str, bytes]]:
    """What identifies this machine and person: each with the reason given
    when it is found, never the value."""
    needles: list[tuple[str, bytes]] = []
    entry = pwd.getpwuid(os.getuid())
    for label, value in [("the recorder's home directory", entry.pw_dir),
                         ("the recorder's home directory", os.environ.get("HOME", "")),
                         ("the recorder's user name", entry.pw_name),
                         ("the recorder's user name", os.environ.get("USER", "")),
                         ("this machine's host name", socket.gethostname())]:
        if len(value) >= 3:
            needles.append((label, value.encode()))
    try:
        machine_id = Path("/etc/machine-id").read_text().strip()
        if machine_id:
            needles.append(("this machine's id", machine_id.encode()))
    except OSError:
        pass
    for name, value in os.environ.items():
        if SECRET_NAMES.search(name) and len(value) >= 8:
            needles.append((f"the value of a secret-looking environment variable ({name})", value.encode()))
    return needles


def leak_check(data: bytes, needles: list[tuple[str, bytes]] | None = None) -> list[str]:
    """Reasons a recording must not be used; empty means it may. Checked on
    the bytes as read and again with escape sequences removed, so a secret
    split by colour codes is still found. Fails closed: every rule that
    matches is a rejection, and nothing is excused but loopback addresses
    and example.invalid mail."""
    needles = machine_needles() if needles is None else needles
    plain = ESCAPES.sub(b"", data)
    reasons: list[str] = []
    for text in (data, plain):
        for label, value in needles:
            if value and value in text:
                reasons.append(f"contains {label}")
        for match in EMAIL.finditer(text):
            if match.group(1).lower() != b"example.invalid":
                reasons.append(f"contains an email address at byte {match.start()}")
        for match in IPV4.finditer(text):
            octets = [int(x) for x in match.groups()]
            if all(o <= 255 for o in octets) and octets[0] != 127:
                reasons.append(f"contains an IPv4 address at byte {match.start()}")
        for match in IPV6.finditer(text):
            if match.group(0) not in (b"::1",):
                reasons.append(f"contains an IPv6 address at byte {match.start()}")
        for match in TOKENS.finditer(text):
            reasons.append(f"contains a token-shaped string at byte {match.start()}")
        for match in HEX_RUN.finditer(text):
            reasons.append(f"contains a {len(match.group(0))}-character hex run at byte {match.start()}")
        for match in B64_RUN.finditer(text):
            run = match.group(0)
            if re.search(rb"[0-9]", run) and re.search(rb"[A-Z]", run) and re.search(rb"[a-z]", run):
                reasons.append(f"contains a {len(run)}-character base64-shaped run at byte {match.start()}")
    return sorted(set(reasons))


# --- recording ------------------------------------------------------------------------


@dataclass
class Recording:
    program: str
    size: tuple[int, int]
    # bytes as read; None where the program went quiet; ("resize", columns, rows)
    reads: list = field(default_factory=list)
    notes: list[str] = field(default_factory=list)

    def data(self) -> bytes:
        return b"".join(r for r in self.reads if isinstance(r, bytes))


def _drain(fd: int, recording: Recording, quiet: float, limit: float) -> bool:
    """Read until the output has been quiet for `quiet` seconds or `limit`
    passes. False once the program has closed the terminal."""
    deadline = time.monotonic() + limit
    while time.monotonic() < deadline:
        ready, _, _ = select.select([fd], [], [], quiet)
        if not ready:
            return True
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            return False
        if not chunk:
            return False
        recording.reads.append(chunk)
        answer(fd, chunk)
    return True


# What a terminal answers to the queries programs send while starting up. An
# editor that hears nothing waits, then complains on screen; these answers
# are fixed, so they say nothing about the machine.
ANSWERS = [
    (b"\x1b[5n", b"\x1b[0n"),
    (b"\x1b[6n", b"\x1b[1;1R"),
    (b"\x1b[c", b"\x1b[?62;22c"),
    (b"\x1b[0c", b"\x1b[?62;22c"),
    (b"\x1b]11;?\x07", b"\x1b]11;rgb:0000/0000/0000\x07"),
    (b"\x1b]10;?\x07", b"\x1b]10;rgb:ffff/ffff/ffff\x07"),
]


def answer(fd: int, chunk: bytes) -> None:
    for query, reply in ANSWERS:
        for _ in range(chunk.count(query)):
            try:
                os.write(fd, reply)
            except OSError:
                return


def record(program: Program, size: tuple[int, int], work: Path, env: dict[str, str]) -> Recording:
    """Run one program on a fresh pty of `size` and keep every read."""
    columns, rows = size
    recording = Recording(program.name, size)
    master, slave = os.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
    run_env = dict(env, COLUMNS=str(columns), LINES=str(rows), **program.env)
    run_env = {k: v for k, v in run_env.items() if v != ""}

    def controlling_tty() -> None:
        fcntl.ioctl(0, termios.TIOCSCTTY, 0)

    child = subprocess.Popen(program.argv, cwd=work, env=run_env, stdin=slave, stdout=slave,
                             stderr=slave, start_new_session=True, preexec_fn=controlling_tty,
                             close_fds=True)
    os.close(slave)
    try:
        open_ = _drain(master, recording, QUIET_S * 2, STEP_LIMIT_S * 2)
        recording.reads.append(None)
        for keys in program.keys:
            if not open_:
                recording.notes.append("closed before every key was sent")
                break
            if keys in (NARROW, WIDEN):
                size = (columns - columns // 3, rows) if keys == NARROW else (columns, rows)
                fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", size[1], size[0], 0, 0))
                recording.reads.append(("resize", *size))
            else:
                os.write(master, keys.encode())
            open_ = _drain(master, recording, QUIET_S, STEP_LIMIT_S)
            recording.reads.append(None)
        deadline = time.monotonic() + EXIT_LIMIT_S
        while open_ and time.monotonic() < deadline:
            open_ = _drain(master, recording, QUIET_S, EXIT_LIMIT_S)
            if child.poll() is not None:
                _drain(master, recording, 0.05, 0.5)
                break
        try:
            child.wait(timeout=1.0)
        except subprocess.TimeoutExpired:
            recording.notes.append("did not exit by itself; stopped")
            os.killpg(child.pid, signal.SIGKILL)
            child.wait(timeout=5)
    finally:
        os.close(master)
        if child.poll() is None:
            os.killpg(child.pid, signal.SIGKILL)
    return recording


def as_case(recording: Recording) -> dict:
    """A lab case: one write per read, a checkpoint wherever the program went
    quiet, cut short (and saying so) past the recorded-size limits."""
    ops: list = []
    total = 0
    for read in recording.reads:
        if read is None:
            if ops and ops[-1] != "checkpoint":
                ops.append("checkpoint")
            continue
        if isinstance(read, tuple):
            ops.append({"resize": list(read[1:])})
            continue
        if total + len(read) > MAX_RECORDED_BYTES or len(ops) >= MAX_RECORDED_OPS:
            recording.notes.append(f"cut at {total} bytes, {len(ops)} operations")
            break
        total += len(read)
        try:
            ops.append({"write": read.decode()})
        except UnicodeDecodeError:
            ops.append({"write_hex": read.hex()})
    columns, rows = recording.size
    return {"name": f"{RECORDED_VERSION}/{recording.program}-{columns}x{rows}",
            "size": [columns, rows], "ops": ops}


def available(program: Program) -> list[str]:
    """The executables `program` needs that this machine does not have."""
    return [n for n in program.needs if not shutil.which(n, path="/usr/bin:/bin")]


def record_all(only: list[str] | None = None) -> tuple[list[dict], list[dict]]:
    """Record every program at every size. Returns the cases that passed the
    leak gate and a report line for every program and size, recorded or not."""
    cases, report = [], []
    needles = machine_needles()
    with tempfile.TemporaryDirectory(prefix="td-lab-rec-") as scratch:
        for program in programs():
            if only and program.name not in only:
                continue
            missing = available(program)
            if missing:
                for columns, rows in SIZES:
                    report.append({"program": program.name, "size": [columns, rows], "recorded": False,
                                   "reason": f"not recorded: missing on this machine ({', '.join(missing)})"})
                continue
            for columns, rows in SIZES:
                # A fresh home and working directory per recording: nothing one
                # program leaves behind reaches the next.
                base = Path(tempfile.mkdtemp(dir=scratch))
                home, work = base / "home", base / "work"
                home.mkdir()
                work.mkdir()
                env = sterile_env(home)
                entry = {"program": program.name, "size": [columns, rows]}
                try:
                    write_fixtures(work, env)
                    recording = record(program, (columns, rows), work, env)
                except (OSError, subprocess.SubprocessError) as error:
                    report.append(dict(entry, recorded=False,
                                       reason=f"not recorded: {type(error).__name__}"))
                    continue
                reasons = leak_check(recording.data(), needles)
                if reasons:
                    report.append(dict(entry, recorded=False,
                                       reason="rejected by the leak gate: " + "; ".join(reasons)))
                    continue
                case = as_case(recording)
                if not any(isinstance(op, dict) for op in case["ops"]):
                    report.append(dict(entry, recorded=False, reason="not recorded: the program printed nothing"))
                    continue
                cases.append(case)
                report.append(dict(entry, recorded=True, case=case["name"], bytes=len(recording.data()),
                                   reads=sum(1 for r in recording.reads if isinstance(r, bytes)), notes=recording.notes))
    return cases, report


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("list")
    p = sub.add_parser("corpus")
    p.add_argument("--out", required=True)
    p.add_argument("--only", nargs="*")
    args = parser.parse_args(argv)
    if args.command == "list":
        for program in programs():
            missing = available(program)
            print(f"{program.name:<14} {'missing: ' + ', '.join(missing) if missing else 'ok'}")
        return 0
    cases, report = record_all(args.only)
    for line in report:
        print(json.dumps(line))
    Path(args.out).write_text(json.dumps({"format_version": 2, "corpus_version": RECORDED_VERSION,
                                          "cases": cases}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
