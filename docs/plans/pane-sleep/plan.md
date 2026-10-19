# Plan: Hidden pane housekeeping

Difficulty 5/10. The direction is approved in the conversation; implementation and verification proceed autonomously.

## What it is for

Hidden panes still run a housekeeping callback every 150 ms although their screen checks run about once per second. Host-owned panes also start a local foreground watcher that cannot find a local PTY descriptor. Remove these callbacks while agents and terminal byte readers continue running.

## The shape

Keep the existing event pump and host protocol. Only start the local foreground watcher when the session owns a master descriptor and knows its child PID. Retain one hidden housekeeping callback per second; visible idle panes keep their 150 ms cadence and visible animations their 33 ms cadence. A bounded futures channel wakes the clock on visibility edges and newly staged resize targets. Repeated draws and streamed bytes do not wake it. Housekeeping chooses the earlier of its fallback and a pending resize or completion deadline. The concurrently approved palette change retires reels entirely; this task retains that removal and schedules only the remaining CRT/status housekeeping. Completion deadlines require an observed Some(false) thinking state; unobserved state cannot schedule a finish.

`TerminalView::mark_shown(&self, shown: bool)` replaces the workspace's raw atomic store and wakes only on an edge. `wake_housekeeping(&self)` coalesces urgent events. `housekeeping_delay(&self)` preserves pending deadlines. The window draws fresh terminal content on activation regardless of clock state.

## Least confident decisions

1. One second is retained as a bounded hidden status fallback. This reduces overhead without inventing a complete list of output-independent agent state changes; indefinite parking is deferred.
2. A wake may occur while a timer is being replaced. A bounded queued channel must retain it, and tests must hit wake-before-wait, repeated visibility writes and mid-sleep resize.
3. Completion debounce and scroll-settle interact. A due deadline must not create a zero-delay busy loop or ring from a scrolling screen.
4. Timer callback reduction is not CPU reduction. Alternate matched normal production binaries under quiet and printing hidden-pane workloads; report raw pairs and uncertainty, including a negligible saving if that is what appears.

## Slices

1. Watcher ownership guard and real attached/local fixture controls.
2. Visibility/resize wake and bounded hidden scheduling, preserving terminal queries, output, prompt/finish status and visible panes regardless of focus.
3. Deliberately broken production mutations, full required checks, matched native window CPU measurements, then merge, rebuild, smoke and safe GUI-only reconnect with the existing host.
