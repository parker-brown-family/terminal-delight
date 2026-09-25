import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:terminal_delight/app/theme/td.dart';

/// Next Run's HUD panel, wearing Terminal Delight: a translucent slab with its
/// top-right corner cut off, a hairline in the card's tint, the bench's 3px
/// spine down the left edge — and the cut corner filled with the colour of the
/// project the pane sits in on the desk, so a card says where it lives before
/// a word is read.
class Hud extends StatelessWidget {
  const Hud({
    required this.child,
    this.tint = Td.accent,
    this.notch,
    this.spine,
    this.onTap,
    this.onLongPress,
    this.padding = const EdgeInsets.fromLTRB(14, 12, 14, 12),
    this.glow = 0.0,
    this.fill,
    super.key,
  });

  final Widget child;
  final Color tint;

  /// The cut corner's fill. Null leaves it open.
  final Color? notch;

  /// The left edge's colour. Null uses the tint.
  final Color? spine;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;
  final EdgeInsets padding;

  /// 0 for a card at rest, 1 for one that wants a person.
  final double glow;
  final Color? fill;

  static const cut = 14.0;

  @override
  Widget build(BuildContext context) {
    final body = CustomPaint(
      painter: _HudPainter(
        tint: tint,
        notch: notch,
        spine: spine ?? tint,
        glow: glow,
        fill: fill ?? Td.surface.withValues(alpha: 0.86),
      ),
      child: Padding(padding: padding.copyWith(left: padding.left + 3), child: child),
    );
    if (onTap == null && onLongPress == null) return body;
    return _Press(onTap: onTap, onLongPress: onLongPress, child: body);
  }
}

class _HudPainter extends CustomPainter {
  _HudPainter({
    required this.tint,
    required this.notch,
    required this.spine,
    required this.glow,
    required this.fill,
  });
  final Color tint;
  final Color? notch;
  final Color spine;
  final double glow;
  final Color fill;

  Path _shape(Size s) => Path()
    ..moveTo(0, 0)
    ..lineTo(s.width - Hud.cut, 0)
    ..lineTo(s.width, Hud.cut)
    ..lineTo(s.width, s.height)
    ..lineTo(0, s.height)
    ..close();

  @override
  void paint(Canvas canvas, Size size) {
    final shape = _shape(size);
    if (glow > 0) {
      canvas.drawPath(
        shape,
        Paint()
          ..color = tint.withValues(alpha: 0.32 * glow)
          ..maskFilter = MaskFilter.blur(BlurStyle.outer, 6 + 10 * glow),
      );
    }
    canvas
      ..drawPath(shape, Paint()..color = fill)
      ..drawPath(
        shape,
        Paint()
          ..style = PaintingStyle.stroke
          ..strokeWidth = 1
          ..color = tint.withValues(alpha: 0.28 + 0.4 * glow),
      )
      ..drawRect(Rect.fromLTWH(0, 0, 3, size.height), Paint()..color = spine);
    final n = notch;
    if (n != null) {
      canvas.drawPath(
        Path()
          ..moveTo(size.width - Hud.cut, 0)
          ..lineTo(size.width, 0)
          ..lineTo(size.width, Hud.cut)
          ..close(),
        Paint()..color = n,
      );
    }
  }

  @override
  bool shouldRepaint(_HudPainter old) =>
      old.tint != tint ||
      old.notch != notch ||
      old.spine != spine ||
      old.glow != glow ||
      old.fill != fill;
}

/// Presses in a little and ticks under the thumb.
class _Press extends StatefulWidget {
  const _Press({required this.child, this.onTap, this.onLongPress});
  final Widget child;
  final VoidCallback? onTap;
  final VoidCallback? onLongPress;

  @override
  State<_Press> createState() => _PressState();
}

class _PressState extends State<_Press> {
  bool _down = false;

  @override
  Widget build(BuildContext context) => GestureDetector(
    behavior: HitTestBehavior.opaque,
    onTapDown: (_) => setState(() => _down = true),
    onTapCancel: () => setState(() => _down = false),
    onTapUp: (_) => setState(() => _down = false),
    onTap: widget.onTap == null
        ? null
        : () {
            HapticFeedback.selectionClick();
            widget.onTap!();
          },
    onLongPress: widget.onLongPress == null
        ? null
        : () {
            HapticFeedback.mediumImpact();
            widget.onLongPress!();
          },
    child: AnimatedScale(
      scale: _down ? 0.982 : 1,
      duration: Duration(milliseconds: _down ? 70 : 160),
      curve: Curves.easeOut,
      child: widget.child,
    ),
  );
}

/// A lit slab of a button — the primary action on a screen, and there is only
/// ever one.
class Slab extends StatefulWidget {
  const Slab({
    required this.label,
    required this.onPressed,
    this.tint = Td.accent,
    this.icon,
    this.busy = false,
    this.dense = false,
    super.key,
  });
  final String label;
  final VoidCallback? onPressed;
  final Color tint;
  final String? icon;
  final bool busy;
  final bool dense;

  @override
  State<Slab> createState() => _SlabState();
}

class _SlabState extends State<Slab> {
  bool _down = false;

  @override
  Widget build(BuildContext context) {
    final enabled = widget.onPressed != null && !widget.busy;
    final t = widget.tint;
    return GestureDetector(
      onTapDown: enabled ? (_) => setState(() => _down = true) : null,
      onTapCancel: () => setState(() => _down = false),
      onTapUp: (_) => setState(() => _down = false),
      onTap: enabled
          ? () {
              HapticFeedback.mediumImpact();
              widget.onPressed!();
            }
          : null,
      child: AnimatedScale(
        scale: _down ? 0.975 : 1,
        duration: const Duration(milliseconds: 110),
        child: AnimatedContainer(
          duration: const Duration(milliseconds: 180),
          padding: EdgeInsets.symmetric(
            horizontal: widget.dense ? 14 : 20,
            vertical: widget.dense ? 9 : 14,
          ),
          decoration: BoxDecoration(
            color: t.withValues(alpha: enabled ? (_down ? 0.34 : 0.2) : 0.06),
            border: Border.all(color: t.withValues(alpha: enabled ? 0.9 : 0.3)),
            borderRadius: BorderRadius.circular(3),
            boxShadow: enabled
                ? [BoxShadow(color: t.withValues(alpha: 0.35), blurRadius: 18)]
                : const [],
          ),
          child: Row(
            mainAxisSize: MainAxisSize.min,
            mainAxisAlignment: MainAxisAlignment.center,
            children: [
              if (widget.busy)
                SizedBox(
                  width: 14,
                  height: 14,
                  child: CircularProgressIndicator(strokeWidth: 1.6, color: t),
                )
              else if (widget.icon != null)
                Text(widget.icon!, style: TdType.m(14, color: t)),
              if (widget.busy || widget.icon != null) const SizedBox(width: 10),
              Text(
                widget.label.toUpperCase(),
                style: TdType.m(
                  widget.dense ? 11.5 : 13,
                  color: enabled ? Td.bright : Td.inkAt(0.4),
                  weight: FontWeight.w700,
                  spacing: 2.2,
                ).copyWith(shadows: enabled ? TdType.glow(t, strength: 0.6) : null),
              ),
            ],
          ),
        ),
      ),
    );
  }
}

/// A quiet outlined button, for everything that is not the one action.
class Ghost extends StatelessWidget {
  const Ghost({required this.label, required this.onPressed, this.tint = Td.ink, super.key});
  final String label;
  final VoidCallback? onPressed;
  final Color tint;

  @override
  Widget build(BuildContext context) => InkWell(
    onTap: onPressed == null
        ? null
        : () {
            HapticFeedback.selectionClick();
            onPressed!();
          },
    borderRadius: BorderRadius.circular(3),
    child: Container(
      padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 10),
      decoration: BoxDecoration(
        border: Border.all(color: tint.withValues(alpha: 0.35)),
        borderRadius: BorderRadius.circular(3),
      ),
      child: Text(
        label.toUpperCase(),
        style: TdType.m(11.5, color: tint, spacing: 1.8, weight: FontWeight.w700),
      ),
    ),
  );
}

/// A staggered fade-and-rise, once per mount — Next Run's rack focus.
class RiseIn extends StatefulWidget {
  const RiseIn({required this.child, this.index = 0, super.key});
  final Widget child;
  final int index;

  @override
  State<RiseIn> createState() => _RiseInState();
}

class _RiseInState extends State<RiseIn> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: const Duration(milliseconds: 460),
  );
  late final Animation<double> _t = CurvedAnimation(
    parent: _c,
    curve: const Cubic(0.2, 0.8, 0.2, 1),
  );

  @override
  void initState() {
    super.initState();
    Future<void>.delayed(
      Duration(milliseconds: 55 * widget.index.clamp(0, 8)),
      () {
        if (mounted) _c.forward();
      },
    );
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
      animation: _t,
      builder: (context, child) => Opacity(
        opacity: _t.value,
        child: Transform.translate(
          offset: Offset(0, 10 * (1 - _t.value)),
          child: child,
        ),
      ),
      child: widget.child,
    );
  }
}

/// A section's name, with a rule running out to the edge.
class Kicker extends StatelessWidget {
  const Kicker(this.text, {this.color = Td.ink, this.trailing, super.key});
  final String text;
  final Color color;
  final Widget? trailing;

  @override
  Widget build(BuildContext context) => Row(
    children: [
      Text(text.toUpperCase(), style: TdType.kicker(color: color)),
      const SizedBox(width: 10),
      Expanded(child: Container(height: 1, color: color.withValues(alpha: 0.16))),
      if (trailing != null) ...[const SizedBox(width: 10), trailing!],
    ],
  );
}
