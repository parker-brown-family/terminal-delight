import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/features/wall/wall_page.dart';

/// Start something on the laptop from the phone. Nothing on the desk is
/// taken: the host makes a new terminal and the phone holds its stream.
Future<void> showNewSheet(BuildContext context, {required Wall wall}) {
  return showModalBottomSheet<void>(
    context: context,
    isScrollControlled: true,
    builder: (context) => _NewSheet(wall: wall),
  );
}

enum _Kind { claude, terminal }

class _NewSheet extends ConsumerStatefulWidget {
  const _NewSheet({required this.wall});
  final Wall wall;

  @override
  ConsumerState<_NewSheet> createState() => _NewSheetState();
}

class _NewSheetState extends ConsumerState<_NewSheet> {
  _Kind _kind = _Kind.claude;
  String? _model;
  late final List<String> _dirs = _recentDirs(widget.wall);
  late String _dir = _dirs.first;
  bool _busy = false;
  String? _error;

  /// Where the desk's panes are, most-used first — where a person is most
  /// likely to want the next one.
  static List<String> _recentDirs(Wall w) {
    final count = <String, int>{};
    for (final p in w.panes) {
      final c = p.cwd;
      if (c != null && !p.ended) count[c] = (count[c] ?? 0) + 1;
    }
    final dirs = count.keys.toList()..sort((a, b) => count[b]!.compareTo(count[a]!));
    return dirs.isEmpty ? ['~'] : dirs.take(8).toList();
  }

  Future<void> _start() async {
    final api = ref.read(apiProvider);
    if (api == null) return;
    setState(() {
      _busy = true;
      _error = null;
    });
    // A first guess at the size; the terminal view sends the exact one the
    // moment it lays out.
    final size = MediaQuery.of(context).size;
    final font = ref.read(initialFontProvider) ?? 11.5;
    final cols = ((size.width - 12) / (font * 0.6)).floor().clamp(20, 400);
    final rows = ((size.height - 230) / (font * 1.2)).floor().clamp(8, 200);
    try {
      final pane = await api.spawn(
        widget.wall.session,
        cols: cols,
        rows: rows,
        cwd: _dir == '~' ? null : _dir,
        agent: _kind == _Kind.claude ? 'claude' : null,
        model: _kind == _Kind.claude ? _model : null,
      );
      unawaited(HapticFeedback.heavyImpact());
      ref.invalidate(wallProvider(widget.wall.session));
      if (!mounted) return;
      Navigator.of(context).pop();
      openPane(context, widget.wall.session, pane, terminal: true);
    } on Object catch (e) {
      setState(() {
        _busy = false;
        _error = '$e';
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    final inset = MediaQuery.of(context).viewInsets.bottom + MediaQuery.of(context).viewPadding.bottom;
    return Padding(
      padding: EdgeInsets.fromLTRB(12, 0, 12, 12 + inset),
      child: Hud(
        notch: Td.accent,
        glow: 0.5,
        fill: Td.ground,
        padding: const EdgeInsets.fromLTRB(16, 18, 16, 18),
        child: Column(
          mainAxisSize: MainAxisSize.min,
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            Text('START ON THE LAPTOP', style: TdType.kicker(color: Td.accent)),
            const SizedBox(height: 14),
            Row(
              children: [
                Expanded(
                  child: _Choice(
                    glyph: '✳',
                    tint: Td.cabinet,
                    title: 'Claude',
                    line: 'an agent that answers on this bench',
                    chosen: _kind == _Kind.claude,
                    onTap: () => setState(() => _kind = _Kind.claude),
                  ),
                ),
                const SizedBox(width: 10),
                Expanded(
                  child: _Choice(
                    glyph: r'$',
                    tint: Td.ink,
                    title: 'Terminal',
                    line: 'a shell, yours to drive',
                    chosen: _kind == _Kind.terminal,
                    onTap: () => setState(() => _kind = _Kind.terminal),
                  ),
                ),
              ],
            ),
            if (_kind == _Kind.claude) ...[
              const SizedBox(height: 16),
              Text('MODEL', style: TdType.kicker()),
              const SizedBox(height: 8),
              Wrap(
                spacing: 8,
                children: [
                  for (final (label, value) in const [('default', null), ('opus', 'opus'), ('sonnet', 'sonnet')])
                    _Pill(label: label, chosen: _model == value, onTap: () => setState(() => _model = value)),
                ],
              ),
            ],
            const SizedBox(height: 16),
            Text('WHERE', style: TdType.kicker()),
            const SizedBox(height: 8),
            Wrap(
              spacing: 8,
              runSpacing: 8,
              children: [
                for (final d in _dirs)
                  _Pill(label: tidyPath(d), chosen: _dir == d, onTap: () => setState(() => _dir = d)),
              ],
            ),
            if (_error != null) ...[
              const SizedBox(height: 14),
              Text(_error!, style: TdType.m(12, color: Td.decision)),
            ],
            const SizedBox(height: 20),
            SizedBox(
              width: double.infinity,
              child: Slab(
                label: _kind == _Kind.claude ? 'Start Claude' : 'Open terminal',
                icon: '▶',
                busy: _busy,
                onPressed: () => unawaited(_start()),
              ),
            ),
          ],
        ),
      ),
    );
  }
}

class _Choice extends StatelessWidget {
  const _Choice({
    required this.glyph,
    required this.tint,
    required this.title,
    required this.line,
    required this.chosen,
    required this.onTap,
  });
  final String glyph;
  final Color tint;
  final String title;
  final String line;
  final bool chosen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => GestureDetector(
    onTap: () {
      HapticFeedback.selectionClick();
      onTap();
    },
    child: AnimatedContainer(
      duration: const Duration(milliseconds: 180),
      padding: const EdgeInsets.fromLTRB(12, 12, 12, 12),
      decoration: BoxDecoration(
        color: tint.withValues(alpha: chosen ? 0.13 : 0.03),
        border: Border.all(color: tint.withValues(alpha: chosen ? 0.85 : 0.2)),
        borderRadius: BorderRadius.circular(3),
        boxShadow: chosen ? [BoxShadow(color: tint.withValues(alpha: 0.3), blurRadius: 14)] : const [],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(glyph, style: TdType.m(22, color: tint, weight: FontWeight.w700)),
          const SizedBox(height: 6),
          Text(title.toUpperCase(), style: TdType.m(13, color: Td.bright, weight: FontWeight.w700, spacing: 1.6)),
          const SizedBox(height: 3),
          Text(line, style: TdType.p(12.5, color: Td.inkAt(0.7), height: 1.3)),
        ],
      ),
    ),
  );
}

class _Pill extends StatelessWidget {
  const _Pill({required this.label, required this.chosen, required this.onTap});
  final String label;
  final bool chosen;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => GestureDetector(
    onTap: () {
      HapticFeedback.selectionClick();
      onTap();
    },
    child: AnimatedContainer(
      duration: const Duration(milliseconds: 150),
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 7),
      decoration: BoxDecoration(
        color: chosen ? Td.accent.withValues(alpha: 0.16) : Colors.transparent,
        border: Border.all(color: Td.accent.withValues(alpha: chosen ? 0.9 : 0.25)),
        borderRadius: BorderRadius.circular(20),
      ),
      child: Text(label, style: TdType.m(11.5, color: chosen ? Td.bright : Td.ink)),
    ),
  );
}
