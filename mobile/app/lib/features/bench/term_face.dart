import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/features/bench/prose.dart';
import 'package:terminal_delight/features/bench/sgr_filter.dart';
import 'package:xterm/xterm.dart';

/// The Hacker theme's sixteen colours, as a terminal palette.
final tdTerminalTheme = TerminalTheme(
  cursor: Td.cursor,
  selection: Td.accent.withValues(alpha: 0.35),
  foreground: Td.ink,
  background: Colors.transparent,
  black: Td.ansi[0],
  red: Td.ansi[1],
  green: Td.ansi[2],
  yellow: Td.ansi[3],
  blue: Td.ansi[4],
  magenta: Td.ansi[5],
  cyan: Td.ansi[6],
  white: Td.ansi[7],
  brightBlack: const Color(0xFF3F6B4C),
  brightRed: Td.ansi[9],
  brightGreen: Td.ansi[10],
  brightYellow: Td.ansi[11],
  brightBlue: Td.ansi[12],
  brightMagenta: Td.ansi[13],
  brightCyan: Td.ansi[14],
  brightWhite: Td.ansi[15],
  searchHitBackground: Td.pending,
  searchHitBackgroundCurrent: Td.accent,
  searchHitForeground: Td.ground,
);

/// The terminal face of a pane. A pane a window on the desk is showing is not
/// taken without asking, because taking it freezes the desk's copy.
class TermFace extends ConsumerStatefulWidget {
  const TermFace({required this.session, required this.pane, required this.info, super.key});
  final String session;
  final int pane;
  final PaneView? info;

  @override
  ConsumerState<TermFace> createState() => _TermFaceState();
}

class _TermFaceState extends ConsumerState<TermFace> {
  bool _take = false;

  /// Once the terminal is on screen it stays there. The phone's own attach is
  /// what makes the host report the pane as attached a moment later, and that
  /// reading must not swap the live terminal for the take-card. An ending is
  /// reported by the stream itself, inside the view.
  bool _live = false;

  @override
  Widget build(BuildContext context) {
    final info = widget.info;
    if (!_live) {
      if (info == null) {
        return Center(child: Text('The host no longer lists this pane.', style: TdType.m(12, color: Td.inkAt(0.6))));
      }
      if (info.ended) {
        return Center(child: Text('The process in this pane has exited.', style: TdType.m(12, color: Td.inkAt(0.6))));
      }
      if (info.attached && !_take) {
        return _TakeCard(onTake: () => setState(() => _take = true));
      }
      _live = true;
    }
    return TermView(session: widget.session, pane: widget.pane, take: _take);
  }
}

class _TakeCard extends StatelessWidget {
  const _TakeCard({required this.onTake});
  final VoidCallback onTake;

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.fromLTRB(16, 30, 16, 30),
    children: [
      Hud(
        tint: Td.pending,
        notch: Td.pending,
        padding: const EdgeInsets.fromLTRB(14, 16, 16, 18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('ANOTHER SCREEN HAS THIS PANE', style: TdType.m(13, color: Td.pending, weight: FontWeight.w700, spacing: 1.6)),
            const SizedBox(height: 12),
            const Prose(
              'Something is showing this terminal — almost always a window on the desk. Terminal '
              'Delight gives a terminal to whoever opened it last, so taking it moves it to the '
              'phone: a desk window keeps its last frame and stops until it is reopened.',
              size: 15,
            ),
            const SizedBox(height: 10),
            Prose(
              'Nothing stops or restarts. The agent keeps running on the laptop either way.',
              size: 14,
              color: Td.inkAt(0.7),
            ),
            const SizedBox(height: 18),
            Slab(label: 'Take it to the phone', icon: '⇲', tint: Td.pending, onPressed: onTake),
          ],
        ),
      ),
    ],
  );
}

enum _Link { connecting, live, closed }

class TermView extends ConsumerStatefulWidget {
  const TermView({required this.session, required this.pane, this.take = false, super.key});
  final String session;
  final int pane;
  final bool take;

  @override
  ConsumerState<TermView> createState() => _TermViewState();
}

class _TermViewState extends ConsumerState<TermView> {
  final terminal = Terminal(maxLines: 6000);
  final controller = TerminalController();
  final _focus = FocusNode();
  WebSocket? _ws;
  StreamSubscription<dynamic>? _sub;
  late final ByteConversionSink _decode;
  final _sgr = SgrFilter();
  _Link _link = _Link.connecting;
  String? _closedWhy;
  (int, int)? _size;
  Timer? _resize;
  bool _started = false;
  bool _ctrl = false;
  bool _composing = false;

  /// Re-attaching after a dropped connection — the cable pulled, the phone
  /// changing networks. Only a drop the gateway gave no reason for is retried:
  /// a pane that ended, or that another screen took, stays closed. And it asks
  /// without taking, so it can never pull a pane back off the desk.
  int _autoTries = 0;
  Timer? _auto;
  static const _autoDelays = [1, 2, 3, 5, 8, 12];
  final _draft = TextEditingController();
  late double _font = ref.read(initialFontProvider) ?? 11.5;

  @override
  void initState() {
    super.initState();
    // Output arrives in arbitrary chunks, and a chunk can end halfway through
    // a multi-byte character. A chunked decoder carries the half over.
    _decode = const Utf8Decoder(allowMalformed: true).startChunkedConversion(
      _StringSink((s) => terminal.write(_sgr.feed(s))),
    );
    terminal
      ..onOutput = _send
      ..onResize = (w, h, pw, ph) {
        _size = (w, h);
        if (!_started) {
          _started = true;
          unawaited(_connect());
        } else {
          _resize?.cancel();
          _resize = Timer(const Duration(milliseconds: 140), _sendSize);
        }
      };
  }

  void _send(String data) {
    var out = data;
    if (_ctrl && data.length == 1) {
      // The sticky Ctrl on the key row applies to the next key typed.
      final c = data.toLowerCase().codeUnitAt(0);
      if (c >= 0x61 && c <= 0x7a) out = String.fromCharCode(c - 0x60);
      setState(() => _ctrl = false);
    }
    _ws?.add(utf8.encode(out));
  }

  void _sendSize() {
    final s = _size;
    if (s == null) return;
    _ws?.add(jsonEncode({'resize': {'cols': s.$1, 'rows': s.$2}}));
  }

  Future<void> _connect({bool? take}) async {
    final api = ref.read(apiProvider);
    final s = _size;
    if (s == null) return;
    if (api == null) {
      // The route is changing under us; the next try will find the new one.
      if (_autoTries > 0) _dropped();
      return;
    }
    setState(() {
      _link = _Link.connecting;
      _closedWhy = null;
    });
    try {
      final ws = await api.socket(
        '/v0/sessions/${widget.session}/panes/${widget.pane}/term',
        {
          'cols': '${s.$1}',
          'rows': '${s.$2}',
          'cell_width': '${(_font * 0.6 * 3).round()}',
          'cell_height': '${(_font * 1.2 * 3).round()}',
          if (take ?? widget.take) 'take': 'true',
        },
      );
      ws.pingInterval = const Duration(seconds: 15);
      _ws = ws;
      _sub = ws.listen(
        (dynamic data) {
          if (data is List<int>) {
            _decode.add(data);
          } else if (data is String) {
            final m = jMap(jsonDecode(data));
            if (m == null) return;
            if (m.containsKey('attached')) {
              _autoTries = 0;
              setState(() => _link = _Link.live);
              _sendSize();
            }
            if (jStr(m['closed']) case final why?) {
              setState(() {
                _link = _Link.closed;
                _closedWhy = why;
              });
            }
          }
        },
        onDone: () {
          if (!mounted) return;
          setState(() {
            _link = _Link.closed;
            _closedWhy ??= 'lost';
          });
          if (_closedWhy == 'lost') _dropped();
        },
        onError: (Object _) {},
        cancelOnError: true,
      );
    } on WebSocketException catch (e) {
      if (!mounted) return;
      setState(() {
        _link = _Link.closed;
        _closedWhy = e.message.contains('409') ? 'attached' : e.message;
      });
      if (_autoTries > 0) _dropped();
    } on Object catch (e) {
      if (!mounted) return;
      setState(() {
        _link = _Link.closed;
        _closedWhy = '$e';
      });
      if (_autoTries > 0) _dropped();
    }
  }

  /// The connection went without a reason. Try again on whatever route the
  /// link has found by then, a few times, spaced out; after that the card's
  /// own button is the way back.
  void _dropped() {
    if (!mounted || _autoTries >= _autoDelays.length) return;
    final wait = _autoDelays[_autoTries++];
    _auto?.cancel();
    _auto = Timer(Duration(seconds: wait), () => unawaited(_reconnect(take: false)));
  }

  Future<void> _reconnect({bool? take}) async {
    await _sub?.cancel();
    await _ws?.close();
    _ws = null;
    if (!mounted) return;
    await _connect(take: take);
  }

  @override
  void dispose() {
    _resize?.cancel();
    _auto?.cancel();
    unawaited(_sub?.cancel());
    // Closing the stream detaches. The terminal keeps running on the laptop.
    unawaited(_ws?.close());
    _decode.close();
    _focus.dispose();
    controller.dispose();
    _draft.dispose();
    super.dispose();
  }

  void _key(TerminalKey key, {bool shift = false, bool ctrl = false}) {
    HapticFeedback.selectionClick();
    terminal.keyInput(key, shift: shift, ctrl: ctrl);
  }

  void _raw(String bytes) {
    HapticFeedback.selectionClick();
    _ws?.add(utf8.encode(bytes));
  }

  /// One write, the way the desk's bench delivers a message (TDAC §6): a
  /// bracketed paste and a return, so nothing inside the text submits early.
  void _say() {
    final text = _draft.text.replaceAll('\r\n', '\n');
    if (text.trim().isEmpty) return;
    HapticFeedback.mediumImpact();
    final body = terminal.bracketedPasteMode
        ? '\x1b[200~$text\x1b[201~\r'
        : '${text.replaceAll('\n', ' ')}\r';
    _ws?.add(utf8.encode(body));
    _draft.clear();
  }

  Future<void> _zoom(double by) async {
    setState(() => _font = (_font + by).clamp(7.0, 20.0));
    await ref.read(storeProvider).writeFont(_font);
  }

  @override
  Widget build(BuildContext context) {
    return Column(
      children: [
        Expanded(
          child: Stack(
            children: [
              Positioned.fill(
                child: Container(
                  margin: const EdgeInsets.fromLTRB(6, 4, 6, 0),
                  decoration: BoxDecoration(
                    color: Colors.black.withValues(alpha: 0.35),
                    border: Border.all(color: Td.accent.withValues(alpha: 0.18)),
                    borderRadius: BorderRadius.circular(4),
                  ),
                  child: TerminalView(
                    terminal,
                    controller: controller,
                    focusNode: _focus,
                    theme: tdTerminalTheme,
                    textStyle: TerminalStyle(
                      fontSize: _font,
                      fontFamily: TdType.mono,
                      // The Nerd Font has the powerline set but not every
                      // symbol an agent TUI draws (⏵, ⎿); Android carries those.
                      fontFamilyFallback: const [
                        'TDSymbols',
                        'Noto Sans Symbols',
                        'Noto Sans Mono',
                        'Noto Color Emoji',
                        'monospace',
                        'sans-serif',
                      ],
                    ),
                    padding: const EdgeInsets.all(4),
                    keyboardType: TextInputType.visiblePassword,
                    alwaysShowCursor: true,
                  ),
                ),
              ),
              if (_link != _Link.live) Positioned.fill(child: _status()),
            ],
          ),
        ),
        _KeyRow(
          ctrl: _ctrl,
          composing: _composing,
          onKey: _key,
          onRaw: _raw,
          onCtrl: () => setState(() => _ctrl = !_ctrl),
          onCompose: () => setState(() => _composing = !_composing),
          onZoom: _zoom,
          onKeyboard: () {
            _focus.requestFocus();
            unawaited(SystemChannels.textInput.invokeMethod<void>('TextInput.show'));
          },
          onPaste: () async {
            final clip = await Clipboard.getData(Clipboard.kTextPlain);
            if (clip?.text != null) terminal.paste(clip!.text!);
          },
        ),
        if (_composing) _compose(),
      ],
    );
  }

  Widget _status() {
    // A retry is already scheduled: say so, rather than showing the socket
    // error of an attempt that is about to be made again.
    final retrying = _auto?.isActive ?? false;
    final (title, body, tint) = switch ((_link, _closedWhy)) {
      (_Link.closed, _) when retrying => ('RECONNECTING', 'The connection dropped. Finding the laptop again…', Td.pending),
      (_Link.connecting, _) when _autoTries > 0 => ('RECONNECTING', 'The connection dropped. Finding the laptop again…', Td.pending),
      (_Link.connecting, _) => ('ATTACHING', 'Opening the terminal on the laptop…', Td.pending),
      (_, 'ended') => ('ENDED', 'The process in this pane has exited.', Td.inkAt(0.6)),
      (_, 'taken') => ('TAKEN BACK', 'Another screen opened this terminal. It is still running there.', Td.pending),
      (_, 'attached') => ('ON THE DESK', 'A window is showing this pane.', Td.pending),
      (_, 'host-gone') => ('HOST GONE', 'The session host on the laptop stopped.', Td.decision),
      (_, final why) => ('DISCONNECTED', why ?? 'The connection dropped.', Td.decision),
    };
    return ColoredBox(
      color: Td.ground.withValues(alpha: 0.72),
      child: Center(
        child: Padding(
          padding: const EdgeInsets.all(24),
          child: Hud(
            tint: tint,
            padding: const EdgeInsets.fromLTRB(14, 14, 16, 16),
            child: Column(
              mainAxisSize: MainAxisSize.min,
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(title, style: TdType.m(13, color: tint, weight: FontWeight.w700, spacing: 1.8)),
                const SizedBox(height: 8),
                Text(body, style: TdType.p(14.5)),
                if (_link == _Link.closed && _closedWhy != 'ended' && !retrying) ...[
                  const SizedBox(height: 14),
                  Slab(
                    label: 'Attach again',
                    icon: '↻',
                    dense: true,
                    onPressed: () {
                      _autoTries = 0;
                      unawaited(_reconnect());
                    },
                  ),
                ],
              ],
            ),
          ),
        ),
      ),
    );
  }

  Widget _compose() => Container(
    padding: EdgeInsets.fromLTRB(10, 8, 10, 8 + MediaQuery.of(context).padding.bottom * 0),
    decoration: BoxDecoration(
      color: Td.surface,
      border: Border(top: BorderSide(color: Td.human.withValues(alpha: 0.5))),
    ),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.end,
      children: [
        Expanded(
          child: TextField(
            controller: _draft,
            autofocus: true,
            minLines: 1,
            maxLines: 6,
            textCapitalization: TextCapitalization.sentences,
            style: TdType.p(15),
            cursorColor: Td.human,
            decoration: InputDecoration(
              isDense: true,
              hintText: 'Say it to the agent — the mic on your keyboard works here',
              hintStyle: TdType.p(13.5, color: Td.inkAt(0.4)),
              border: InputBorder.none,
            ),
          ),
        ),
        const SizedBox(width: 8),
        Slab(label: 'Send', icon: '⏎', tint: Td.human, dense: true, onPressed: _say),
      ],
    ),
  );
}

class _StringSink implements Sink<String> {
  _StringSink(this.onData);
  final void Function(String) onData;
  @override
  void add(String data) => onData(data);
  @override
  void close() {}
}

/// Every key a phone keyboard lacks, in reach of a thumb.
class _KeyRow extends StatelessWidget {
  const _KeyRow({
    required this.ctrl,
    required this.composing,
    required this.onKey,
    required this.onRaw,
    required this.onCtrl,
    required this.onCompose,
    required this.onZoom,
    required this.onKeyboard,
    required this.onPaste,
  });
  final bool ctrl;
  final bool composing;
  final void Function(TerminalKey key, {bool shift, bool ctrl}) onKey;
  final void Function(String) onRaw;
  final VoidCallback onCtrl;
  final VoidCallback onCompose;
  final Future<void> Function(double) onZoom;
  final VoidCallback onKeyboard;
  final VoidCallback onPaste;

  @override
  Widget build(BuildContext context) {
    Widget k(String label, VoidCallback onTap, {Color? tint, bool on = false, double width = 44}) => Padding(
      padding: const EdgeInsets.only(right: 5),
      child: Material(
        color: on ? (tint ?? Td.accent).withValues(alpha: 0.25) : Colors.black.withValues(alpha: 0.35),
        shape: RoundedRectangleBorder(
          side: BorderSide(color: (tint ?? Td.accent).withValues(alpha: on ? 0.9 : 0.28)),
          borderRadius: BorderRadius.circular(3),
        ),
        child: InkWell(
          onTap: onTap,
          borderRadius: BorderRadius.circular(3),
          child: SizedBox(
            width: width,
            height: 38,
            child: Center(
              child: Text(
                label,
                style: TdType.m(12, color: tint ?? Td.ink, weight: FontWeight.w700),
              ),
            ),
          ),
        ),
      ),
    );
    return Container(
      height: 50,
      padding: const EdgeInsets.fromLTRB(6, 6, 0, 6),
      child: ListView(
        scrollDirection: Axis.horizontal,
        children: [
          k('✎', onCompose, tint: Td.human, on: composing),
          k('⌨', onKeyboard),
          k('ESC', () => onKey(TerminalKey.escape), width: 48),
          k('TAB', () => onKey(TerminalKey.tab), width: 48),
          k('⇧TAB', () => onKey(TerminalKey.tab, shift: true), width: 56),
          k('CTRL', onCtrl, on: ctrl, tint: Td.pending, width: 54),
          k('^C', () => onRaw('\x03'), tint: Td.decision),
          k('↑', () => onKey(TerminalKey.arrowUp)),
          k('↓', () => onKey(TerminalKey.arrowDown)),
          k('←', () => onKey(TerminalKey.arrowLeft)),
          k('→', () => onKey(TerminalKey.arrowRight)),
          k('⏎', () => onKey(TerminalKey.enter), tint: Td.accent),
          k('/', () => onRaw('/')),
          k('|', () => onRaw('|')),
          k('~', () => onRaw('~')),
          k('^D', () => onRaw('\x04')),
          k('^L', () => onRaw('\x0c')),
          k('^R', () => onRaw('\x12')),
          k('PASTE', onPaste, width: 60),
          k('A−', () => unawaited(onZoom(-0.5))),
          k('A+', () => unawaited(onZoom(0.5))),
          const SizedBox(width: 6),
        ],
      ),
    );
  }
}
