import 'package:flutter/material.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/widgets/ground.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/core/widgets/marks.dart';
import 'package:terminal_delight/features/bench/workbench_face.dart';
import 'package:terminal_delight/features/wall/wall_page.dart';

/// One pane on the wall. Its cut corner wears its project's colour; its body
/// says what it is running, where, what it is doing — and then the one thing
/// most worth reading: for a pane at work, what you asked it; for a pane
/// waiting, why; for a pane done, the title and brief of what it said.
class PaneCard extends StatelessWidget {
  const PaneCard({required this.wall, required this.pane, this.compact = false, super.key});
  final Wall wall;
  final PaneView pane;
  final bool compact;

  @override
  Widget build(BuildContext context) {
    final p = pane;
    final state = p.state;
    final notch = wall.colourOf(p.pane);
    final place = wall.place(p.pane);
    final note = place?.$2.note;
    final age = ago(p.pulse.activityMs ?? p.latest?.mtimeMs, wall.skewMs);
    final tint = switch (state) {
      PaneState.needsYou => Td.decision,
      PaneState.working => Td.pending,
      PaneState.ended || PaneState.unknown => Td.inkAt(0.5),
      PaneState.idle => Td.accent,
    };

    Widget card(double glow) => Hud(
      tint: tint,
      notch: notch,
      spine: state == PaneState.idle ? Td.accent.withValues(alpha: 0.55) : tint,
      glow: glow,
      padding: compact
          ? const EdgeInsets.fromLTRB(12, 11, 14, 11)
          : const EdgeInsets.fromLTRB(14, 13, 16, 14),
      onTap: () => openPane(context, wall.session, p.pane),
      onLongPress: () => openPane(context, wall.session, p.pane, terminal: true),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        mainAxisSize: MainAxisSize.min,
        children: [
          Row(
            children: [
              ModeGlyph(p.mode, size: compact ? 13 : 15),
              const SizedBox(width: 8),
              Expanded(
                child: Text(
                  wall.titleOf(p.pane),
                  maxLines: 1,
                  overflow: TextOverflow.ellipsis,
                  style: TdType.m(compact ? 12.5 : 14, color: Td.bright, weight: FontWeight.w700, spacing: 0.4),
                ),
              ),
              const SizedBox(width: 8),
              StateChip(state, age: age),
            ],
          ),
          const SizedBox(height: 3),
          Padding(
            padding: EdgeInsets.only(left: compact ? 23.6 : 26),
            child: Text(
              '${tidyPath(p.cwd)}${p.attached ? '' : '  ·  not on a screen'}',
              maxLines: 1,
              overflow: TextOverflow.ellipsis,
              style: TdType.m(10.5, color: Td.inkAt(0.5)),
            ),
          ),
          ..._reading(p, compact),
          if (note != null && !compact) ...[
            const SizedBox(height: 12),
            StickyNote(note.text),
          ],
        ],
      ),
    );

    if (state == PaneState.needsYou) {
      return Breathe(builder: (context, v) => card(0.35 + 0.65 * v));
    }
    return card(0);
  }

  List<Widget> _reading(PaneView p, bool compact) {
    final lines = compact ? 2 : 3;
    switch (p.state) {
      case PaneState.needsYou:
        return [
          const SizedBox(height: 10),
          Text(
            p.pulse.why ?? 'waiting on you',
            maxLines: lines,
            overflow: TextOverflow.ellipsis,
            style: TdType.p(compact ? 13.5 : 14.5, weight: FontWeight.w500),
          ),
        ];
      case PaneState.working:
        final asked = p.pulse.prompt?.text;
        if (asked == null) break;
        return [
          const SizedBox(height: 10),
          Text.rich(
            TextSpan(
              children: [
                TextSpan(text: 'YOU ▸ ', style: TdType.m(10, color: Td.human, weight: FontWeight.w700, spacing: 1.2)),
                TextSpan(text: asked, style: TdType.p(compact ? 13.5 : 14.5, color: Td.bright.withValues(alpha: 0.9))),
              ],
            ),
            maxLines: lines,
            overflow: TextOverflow.ellipsis,
          ),
        ];
      case PaneState.idle || PaneState.unknown || PaneState.ended:
        break;
    }
    var l = p.latest;
    if (l == null) {
      final (rest, fenced) = liftFences(p.pulse.reply?.text ?? '');
      // A card printed inside the reply is still the card.
      if (fenced.isNotEmpty) {
        final doc = fenced.last;
        final m = jMap(doc['model']);
        l = Latest(
          title: jText(doc['title']),
          brief: jText(m?['brief']) ?? jText(m?['tldr']),
          layman: jText(m?['layman']),
        );
      }
      final reply = rest.trim();
      if (l == null && reply.isEmpty) return const [];
      if (l == null) {
        return [
          const SizedBox(height: 10),
          Text(
            reply,
            maxLines: lines,
            overflow: TextOverflow.ellipsis,
            style: TdType.p(compact ? 13 : 14, color: Td.bright.withValues(alpha: 0.78)),
          ),
        ];
      }
    }
    return _card(l, compact, lines);
  }

  List<Widget> _card(Latest l, bool compact, int lines) {
    if (l.title == null && l.brief == null && l.layman == null) return const [];
    return [
      if (l.title != null) ...[
        const SizedBox(height: 10),
        Text(
          l.title!,
          maxLines: 2,
          overflow: TextOverflow.ellipsis,
          style: TdType.p(compact ? 14 : 16, weight: FontWeight.w500, height: 1.3),
        ),
      ],
      if (!compact && (l.brief ?? l.layman) != null) ...[
        const SizedBox(height: 5),
        Text(
          (l.brief ?? l.layman)!,
          maxLines: lines,
          overflow: TextOverflow.ellipsis,
          style: TdType.p(13.5, color: Td.bright.withValues(alpha: 0.66)),
        ),
      ],
    ];
  }
}

/// The desk's sticky note, on paper, in the desk's hand.
class StickyNote extends StatelessWidget {
  const StickyNote(this.text, {super.key});
  final String text;

  @override
  Widget build(BuildContext context) => Transform.rotate(
    angle: -0.014,
    alignment: Alignment.centerLeft,
    child: Container(
      padding: const EdgeInsets.fromLTRB(12, 7, 12, 8),
      decoration: BoxDecoration(
        color: const Color(0xFFF3DF7A),
        borderRadius: BorderRadius.circular(1),
        boxShadow: [
          BoxShadow(color: Colors.black.withValues(alpha: 0.5), blurRadius: 8, offset: const Offset(1, 3)),
        ],
      ),
      child: Text(
        text,
        maxLines: 3,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(
          fontFamily: TdType.hand,
          fontSize: 20,
          height: 1.05,
          color: Color(0xFF3A2E08),
          fontWeight: FontWeight.w700,
        ),
      ),
    ),
  );
}
