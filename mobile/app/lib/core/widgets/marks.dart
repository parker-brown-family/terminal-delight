import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/link.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/widgets/ground.dart';

/// A tube switching on: a line of light across the middle, the picture
/// opening out of it, one flicker, then still. Plays once per mount.
class Ignition extends StatefulWidget {
  const Ignition({required this.child, this.delayMs = 0, super.key});
  final Widget child;
  final int delayMs;

  @override
  State<Ignition> createState() => _IgnitionState();
}

class _IgnitionState extends State<Ignition> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 1100),
  );

  @override
  void initState() {
    super.initState();
    Future<void>.delayed(Duration(milliseconds: widget.delayMs), () {
      if (mounted) _c.forward();
    });
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    if (MediaQuery.of(context).disableAnimations) return widget.child;
    return AnimatedBuilder(
      animation: _c,
      builder: (context, child) {
        final t = _c.value;
        // 0–.28: the line grows out from the centre.
        // .28–.62: the picture opens vertically out of it, overbright.
        // .62–1: settles, with one dip in brightness on the way.
        final sx = Curves.easeOutCubic.transform((t / 0.28).clamp(0, 1));
        final open = Curves.easeOutBack.transform(((t - 0.28) / 0.34).clamp(0, 1));
        final sy = 0.02 + 0.98 * open;
        final flicker = t > 0.72 && t < 0.78 ? 0.55 : 1.0;
        final over = (1 - ((t - 0.28) / 0.6).clamp(0, 1)) * 0.9;
        return Opacity(
          opacity: (t < 0.02 ? 0 : 1) * flicker,
          child: Transform(
            alignment: Alignment.center,
            transform: Matrix4.diagonal3Values(sx, sy, 1),
            child: ColorFiltered(
              colorFilter: ColorFilter.mode(
                Colors.white.withValues(alpha: over),
                BlendMode.srcATop,
              ),
              child: child,
            ),
          ),
        );
      },
      child: widget.child,
    );
  }
}

/// TERMINAL DELIGHT, lit.
class Wordmark extends StatelessWidget {
  const Wordmark({this.size = 26, this.stacked = true, super.key});
  final double size;
  final bool stacked;

  @override
  Widget build(BuildContext context) {
    final style = TdType.m(
      size,
      color: Td.bright,
      weight: FontWeight.w700,
      spacing: size * 0.14,
      height: 1.05,
    ).copyWith(shadows: TdType.glow(Td.accent, strength: 1.2));
    final accent = style.copyWith(color: Td.accent);
    if (!stacked) {
      return Text.rich(
        TextSpan(
          children: [
            TextSpan(text: 'TERMINAL ', style: style),
            TextSpan(text: 'DELIGHT', style: accent),
          ],
        ),
      );
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      mainAxisSize: MainAxisSize.min,
      children: [Text('TERMINAL', style: style), Text('DELIGHT', style: accent)],
    );
  }
}

/// What a pane is doing, as a small lit chip. Needs-you breathes; working
/// runs a cursor; unknown is drawn as unknown, never as idle.
class StateChip extends StatelessWidget {
  const StateChip(this.state, {this.age, super.key});
  final PaneState state;
  final String? age;

  @override
  Widget build(BuildContext context) {
    final tint = state == PaneState.unknown || state == PaneState.ended
        ? Td.inkAt(0.45)
        : state.tint;
    Widget chip(double lit) => Container(
      padding: const EdgeInsets.symmetric(horizontal: 7, vertical: 3),
      decoration: BoxDecoration(
        color: tint.withValues(alpha: 0.08 + 0.14 * lit),
        border: Border.all(
          color: tint.withValues(alpha: 0.35 + 0.5 * lit),
          width: state == PaneState.unknown ? 0.8 : 1,
        ),
        borderRadius: BorderRadius.circular(2),
        boxShadow: lit > 0
            ? [BoxShadow(color: tint.withValues(alpha: 0.4 * lit), blurRadius: 12)]
            : const [],
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: [
          if (state == PaneState.working) const _Cursor() else _Dot(tint),
          const SizedBox(width: 6),
          Text(
            [state.word, ?age].join(' · '),
            style: TdType.m(9.5, color: tint, weight: FontWeight.w700, spacing: 1.4),
          ),
        ],
      ),
    );
    if (state == PaneState.needsYou) {
      return Breathe(builder: (context, v) => chip(0.35 + 0.65 * v));
    }
    return chip(state == PaneState.working ? 0.25 : 0);
  }
}

class _Dot extends StatelessWidget {
  const _Dot(this.color);
  final Color color;
  @override
  Widget build(BuildContext context) => Container(
    width: 6,
    height: 6,
    decoration: BoxDecoration(color: color, shape: BoxShape.circle),
  );
}

/// A block cursor, blinking at the terminal's own rate.
class _Cursor extends StatefulWidget {
  const _Cursor();
  @override
  State<_Cursor> createState() => _CursorState();
}

class _CursorState extends State<_Cursor> {
  bool _on = true;
  Timer? _t;

  @override
  void initState() {
    super.initState();
    _t = Timer.periodic(const Duration(milliseconds: 530), (_) {
      if (mounted) setState(() => _on = !_on);
    });
  }

  @override
  void dispose() {
    _t?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => Container(
    width: 6,
    height: 9,
    color: Td.pending.withValues(alpha: _on ? 1 : 0.15),
  );
}

class ModeGlyph extends StatelessWidget {
  const ModeGlyph(this.mode, {this.size = 15, super.key});
  final PaneMode mode;
  final double size;

  @override
  Widget build(BuildContext context) => SizedBox(
    width: size * 1.2,
    child: Text(
      mode.glyph,
      textAlign: TextAlign.center,
      style: TdType.m(size, color: mode.tint, weight: FontWeight.w700)
          .copyWith(shadows: TdType.glow(mode.tint, strength: 0.5)),
    ),
  );
}

/// Which route is live, how far away the laptop is, and whether the live feed
/// is open. Tapping it looks again for the best route.
class LinkPill extends ConsumerWidget {
  const LinkPill({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final link = ref.watch(linkProvider);
    final live = ref.watch(liveProvider).valueOrNull ?? ref.read(eventsProvider).isLive;
    final l = link.valueOrNull;
    final (text, tint) = switch (link) {
      AsyncData(value: final Link v) => ('${v.route} · ${v.rttMs}MS', Td.accent),
      AsyncError() => ('NO ROUTE', Td.decision),
      _ => ('SEEKING', Td.pending),
    };
    return GestureDetector(
      onTap: () => ref.read(linkProvider.notifier).rediscover(),
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 5),
        decoration: BoxDecoration(
          border: Border.all(color: tint.withValues(alpha: 0.5)),
          borderRadius: BorderRadius.circular(20),
          color: tint.withValues(alpha: 0.07),
        ),
        child: Row(
          mainAxisSize: MainAxisSize.min,
          children: [
            if (l != null && live)
              Breathe(
                period: 2200,
                builder: (context, v) => Container(
                  width: 7,
                  height: 7,
                  decoration: BoxDecoration(
                    color: Td.accent,
                    shape: BoxShape.circle,
                    boxShadow: [
                      BoxShadow(color: Td.accent.withValues(alpha: 0.4 + 0.5 * v), blurRadius: 8),
                    ],
                  ),
                ),
              )
            else
              _Dot(tint.withValues(alpha: 0.6)),
            const SizedBox(width: 7),
            Text(text, style: TdType.m(9.5, color: tint, weight: FontWeight.w700, spacing: 1.3)),
          ],
        ),
      ),
    );
  }
}
