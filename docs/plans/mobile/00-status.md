# Status: Terminal Delight on the phone

**Difficulty: 6/10.** The screens are cheap to get wrong, because a wrong screen
is visible the first time anyone opens it. The part that can be wrong for a long
time without anyone noticing is the door: a network listener that can type into
a shell. That door is small, so this gets one combined plan page
(`01-plan.md`) rather than four gates.

**Approval.** Parker's opening message was the product brief and said
"surprise me", so the tracer slice was built in the same sitting against this
page rather than held for a sign-off. Everything after the tracer waits for his
notes on `01-plan.md`.

## Slices

- [x] **Slice 0 — the tracer.** DONE 2026-09-25, on the S21 FE over USB. A
      gateway on the laptop (`mobile/gateway`, a user service on :7717), a
      Flutter app on the phone (`mobile/app`), paired by `td-mobile-gateway
      pair`. Proved on the device, by screenshot, not by argument:
      - the wall draws session 1's thirty panes in the desk's projects, groups
        and colours, with the desk's sticky notes, in 63 ms from the gateway;
      - a pane's workbench shows the person's prompt and the agent's response
        card with its readings;
      - **New → Terminal** started a shell on the host from the phone, and
        `echo hello from the phone && uname -n` typed on the phone's keyboard
        answered `legion`;
      - **New → Claude** started Claude Code on the host; a message sent from
        the compose line was answered in six seconds, and the answer arrived on
        the phone's workbench as a card.
      Three things measured on the way, each now pinned by a test: xterm.dart
      reads Claude Code's `ESC[>4;2m` as underline-and-faint (the SGR filter
      drops it — the test fails with the fix removed); the channel records the
      harness's own injected turns as prompts, which must not be shown as the
      person's words; and an agent with no file or MCP route prints its card as
      a ```td fence, which the phone now lifts into a card the way the desk does.
- [ ] Slice 1 — say a line to a desk agent without taking its pane (needs a
      pane-addressed `bench say` on the window's control socket).
- [ ] Slice 2 — answer an agent's question from the phone through the channel's
      answer file.
- [ ] Slice 3 — push notifications when a pane starts needing you.
- [ ] Slice 4 — both screens live on one pane (a shared stream in the host;
      waits for the next natural host restart, because a host is never
      restarted to ship a feature).
- [ ] Later — the CRT warp on the phone.
