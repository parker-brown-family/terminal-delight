import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/link.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/core/widgets/ground.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/core/widgets/marks.dart';

const pairCommand = 'td-mobile-gateway pair';

/// The first thing an unpaired phone shows: the mark switching on, and the
/// one thing to do on the laptop.
class PairPage extends StatelessWidget {
  const PairPage({super.key});

  @override
  Widget build(BuildContext context) {
    return Ground(
      child: Scaffold(
        body: SafeArea(
          child: ListView(
            padding: const EdgeInsets.fromLTRB(26, 36, 26, 30),
            children: [
              Text('THE DESK, IN YOUR POCKET', style: TdType.kicker(color: Td.accent)),
              const SizedBox(height: 18),
              const Ignition(delayMs: 250, child: Wordmark(size: 40)),
              const SizedBox(height: 22),
              RiseIn(
                index: 8,
                child: Text(
                  'Every agent on your desk, what each one last said, and the '
                  'terminals behind them — on the glass in your hand.',
                  style: TdType.p(17, color: Td.bright.withValues(alpha: 0.86)),
                ),
              ),
              const SizedBox(height: 34),
              RiseIn(
                index: 10,
                child: Hud(
                  notch: Td.cabinet,
                  padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text('PAIR OVER USB', style: TdType.kicker(color: Td.accent)),
                      const SizedBox(height: 14),
                      const _Step(n: 1, text: 'Plug this phone into the laptop.'),
                      const _Step(n: 2, text: 'On the laptop, run:'),
                      const _Command(pairCommand),
                      const _Step(n: 3, text: 'Accept the sheet that appears here.'),
                    ],
                  ),
                ),
              ),
              const SizedBox(height: 26),
              const RiseIn(index: 12, child: _Waiting()),
              const SizedBox(height: 40),
              RiseIn(
                index: 13,
                child: Text(
                  'The token travels on the cable and nowhere else. '
                  'Unplugged, the phone finds the laptop over Tailscale.',
                  style: TdType.p(13, color: Td.inkAt(0.55)),
                ),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

class _Step extends StatelessWidget {
  const _Step({required this.n, required this.text});
  final int n;
  final String text;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 10),
    child: Row(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Container(
          width: 22,
          height: 22,
          alignment: Alignment.center,
          decoration: BoxDecoration(
            border: Border.all(color: Td.accent.withValues(alpha: 0.6)),
            borderRadius: BorderRadius.circular(2),
          ),
          child: Text('$n', style: TdType.m(11.5, color: Td.accent, weight: FontWeight.w700)),
        ),
        const SizedBox(width: 12),
        Expanded(
          child: Padding(
            padding: const EdgeInsets.only(top: 1),
            child: Text(text, style: TdType.p(15.5)),
          ),
        ),
      ],
    ),
  );
}

class _Command extends StatelessWidget {
  const _Command(this.command);
  final String command;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.fromLTRB(34, 0, 0, 12),
    child: GestureDetector(
      onTap: () {
        unawaited(Clipboard.setData(const ClipboardData(text: pairCommand)));
        HapticFeedback.selectionClick();
        ScaffoldMessenger.of(context).showSnackBar(
          const SnackBar(content: Text('Copied')),
        );
      },
      child: Container(
        width: double.infinity,
        padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 11),
        decoration: BoxDecoration(
          color: Colors.black.withValues(alpha: 0.4),
          border: Border.all(color: Td.faint),
          borderRadius: BorderRadius.circular(3),
        ),
        child: Text.rich(
          TextSpan(
            children: [
              TextSpan(text: r'$ ', style: TdType.m(13.5, color: Td.inkAt(0.5))),
              TextSpan(
                text: command,
                style: TdType.m(13.5, color: Td.bright, weight: FontWeight.w700),
              ),
            ],
          ),
        ),
      ),
    ),
  );
}

class _Waiting extends StatefulWidget {
  const _Waiting();
  @override
  State<_Waiting> createState() => _WaitingState();
}

class _WaitingState extends State<_Waiting> {
  bool _on = true;
  Timer? _t;

  @override
  void initState() {
    super.initState();
    _t = Timer.periodic(
      const Duration(milliseconds: 530),
      (_) => setState(() => _on = !_on),
    );
  }

  @override
  void dispose() {
    _t?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Text.rich(
    TextSpan(
      children: [
        TextSpan(text: '> waiting for the desk ', style: TdType.m(13, color: Td.inkAt(0.7))),
        TextSpan(
          text: '█',
          style: TdType.m(13, color: Td.cursor.withValues(alpha: _on ? 1 : 0)),
        ),
      ],
    ),
  );
}

/// A pairing link arrived. It is a proposal — any app on the phone could have
/// sent one — so the person reads where it points and says yes.
Future<void> showPairingSheet(BuildContext context, Pairing p) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (context) => _PairingSheet(p),
  );
}

class _PairingSheet extends ConsumerStatefulWidget {
  const _PairingSheet(this.p);
  final Pairing p;

  @override
  ConsumerState<_PairingSheet> createState() => _PairingSheetState();
}

class _PairingSheetState extends ConsumerState<_PairingSheet> {
  late final Map<String, Future<(int, String?)?>> _probes = {
    for (final r in widget.p.routes) r.base: probe(r, widget.p.machine),
  };
  bool _busy = false;

  Future<void> _accept() async {
    setState(() => _busy = true);
    await ref.read(pairingProvider.notifier).accept(widget.p);
    ref.read(proposedPairingProvider.notifier).state = null;
    if (mounted) Navigator.of(context).pop();
  }

  @override
  Widget build(BuildContext context) {
    final p = widget.p;
    return Padding(
      padding: EdgeInsets.fromLTRB(12, 0, 12, 12 + MediaQuery.of(context).viewPadding.bottom),
      child: Hud(
        notch: Td.accent,
        glow: 0.6,
        fill: Td.ground,
        padding: const EdgeInsets.fromLTRB(18, 18, 18, 20),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('PAIR WITH', style: TdType.kicker(color: Td.accent)),
            const SizedBox(height: 8),
            Text(
              p.machine.toUpperCase(),
              style: TdType.m(34, color: Td.bright, weight: FontWeight.w700, spacing: 3)
                  .copyWith(shadows: TdType.glow(Td.accent)),
            ),
            const SizedBox(height: 4),
            Text('token ${p.fingerprint}', style: TdType.m(11.5, color: Td.inkAt(0.55))),
            const SizedBox(height: 18),
            for (final r in p.routes)
              FutureBuilder(
                future: _probes[r.base],
                builder: (context, snap) {
                  final res = snap.data;
                  final (mark, note, tint) = snap.connectionState != ConnectionState.done
                      ? ('…', 'checking', Td.pending)
                      : (res != null && res.$2 == null)
                      ? ('✓', '${res.$1} ms', Td.accent)
                      : ('·', res?.$2 ?? 'no answer', Td.inkAt(0.4));
                  return Padding(
                    padding: const EdgeInsets.only(bottom: 7),
                    child: Row(
                      children: [
                        SizedBox(width: 18, child: Text(mark, style: TdType.m(13, color: tint))),
                        SizedBox(
                          width: 96,
                          child: Text(
                            r.name,
                            style: TdType.m(11.5, weight: FontWeight.w700, spacing: 1.2),
                          ),
                        ),
                        Expanded(
                          child: Text(
                            Uri.parse(r.base).host,
                            style: TdType.m(11.5, color: Td.inkAt(0.6)),
                          ),
                        ),
                        Text(note, style: TdType.m(11, color: tint)),
                      ],
                    ),
                  );
                },
              ),
            const SizedBox(height: 18),
            Row(
              children: [
                Expanded(child: Slab(label: 'Pair', icon: '⏚', busy: _busy, onPressed: _accept)),
                const SizedBox(width: 10),
                Ghost(
                  label: 'Not now',
                  onPressed: () {
                    ref.read(proposedPairingProvider.notifier).state = null;
                    Navigator.of(context).pop();
                  },
                ),
              ],
            ),
          ],
        ),
      ),
    );
  }
}
