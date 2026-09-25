#!/usr/bin/env python3
"""Run an image program under a fake terminal and record what it sends.

The fake terminal answers the questions image programs ask (DA1, XTVERSION,
the size window-ops, colour queries, Kitty `a=q`) according to an IDENTITY,
and records every escape sequence the program prints. It never draws.

Usage: fakepty.py IDENTITY OUTFILE.json -- cmd args...
IDENTITY:
  td-today   what TD answers on main at 81ec545: DA1 ?6c, 14t/16t/18t, no
             XTVERSION, no Kitty reply, no colour replies (#719)
  td-kitty   td-today plus Kitty graphics answered OK and XTVERSION "TD"
  kitty      what kitty itself answers: Kitty OK, DA1 ?62;c, XTVERSION kitty,
             colour replies, TERM=xterm-kitty in the environment
"""
import json, os, pty, re, select, signal, struct, sys, termios, fcntl, time, base64, collections

ROWS, COLS, CW, CH = 40, 120, 10, 22   # cells and a device cell, like a 1.6x monitor

IDS = {
    "td-today": dict(da1=b"\x1b[?6c", xtversion=None, kitty=False, colours=False, env={"TERM": "alacritty", "COLORTERM": "truecolor"}),
    "td-kitty": dict(da1=b"\x1b[?6c", xtversion=b"TD(0.3.0)", kitty=True, colours=False, env={"TERM": "alacritty", "COLORTERM": "truecolor"}),
    "kitty":    dict(da1=b"\x1b[?62;c", xtversion=b"kitty(0.43.1)", kitty=True, colours=True, env={"TERM": "xterm-kitty", "KITTY_WINDOW_ID": "1", "TERM_PROGRAM": "kitty"}),
}

def main():
    ident, out = sys.argv[1], sys.argv[2]
    cmd = sys.argv[sys.argv.index("--") + 1:]
    idn = IDS[ident]
    env = dict(os.environ)
    for k in ("KITTY_WINDOW_ID", "KITTY_PID", "TERM_PROGRAM", "GHOSTTY_BIN_DIR", "WEZTERM_EXECUTABLE", "TMUX", "TD_SESSION", "TD_PANE_ID", "TD_TAG"):
        env.pop(k, None)
    env.update(idn["env"])
    env["COLUMNS"], env["LINES"] = str(COLS), str(ROWS)
    pid, fd = pty.fork()
    if pid == 0:
        os.execvpe(cmd[0], cmd, env)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, COLS * CW, ROWS * CH))
    os.kill(pid, signal.SIGWINCH)

    events, counts = [], collections.Counter()
    apc_keys = collections.Counter()
    total = 0
    placeholder_cells = 0
    b64_bytes = 0
    frames_T = 0
    st = "ground"; buf = bytearray(); text = bytearray()
    raw_path = os.environ.get("FAKEPTY_RAW")
    raw_out = open(raw_path, "wb") if raw_path else None
    t0 = time.time()
    deadline = t0 + float(os.environ.get("FAKEPTY_SECS", "8"))

    def reply(b):
        try:
            os.write(fd, b)
        except OSError:
            pass
        events.append({"t": round(time.time() - t0, 3), "reply": b.decode("latin1")[:120]})

    def csi(seq):
        s = seq.decode("latin1")
        counts["CSI " + re.sub(r"\d+", "n", s)] += 1 if not re.fullmatch(r"[\d;]*[HfmKJABCDGdlh]|\?[\d;]*[hl]", s) else 0
        if s in ("c", "0c"):
            events.append({"t": round(time.time() - t0, 3), "q": "DA1"}); reply(idn["da1"])
        elif s == ">q" or s == ">0q":
            events.append({"t": round(time.time() - t0, 3), "q": "XTVERSION"})
            if idn["xtversion"]:
                reply(b"\x1bP>|" + idn["xtversion"] + b"\x1b\\")
        elif s == "14t":
            events.append({"t": round(time.time() - t0, 3), "q": "14t"}); reply(b"\x1b[4;%d;%dt" % (ROWS * CH, COLS * CW))
        elif s == "16t":
            events.append({"t": round(time.time() - t0, 3), "q": "16t"}); reply(b"\x1b[6;%d;%dt" % (CH, CW))
        elif s == "18t":
            events.append({"t": round(time.time() - t0, 3), "q": "18t"}); reply(b"\x1b[8;%d;%dt" % (ROWS, COLS))
        elif s == "5n":
            events.append({"t": round(time.time() - t0, 3), "q": "DSR5"}); reply(b"\x1b[0n")
        elif s == "6n":
            events.append({"t": round(time.time() - t0, 3), "q": "CPR"}); reply(b"\x1b[1;1R")
        elif s.startswith("?") and s.endswith("$p"):
            events.append({"t": round(time.time() - t0, 3), "q": "DECRQM " + s})
        elif s.startswith("?") and s.endswith("S"):
            events.append({"t": round(time.time() - t0, 3), "q": "XTSMGRAPHICS " + s})
        elif s == "?u":
            events.append({"t": round(time.time() - t0, 3), "q": "kitty-kbd"})

    def osc(body):
        s = body.decode("latin1", "replace")
        num = s.split(";", 1)[0]
        counts["OSC " + num] += 1
        if s.endswith(";?") and num in ("10", "11", "12", "4"):
            events.append({"t": round(time.time() - t0, 3), "q": "OSC " + num + " ?"})
            if idn["colours"]:
                reply(("\x1b]%s;rgb:0000/0000/0000\x1b\\" % num).encode())
        elif num == "1337":
            events.append({"t": round(time.time() - t0, 3), "iterm2": s[:80], "len": len(body)})

    def apc(body):
        nonlocal b64_bytes, frames_T
        if not body.startswith(b"G"):
            counts["APC other"] += 1; return
        ctrl, _, payload = body[1:].partition(b";")
        kv = dict(p.split(b"=", 1) for p in ctrl.split(b",") if b"=" in p)
        k = {a.decode(): b.decode("latin1") for a, b in kv.items()}
        for key in k:
            apc_keys[key + "=" + (k[key] if key in "aftUqCmtoI" or key in ("a", "t", "f", "U", "q", "C", "m", "o", "d") else "*")] += 1
        b64_bytes += len(payload)
        a = k.get("a", "t")
        if a == "T":
            frames_T += 1
        if len([e for e in events if "apc" in e]) < 25 or a in ("q", "d", "p"):
            events.append({"t": round(time.time() - t0, 3), "apc": ",".join(f"{x}={y}" for x, y in k.items()), "payload": len(payload)})
        if idn["kitty"] and "i" in k and k.get("m", "0") == "0" and k.get("q", "0") in ("0", "1") and a in ("q", "t", "T", "p"):
            if k.get("q", "0") == "0":
                tail = (",p=" + k["p"]) if "p" in k else ""
                reply(("\x1b_Gi=%s%s;OK\x1b\\" % (k["i"], tail)).encode())
        # a kitty would consume temp files and shm; do the same so clients do not hang or leak
        if k.get("t") in ("t",) and payload and not os.environ.get("FAKEPTY_KEEP_TEMP"):
            try:
                p = base64.b64decode(payload).decode()
                if "tty-graphics-protocol" in p and os.path.isfile(p):
                    os.unlink(p)
            except Exception:
                pass

    while time.time() < deadline:
        r, _, _ = select.select([fd], [], [], 0.05)
        if not r:
            try:
                if os.waitpid(pid, os.WNOHANG)[0]:
                    break
            except ChildProcessError:
                break
            continue
        try:
            data = os.read(fd, 65536)
        except OSError:
            break
        if not data:
            break
        total += len(data)
        if raw_out is not None:
            raw_out.write(data)
        placeholder_cells += data.count("\U0010EEEE".encode())
        for byte in data:
            c = bytes([byte])
            if st == "ground":
                if byte == 0x1b: st = "esc"
                elif len(text) < 600 and (byte >= 0x20 or byte in (0x0a,)) and byte < 0x80: text.append(byte)
            elif st == "esc":
                buf = bytearray()
                st = {0x5b: "csi", 0x5d: "osc", 0x50: "dcs", 0x5f: "apc", 0x5e: "str", 0x58: "str"}.get(byte, "ground")
            elif st == "csi":
                buf += c
                if 0x40 <= byte <= 0x7e:
                    csi(bytes(buf)); st = "ground"
            elif st in ("osc", "dcs", "apc", "str"):
                if byte == 0x07 and st == "osc":
                    osc(bytes(buf)); st = "ground"
                elif byte == 0x1b:
                    st = st + "-esc"
                else:
                    buf += c
            elif st.endswith("-esc"):
                kind = st[:-4]
                if byte == 0x5c:
                    if kind == "osc": osc(bytes(buf))
                    elif kind == "apc": apc(bytes(buf))
                    elif kind == "dcs":
                        counts["DCS " + chr(buf[0]) if buf else "DCS"] += 1
                        if b"q" in bytes(buf[:12]): counts["DCS sixel?"] += 1
                    st = "ground"
                else:
                    st = "ground"
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    res = {
        "identity": ident, "cmd": cmd, "bytes_out": total, "secs": round(time.time() - t0, 2),
        "placeholder_cells_U10EEEE": placeholder_cells, "apc_payload_bytes": b64_bytes,
        "apc_a_T_count": frames_T,
        "apc_keys": dict(apc_keys.most_common(40)),
        "seq_counts": {k: v for k, v in counts.most_common(40) if v},
        "events": events[:80],
        "text": text.decode("latin1"),
    }
    json.dump(res, open(out, "w"), indent=1)
    print(json.dumps({k: res[k] for k in ("identity", "secs", "text", "bytes_out","placeholder_cells_U10EEEE", "apc_payload_bytes", "apc_a_T_count", "apc_keys")}))
    print("  queries/replies:", [e.get("q") or ("R:" + e["reply"][:24] if "reply" in e else None) or ("APC " + e["apc"][:60] if "apc" in e else "") for e in events[:30]])

if __name__ == "__main__":
    main()
