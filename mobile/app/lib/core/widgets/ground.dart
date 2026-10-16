import 'dart:math' as math;

import 'package:flutter/material.dart';
import 'package:terminal_delight/app/theme/td.dart';

/// The glass everything sits behind: a phosphor ground with a faint survey
/// grid, and over the content the tube's own marks — scanlines, a vignette,
/// and a band of brightness rolling down the screen the way a real CRT's
/// refresh does when a camera catches it.
///
/// Not the warp. The desk bends its glass; the phone keeps its content flat
/// and only borrows the light, because text a thumb has to hit must stay where
/// it is drawn.
class Ground extends StatefulWidget {
  const Ground({required this.child, this.tint = Td.accent, super.key});
  final Widget child;
  final Color tint;

  @override
  State<Ground> createState() => _GroundState();
}

class _GroundState extends State<Ground> with SingleTickerProviderStateMixin {
  late final AnimationController _roll = AnimationController(
    vsync: this,
    duration: const Duration(seconds: 9),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.of(context).disableAnimations) {
      _roll.stop();
    } else if (!_roll.isAnimating) {
      _roll.repeat();
    }
  }

  @override
  void dispose() {
    _roll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return ColoredBox(
      color: Td.ground,
      child: Stack(
        fit: StackFit.expand,
        children: [
          RepaintBoundary(
            child: CustomPaint(painter: _GridPainter(widget.tint)),
          ),
          widget.child,
          IgnorePointer(
            child: RepaintBoundary(
              child: CustomPaint(painter: _GlassPainter()),
            ),
          ),
          IgnorePointer(
            child: RepaintBoundary(
              child: AnimatedBuilder(
                animation: _roll,
                builder: (context, _) => CustomPaint(
                  painter: _BandPainter(_roll.value, widget.tint),
                ),
              ),
            ),
          ),
        ],
      ),
    );
  }
}

class _GridPainter extends CustomPainter {
  _GridPainter(this.tint);
  final Color tint;

  @override
  void paint(Canvas canvas, Size size) {
    // Light rising from the top edge, as if the tube's gun sat just above it.
    final rise = Paint()
      ..shader = RadialGradient(
        center: const Alignment(0, -1.25),
        radius: 1.25,
        colors: [tint.withValues(alpha: 0.13), tint.withValues(alpha: 0)],
      ).createShader(Offset.zero & size);
    canvas.drawRect(Offset.zero & size, rise);

    final line = Paint()
      ..color = Td.faint.withValues(alpha: 0.42)
      ..strokeWidth = 0.6;
    const step = 28.0;
    for (var x = step / 2; x < size.width; x += step) {
      canvas.drawLine(Offset(x, 0), Offset(x, size.height), line);
    }
    for (var y = step / 2; y < size.height; y += step) {
      canvas.drawLine(Offset(0, y), Offset(size.width, y), line);
    }
  }

  @override
  bool shouldRepaint(_GridPainter old) => old.tint != tint;
}

class _GlassPainter extends CustomPainter {
  @override
  void paint(Canvas canvas, Size size) {
    final scan = Paint()..color = Colors.black.withValues(alpha: 0.13);
    for (var y = 0.0; y < size.height; y += 3) {
      canvas.drawRect(Rect.fromLTWH(0, y, size.width, 1), scan);
    }
    final vignette = Paint()
      ..shader = RadialGradient(
        radius: 1.05,
        colors: [
          Colors.transparent,
          Colors.black.withValues(alpha: 0.1),
          Colors.black.withValues(alpha: 0.55),
        ],
        stops: const [0.55, 0.8, 1],
      ).createShader(Offset.zero & size);
    canvas.drawRect(Offset.zero & size, vignette);
  }

  @override
  bool shouldRepaint(_GlassPainter old) => false;
}

class _BandPainter extends CustomPainter {
  _BandPainter(this.t, this.tint);
  final double t;
  final Color tint;

  @override
  void paint(Canvas canvas, Size size) {
    const band = 180.0;
    final y = -band + (size.height + band * 2) * Curves.easeInOut.transform(t);
    final rect = Rect.fromLTWH(0, y - band / 2, size.width, band);
    final p = Paint()
      ..shader = LinearGradient(
        begin: Alignment.topCenter,
        end: Alignment.bottomCenter,
        colors: [
          tint.withValues(alpha: 0),
          tint.withValues(alpha: 0.045),
          tint.withValues(alpha: 0),
        ],
      ).createShader(rect);
    canvas.drawRect(rect, p);
  }

  @override
  bool shouldRepaint(_BandPainter old) => old.t != t;
}

/// A breathing glow, for things that are waiting on a person.
class Breathe extends StatefulWidget {
  const Breathe({required this.builder, this.period = 1600, super.key});
  final Widget Function(BuildContext, double) builder;
  final int period;

  @override
  State<Breathe> createState() => _BreatheState();
}

class _BreatheState extends State<Breathe> with SingleTickerProviderStateMixin {
  late final AnimationController _c = AnimationController(
    vsync: this,
    duration: Duration(milliseconds: widget.period),
  );

  @override
  void didChangeDependencies() {
    super.didChangeDependencies();
    if (MediaQuery.of(context).disableAnimations) {
      _c.value = 1;
    } else if (!_c.isAnimating) {
      _c.repeat(reverse: true);
    }
  }

  @override
  void dispose() {
    _c.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AnimatedBuilder(
    animation: _c,
    builder: (context, _) => widget.builder(
      context,
      0.5 - 0.5 * math.cos(_c.value * math.pi),
    ),
  );
}
