# Strata reveal qualification

Measured 2026-10-23. Difficulty predicted 3/10; post-investigation 3/10 for diagnosis and recovery. Upstream closed the report as resolved in Strata 1.1.0; that release is now installed and passes desktop reveal checks.

Ctrl+Shift-click reveals a local file in its containing folder. Terminal Delight sends `org.freedesktop.FileManager1.ShowItems`, then opens the parent with `xdg-open` if that request fails. On this desktop, both paths reach Strata.

The reported 10–30 second delay accompanied an unresponsive Strata 0.21.0-2 process that had run for about three days. Direct calls reproduced the failure without TD: ShowItems reached a 4-second deadline (4.006 s), ShowFolders reached a 3-second deadline (3.006 s), and `strata /tmp` exceeded a 5-second deadline. Introspection and Peer.Ping still answered. A text file failed too; an MP4-specific cause is unproven.

## Recovery and limits

After recording the windows and stack samples, the affected process was stopped with SIGTERM and the same binary relaunched through `uwsm-app`. Three existing Strata windows closed; their browsing locations were not recoverable from the collected window metadata. TD and its agents were not restarted. ShowItems then replied in 0.400 s and ShowFolders in 0.236 s. Three reveals of a generated one-second MP4 replied in 0.132, 0.145 and 0.154 s. These measure method replies, not first painted frames or a physical mouse click.

Two main-thread samples showed `g_weak_ref_get` beneath GLib event dispatch, with stripped application frames. This does not identify a faulty callback or establish a deadlock. The user's repeated desktop “Application Not Responding” report agrees with the independent method and CLI failures. Restart restored service; it did not fix recurrence.

Upstream investigation: [Long-running Strata stops answering reveals and folder opens](https://github.com/lgse/strata/issues/1833). The report records reproduction after the freeze, version, environment, timing, invalidation criteria and missing trigger. The original investigation retained the old version. After the maintainer reviewed the release comparison, [they closed the report as resolved in 1.1.0](https://github.com/lgse/strata/issues/1833#issuecomment-6372286779) and invited a reopen if the failure recurs on that release or later. The desktop was then upgraded at the user's request, as recorded below.

The [cold-activation portal fix](https://github.com/lgse/strata/pull/1811) addresses a different state: GTK initialization waits on a portal that is waiting for Strata. Today's process was already running, and the probes called it directly. Do not mark this recurrence fixed from that PR alone.

## Terminal Delight release disposition

[The existing three-second reveal timeout](https://github.com/parker-brown-family/terminal-delight/pull/1023) is already merged and present on `dev/0.5.0`, as well as the source of the installed `td-9fbcdfc4-0.5-war-room-tiles` window binary. `app/src/pane.rs::reveal_script` uses `REVEAL_REPLY_TIMEOUT_MS = 3000`; `reveal_script_asks_dbus_first_then_falls_back_to_the_folder` asserts the cap and containing-folder fallback.

The cap bounds the first request, not the whole opening operation. The fallback can still reach a frozen manager. A slow manager may also answer after the fallback launches, producing a second window. TD should not terminate a user's file manager or silently substitute another application.

No new TD implementation change was made. This documentation belongs on `dev/0.5.0` as candidate qualification evidence, not as a claim that 0.5.0 fixes Strata freezes. No binary, release tag or installed TD launcher was changed by this investigation.

## Repeatable proof

The [reproduction guide](reveal-evidence/README.md) and two executable harnesses now accompany this record. The TD protocol harness compiles the actual reveal functions from `app/src/pane.rs`; it uses a private D-Bus service and recording-only launcher stubs, without touching the desktop.

All 13 case assertions passed. Six no-reply trials, across direct and uwsm paths, fell back in 3.069–3.070 seconds. Removing the cap produced a 25.063-second wait. The tests also cover immediate rejection, healthy replies with no fallback, five-second late replies, a hanging fallback, exact URI/parent arguments and shell quoting. A deliberately wrong parent is detected. Raw events, commands, source hashes and the [receipt](reveal-evidence/protocol/receipt.json) are retained.

The late-reply case confirms that a successful reply can arrive after fallback; it does not render duplicate windows. The hanging-fallback case reaches the harness's five-second watchdog: TD's three-second request cap is not an end-to-end deadline. Neither injected failure is a reproduction of Strata's internal bug.

Separately, the real-release harness opens synthetic MP4/text fixtures on a private Xvfb and D-Bus inside upstream's pinned test image. It accumulates windows, so this is a load test, not a recreation of the user's three-window desktop. The initial 0.21.0 release trial reached its three-second deadline on request 29; the repeat did so on request 30. After five seconds idle, the repeat answered a ten-second-deadline probe in 4.124 seconds. That demonstrates a slow but still responsive process, not the original persistent freeze. The first 1.1.0 trial answered all 60 requests, with a 0.282-second median and 0.613-second maximum. Raw repeat results are linked in the evidence guide.

These releases use a different build from the installed Arch package, and the test uses GTK 4.22.5/X11/Cairo rather than GTK 4.22.4/Wayland/NVIDIA. Portal warnings and resource limits are recorded. The old release's poor performance under this load is reproduced; a version upgrade has not been proved to cure the original lifetime failure. The installed version was unchanged during those trials; the subsequent desktop upgrade is recorded below.

The 1.1.0 repeat also answered all 60 requests (0.373-second median, 0.689-second maximum): 120 successful requests over two fresh processes. This is evidence for improved behavior in this test, not proof of multi-day reliability. Both final harnesses and their results are linked from the reproduction guide; the original desktop capture remains separate from the synthetic load results.

Before admitting this behavior as qualified for a candidate:

- Run the existing reveal tests on the candidate's exact source and retain their nonzero test count.
- On a healthy manager, verify Ctrl+Shift-click selects an MP4 and a text file; record both reply and visible-window timing.
- Rerun the now-passing private-bus protocol proof against the exact candidate source; preserve its receipt and source hash.
- If the desktop manager itself is frozen, record it as an external failure. A passing fresh-process test cannot establish multi-day reliability.

## Desktop upgrade after upstream closure

The official 1.1.0 archive passed its SHA-256 check and GitHub Actions attestation verification. Omarchy's live package repository still offered 1.0.0-1, so the official release was installed at `~/.local/bin/strata`, with `~/bin/strata` pointing to it because this desktop searches `/usr/bin` before `~/.local/bin`. The per-user desktop entry and FileManager1 D-Bus service both name the new binary by absolute path. The existing Strata directory association was retained. The old pacman package remains installed to retain its managed dependencies; its binary is no longer the normal desktop or shell launch target.

The old process exited with SIGTERM. A fresh ShowItems request activated 1.1.0 through D-Bus and replied in **0.8793 seconds**. Three warm requests for the same generated one-second MP4 replied in **0.0809, 0.0799 and 0.0908 seconds**. All returned successfully. The bus owner was verified as the process executing the new binary, and Hyprland reported its windows mapped and visible. These are method-reply timings, not physical click or painted-frame measurements. Multi-day use has not yet been measured.

The executable reports `strata 1.1.0` and has SHA-256 `fbb77cab62ec9eda6b9d22fb35204304ca9d8a6bd95a8632246611cb46a184e4`, matching the release already tested in isolation. Settings were backed up under `~/.local/state/strata/upgrade-2026-10-23/` before restart. [Desktop upgrade receipt](reveal-evidence/desktop-upgrade-1.1.0.json).

## Evidence retained locally

`/home/parker/Work/reports/strata-reveal-evidence/` holds the pre-restart thread dump and window inventory, restart log, and post-restart JSON timings. Those local diagnostics are not published because window metadata can include private paths. The upstream report contains the sanitized observations. No core dump was collected.

Validation: the private-bus PoC passed 13 assertions, real release trials produced per-request receipts, Python syntax compilation passed, and the documentation/source diff was reviewed with `git diff --check`. No application source changed; no full local Rust application build or suite was run. The protocol harness compiled and executed the extracted production Rust functions directly.
