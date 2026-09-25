# Terminal Delight on the phone

Two pieces, and the plan they come from is `docs/plans/mobile/01-plan.md`.

```
mobile/gateway   the door on the laptop — Rust, speaks the session host protocol
mobile/app       the phone — Flutter, the workbench first and the terminal behind it
```

## The gateway

Build and install:

```sh
cargo build --release --manifest-path mobile/gateway/Cargo.toml
```
```sh
install -Dm755 mobile/gateway/target/release/td-mobile-gateway ~/.local/bin/td-mobile-gateway
```

Run it for good, as a user service:

```sh
install -Dm644 mobile/gateway/td-mobile-gateway.service ~/.config/systemd/user/td-mobile-gateway.service
```
```sh
systemctl --user daemon-reload && systemctl --user enable --now td-mobile-gateway
```

It listens on `127.0.0.1:7717` and on the laptop's tailnet address. It reads
the session sockets under `$XDG_RUNTIME_DIR/terminal-delight/`, the mailboxes
under `~/.local/state/terminal-delight/surfaces/`, and the layouts under
`~/.config/terminal-delight/sessions/`. It writes nothing but its own token.

**Pair the phone** — plugged in over USB:

```sh
td-mobile-gateway pair
```

The phone shows a *Pair with legion?* sheet with each route already probed.
Accept it. `td-mobile-gateway token --rotate` unpairs every phone.

## The app

```sh
cd mobile/app && flutter build apk --release --target-platform android-arm64
```
```sh
adb install -r mobile/app/build/app/outputs/flutter-apk/app-release.apk
```

Release builds only; `android/SIGNING.md` says why and where the key is.

| Route | Address | Needs |
|---|---|---|
| USB | `http://127.0.0.1:7717` | the cable, and `pair` (which runs `adb reverse`) |
| Tailscale | `http://100.122.239.70:7717` | Tailscale switched on on the phone |
| Wi-Fi | `http://192.168.1.x:7717` | the gateway started with `serve --lan` |

## What it can do, and what it will not

- The **wall** draws the session the way the desk's left bar does — project,
  group, tab, in their colours — with each pane's state (*needs you*,
  *working*, *done*, or *unknown* when a pane has no hooked agent) and the
  newest thing its agent said.
- The **workbench** face of a pane reads the pane's mailbox: your own prompts,
  every response card with its registers, decisions, questions, documents.
- The **terminal** face attaches to the pane on the laptop's session host.
  A terminal started from the phone (**New**) is the phone's to drive. A pane a
  desk window is showing is **not** taken unless you say so, because the host
  gives a terminal to whoever opened it last and the desk's copy would freeze.
- **✎** on the key row is a compose line: dictate or type a whole prompt with
  the phone's own keyboard, and it is delivered as one bracketed paste, the
  way the desk's bench delivers a message.
