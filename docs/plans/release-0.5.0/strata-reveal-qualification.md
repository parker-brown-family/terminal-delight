# Strata reveal qualification

Measured 2026-10-23. Difficulty predicted 3/10; post-investigation 3/10 for diagnosis and recovery. The recurring upstream freeze remains unresolved.

Ctrl+Shift-click reveals a local file in its containing folder. Terminal Delight sends `org.freedesktop.FileManager1.ShowItems`, then opens the parent with `xdg-open` if that request fails. On this desktop, both paths reach Strata.

The reported 10–30 second delay accompanied an unresponsive Strata 0.21.0-2 process that had run for about three days. Direct calls reproduced the failure without TD: ShowItems reached a 4-second deadline (4.006 s), ShowFolders reached a 3-second deadline (3.006 s), and `strata /tmp` exceeded a 5-second deadline. Introspection and Peer.Ping still answered. A text file failed too; an MP4-specific cause is unproven.

## Recovery and limits

After recording the windows and stack samples, the affected process was stopped with SIGTERM and the same binary relaunched through `uwsm-app`. Three existing Strata windows closed; their browsing locations were not recoverable from the collected window metadata. TD and its agents were not restarted. ShowItems then replied in 0.400 s and ShowFolders in 0.236 s. Three reveals of a generated one-second MP4 replied in 0.132, 0.145 and 0.154 s. These measure method replies, not first painted frames or a physical mouse click.

Two main-thread samples showed `g_weak_ref_get` beneath GLib event dispatch, with stripped application frames. This does not identify a faulty callback or establish a deadlock. The user's repeated desktop “Application Not Responding” report agrees with the independent method and CLI failures. Restart restored service; it did not fix recurrence.

Upstream investigation: [Long-running Strata stops answering reveals and folder opens](https://github.com/lgse/strata/issues/1833). The report records reproduction after the freeze, version, environment, timing, invalidation criteria and missing trigger. The installed old version was retained to avoid claiming an unverified upgrade solves this. Current Strata has not been qualified against this multi-day failure.

The [cold-activation portal fix](https://github.com/lgse/strata/pull/1811) addresses a different state: GTK initialization waits on a portal that is waiting for Strata. Today's process was already running, and the probes called it directly. Do not mark this recurrence fixed from that PR alone.

## Terminal Delight release disposition

[The existing three-second reveal timeout](https://github.com/parker-brown-family/terminal-delight/pull/1023) is already merged and present on `dev/0.5.0`, as well as the source of the installed `td-9fbcdfc4-0.5-war-room-tiles` window binary. `app/src/pane.rs::reveal_script` uses `REVEAL_REPLY_TIMEOUT_MS = 3000`; `reveal_script_asks_dbus_first_then_falls_back_to_the_folder` asserts the cap and containing-folder fallback.

The cap bounds the first request, not the whole opening operation. The fallback can still reach a frozen manager. A slow manager may also answer after the fallback launches, producing a second window. TD should not terminate a user's file manager or silently substitute another application.

No new TD implementation change was made. This documentation belongs on `dev/0.5.0` as candidate qualification evidence, not as a claim that 0.5.0 fixes Strata freezes. No binary, release tag or installed TD launcher was changed by this investigation.

Before admitting this behavior as qualified for a candidate:

- Run the existing reveal tests on the candidate's exact source and retain their nonzero test count.
- On a healthy manager, verify Ctrl+Shift-click selects an MP4 and a text file; record both reply and visible-window timing.
- With an isolated unresponsive FileManager1 service, verify fallback starts after about three seconds and targets the parent directory. This controlled failure test was not run in this investigation.
- If the desktop manager itself is frozen, record it as an external failure. A passing fresh-process test cannot establish multi-day reliability.

## Evidence retained locally

`/home/parker/Work/reports/strata-reveal-evidence/` holds the pre-restart thread dump and window inventory, restart log, and post-restart JSON timings. Those local diagnostics are not published because window metadata can include private paths. The upstream report contains the sanitized observations. No core dump was collected.

Validation of this documentation change: compare the timeout and test against current development source, review linked upstream reports and the complete diff, and run `git diff --check`. No application source changed; no full Rust build or suite was run for this documentation-only update.
