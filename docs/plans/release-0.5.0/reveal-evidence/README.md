# Reproduce the reveal evidence

There are two experiments. `reveal-poc.py` injects a non-answering file manager
to test TD's handling of it. `strata-reveal-stress.py` exercises real Strata
release binaries. Neither claims to reproduce the internal cause of the
three-day desktop freeze.

## TD protocol proof

Dependencies: `rustc`, `dbus-run-session`, `dbus-send`, Python 3 and PyGObject
with Gio. From this checkout, use an output directory that does not yet exist:

```sh
python3 docs/plans/release-0.5.0/reveal-poc.py --repo "$PWD" --out /tmp/td-reveal-proof-new
```

The harness extracts `path_to_file_uri`, `shell_quote`, `reveal_script` and the
timeout constant from `app/src/pane.rs`, compiles those exact functions with
`rustc`, and executes their generated shell command. It fails if the extraction
shape changes. It does not reimplement TD's command in Python.

Each case gets a new private D-Bus daemon and a real Gio FileManager1 service.
Display variables are removed. Fake `xdg-open` and `uwsm-app` programs record
their arguments; they never open an application. The real `dbus-send` handles
the request and timeout. Source and harness hashes, exact shell commands,
monotonic event times, subprocess status and assertions are retained.

The 13 cases cover:

- Healthy replies on both direct and uwsm launch paths: no fallback.
- Six nonreply trials, three per path: one fallback with the parent folder.
- Explicit rejection: immediate fallback.
- A reply delayed five seconds: fallback precedes the late reply.
- A fallback that also stalls: the harness stops it at five seconds; TD itself
  has no end-to-end deadline.
- The old command with its reply cap removed: approximately 25 seconds. This
  is a negative control which fails the current behavior's 4.5-second bound.
- A deliberately wrong fallback target: the parent-folder oracle detects it.

Every case uses a name containing spaces, apostrophes, percent signs, a comma,
Unicode, `$HOME` and literal shell command-substitution syntax. The service must
receive exactly one correctly encoded URI; the fallback must receive exactly
one parent path. The marker command must not execute. This file is an empty
protocol fixture: the PoC performs no MP4 decoding.

The recorded `protocol/receipt.json` has 13 passing case assertions. Negative
controls are explicitly labelled; their intentionally degraded behavior is not
a passing product result. `old-uncapped` took 25.063 seconds; six current silent
cases took 3.069–3.070 seconds. Event timestamps put actual fallback invocation
3.024–3.055 seconds after receipt. The larger process elapsed includes the
parent's wait/poll overhead.

## Real Strata releases

The fixed environment is upstream's verified test image:

```text
ghcr.io/lgse/strata-test-image@sha256:d8bf7eb45b87a52921d41f7fa734eee167180f8ab2d4f2d734347df6d06e8d13
org.strata.test.inputs=94b0518ccf9fbc4a1f0badb87c82551639b6017c2a8222586d9f7026a67938ef
GTK 4.22.5, GLib 2.88.3
```

Upstream's `scripts/image_base.py ensure --engine docker` verified the image
label against its current source inputs before these runs. The host's installed
Strata uses GTK 4.22.4; the different toolkit, X11/Cairo rendering and resource
limits matter when interpreting the results.

Download the official archives and check each published checksum:

```sh
probe_root=$(mktemp -d /tmp/strata-release-proof.XXXXXX)
mkdir -p "$probe_root/releases" "$probe_root/evidence"
for probe_version in 0.21.0 1.1.0; do
  probe_name="strata-$probe_version-x86_64-unknown-linux-gnu.tar.gz"
  curl -fL "https://github.com/lgse/strata/releases/download/v$probe_version/$probe_name" -o "$probe_root/releases/$probe_name"
  curl -fL "https://github.com/lgse/strata/releases/download/v$probe_version/$probe_name.sha256" -o "$probe_root/releases/$probe_name.sha256"
  (cd "$probe_root/releases" && sha256sum -c "$probe_name.sha256" && tar -xzf "$probe_name")
done
```

The image's account IDs differ from the host's. Supply minimal identity files
so D-Bus can resolve the non-root test user. The first attempted container run
omitted these files and failed before any application started; it is not a
Strata test result.

```sh
probe_uid=$(id -u)
probe_gid=$(id -g)
printf 'root:x:0:0:root:/root:/bin/sh\nprobe:x:%s:%s:Probe:/tmp:/bin/sh\n' "$probe_uid" "$probe_gid" > "$probe_root/passwd"
printf 'root:x:0:\nprobe:x:%s:\n' "$probe_gid" > "$probe_root/group"
probe_image=ghcr.io/lgse/strata-test-image@sha256:d8bf7eb45b87a52921d41f7fa734eee167180f8ab2d4f2d734347df6d06e8d13
docker pull "$probe_image"
```

Run these sequentially, so the trials do not compete with each other. On this
machine `sudo -n docker` was needed for socket access; no desktop directories,
sockets or devices are mounted. Run as a non-root host user.

```sh
for probe_version in 0.21.0 1.1.0; do
  docker run --rm --network none --cap-drop ALL --security-opt no-new-privileges --user "$probe_uid:$probe_gid" --memory 3g --pids-limit 512 --cpus 2 --shm-size 512m --tmpfs /tmp:rw,exec,size=512m -e STRATA_PROBE_CONTAINER=1 -v "$probe_root/passwd:/etc/passwd:ro" -v "$probe_root/group:/etc/group:ro" -v "$probe_root/releases:/releases:ro" -v "$PWD/docs/plans/release-0.5.0/strata-reveal-stress.py:/probe.py:ro" -v "$probe_root/evidence:/evidence" "$probe_image" dbus-run-session -- python3 /probe.py "$probe_version"
done
```

Each process has a private Xvfb and bus, fresh configuration, no network or GPU,
and a generated one-second blue MP4 plus text file. It alternates up to 60
ShowItems requests, waiting 250 ms after each completed request, with a
three-second reply deadline. Each reveal opens another window. The first
timeout stops the load; after five seconds idle, a ten-second-deadline request
tests whether the application can still answer. Application and display logs,
per-request output, final memory/process state, binary/fixture/harness hashes,
toolkit versions and recovery measurements are written to the evidence mount.

For a repeat, add `-e POC_LABEL=-repeat`; the output directory must be new.
The script records nonzero request status as data and exits normally after
writing the receipt. Read `failures` and `recovery_probe`, not just the runner's
exit code. Harness/startup exceptions fail the runner.

The exploratory first runs predate the idle-recovery and toolkit metadata
fields. Their `packages` string was empty because the package query did not
produce a value; it is **unknown**, not a successful version check. The repeat
receipts record toolkit versions through `pkg-config` and carry the final
harness hash. Keep those distinctions when comparing receipts.

The private bus emits portal-backend and FUSE warnings because the container
has no full desktop services. These are retained in the run output. Requests
are made directly after Strata owns FileManager1. An X11 test with dozens of
accumulated windows is not evidence that the original three-window Wayland
freeze has been reproduced or fixed.

## What would change the conclusion

Recorded real-release trials, run sequentially with the same limits:

| Release / run | Requests before stopping | Three-second timeouts | Median reply | Maximum | Probe after five seconds idle |
|---|---:|---:|---:|---:|---|
| [0.21.0 first](strata/0.21.0/receipt.json) | 29 | 1 | 0.979 s | 3.006 s (timeout) | Not collected |
| [0.21.0 repeat](strata/0.21.0-repeat/receipt.json) | 30 | 1 | 0.894 s | 3.004 s (timeout) | Replied in 4.124 s, using a 10 s deadline |
| [1.1.0 first](strata/1.1.0/receipt.json) | 60 | 0 | 0.282 s | 0.613 s | Not needed; no timeout |
| [1.1.0 repeat](strata/1.1.0-repeat/receipt.json) | 60 | 0 | 0.373 s | 0.689 s | Not needed; no timeout |

The old-version medians include the one timeout measurement in each run; they
are summaries of request elapsed time, not completed-opens-only distributions.
The final old-version receipt logs 32 presented windows (initial, requests and
the recovery probe). The final new-version receipt logs 61 (initial plus 60).
This is an accumulation experiment. **The original persistent freeze was not
reproduced.** The old binary remained capable of answering with a longer
deadline. There was no source bisect, so no particular upstream performance
commit is credited as the cause of the improvement.

`provenance.json` records release source commits, binary and harness hashes,
container identity, limits and the incompatible debug build IDs. Each trial
directory contains `requests.jsonl`, the application log, display log and
receipt. `incident-main-thread.txt` is a sanitized excerpt of the original
desktop capture; `after-restart.json` and `mp4-after-restart.json` preserve its
recovery measurements. No desktop window inventory or core dump is published.

A healthy fresh process does not invalidate an intermittent lifetime bug.
Reproduce the original freeze with a bounded sequence on a current supported
build and capture the matching build ID, process maps, source callback stack,
window lifecycle and D-Bus timing before restarting it. The downloadable
0.21.0 release debug symbols have build ID
`24cdfe2aa74373ea7ee60f85cb32d4cff8e8a023`; the installed Arch executable has
`d10b10288cb903abe2918eeaed5779a5763f8c89`. They must not be combined.

For TD, rerun the protocol PoC against the exact candidate source. A missing
cap, wrong parent, quoting error or unexpected fallback should fail the
assertions. Actual Ctrl+Shift-click routing and first-painted-frame timing are
separate candidate checks; this script begins at the production command
generator, not at a physical mouse event.

## Desktop upgrade to 1.1.0

After [upstream closed the report as resolved](https://github.com/lgse/strata/issues/1833#issuecomment-6372286779), the official 1.1.0 release was installed on the affected Wayland desktop. [The upgrade receipt](desktop-upgrade-1.1.0.json) records checksum and attestation verification, the running binary hash, the FileManager1 owner and four successful MP4 reveal timings. Cold activation replied in 0.8793 seconds; warm requests in 0.0799–0.0908 seconds. Multi-day reliability and physical click-to-frame timing were not measured.
