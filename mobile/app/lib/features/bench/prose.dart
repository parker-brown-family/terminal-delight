import 'package:flutter/material.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/json.dart';

/// Agents write their prose with a little markdown in it — `code`, **bold**.
/// This draws those two and nothing else, so a card's plain reading stays
/// plain and never renders a stray symbol as structure.
List<InlineSpan> inline(String text, TextStyle base) {
  final spans = <InlineSpan>[];
  final re = RegExp(r'(\*\*[^*\n]+\*\*|`[^`\n]+`)');
  var at = 0;
  for (final m in re.allMatches(text)) {
    if (m.start > at) spans.add(TextSpan(text: text.substring(at, m.start), style: base));
    final t = m.group(0)!;
    if (t.startsWith('**')) {
      spans.add(
        TextSpan(
          text: t.substring(2, t.length - 2),
          style: base.copyWith(fontWeight: FontWeight.w700, color: Td.bright),
        ),
      );
    } else {
      spans.add(
        TextSpan(
          text: t.substring(1, t.length - 1),
          style: TdType.m((base.fontSize ?? 14) * 0.86, color: Td.cursor)
              .copyWith(backgroundColor: Td.faint.withValues(alpha: 0.55)),
        ),
      );
    }
    at = m.end;
  }
  if (at < text.length) spans.add(TextSpan(text: text.substring(at), style: base));
  return spans;
}

/// Paragraphs of prose, selectable.
class Prose extends StatelessWidget {
  const Prose(this.text, {this.size = 15.5, this.color, super.key});
  final String text;
  final double size;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final base = TdType.p(size, color: color ?? Td.bright.withValues(alpha: 0.92));
    final paras = text.split(RegExp(r'\n\s*\n')).where((p) => p.trim().isNotEmpty).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        for (var i = 0; i < paras.length; i++)
          Padding(
            padding: EdgeInsets.only(bottom: i == paras.length - 1 ? 0 : 10),
            child: SelectableText.rich(TextSpan(children: inline(paras[i].trim(), base))),
          ),
      ],
    );
  }
}

/// TDSP's rule for any register it does not name: a string is prose, an
/// array is a list, an object is facts. Applied all the way down.
class ValueView extends StatelessWidget {
  const ValueView(this.value, {this.numbered = false, this.size = 15, super.key});
  final Object? value;
  final bool numbered;
  final double size;

  @override
  Widget build(BuildContext context) {
    final v = value;
    if (v is String) return Prose(v, size: size);
    if (v is List) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (var i = 0; i < v.length; i++)
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SizedBox(
                    width: numbered ? 24 : 16,
                    child: Text(
                      numbered ? '${i + 1}.' : '·',
                      style: TdType.m(size * 0.9, color: Td.accent, weight: FontWeight.w700),
                    ),
                  ),
                  Expanded(child: ValueView(v[i], size: size)),
                ],
              ),
            ),
        ],
      );
    }
    final m = jMap(v);
    if (m != null) {
      return Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          for (final e in m.entries)
            Padding(
              padding: const EdgeInsets.only(bottom: 6),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  SizedBox(
                    width: 104,
                    child: Text(
                      e.key.replaceAll('_', ' '),
                      style: TdType.m(11, color: Td.inkAt(0.6)),
                    ),
                  ),
                  Expanded(child: ValueView(e.value, size: size - 1)),
                ],
              ),
            ),
        ],
      );
    }
    if (v == null) {
      // Absent is drawn as absent.
      return Text('unavailable', style: TdType.m(11.5, color: Td.inkAt(0.4)));
    }
    return Text('$v', style: TdType.p(size));
  }
}

/// A small upper-case label naming what kind of thing a card is.
class KindChip extends StatelessWidget {
  const KindChip(this.text, {this.tint = Td.ident, super.key});
  final String text;
  final Color tint;

  @override
  Widget build(BuildContext context) => Container(
    padding: const EdgeInsets.symmetric(horizontal: 6, vertical: 2),
    decoration: BoxDecoration(
      border: Border.all(color: tint.withValues(alpha: 0.55)),
      borderRadius: BorderRadius.circular(2),
    ),
    child: Text(text.toUpperCase(), style: TdType.m(9, color: tint, weight: FontWeight.w700, spacing: 1.4)),
  );
}
