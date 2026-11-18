import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/features/bench/prose.dart';

/// Any surface, drawn by its kind. A kind this build does not know is drawn
/// as unclassified rather than dropped (TDSP §5).
class SurfaceCard extends StatelessWidget {
  const SurfaceCard(this.s, {required this.skewMs, super.key});
  final Surface s;
  final int skewMs;

  @override
  Widget build(BuildContext context) => switch (s.kind) {
    'response' => ResponseCard(s, skewMs: skewMs),
    'decision' => _Frame(s: s, skewMs: skewMs, tint: Td.accent, child: _Decision(s.model)),
    'question' => _Frame(s: s, skewMs: skewMs, tint: Td.accent, child: _Question(s.model)),
    'markdown' => _Frame(s: s, skewMs: skewMs, child: _Markdown(jStr(s.model['body']) ?? '')),
    'table' => _Frame(s: s, skewMs: skewMs, child: _Table(s.model)),
    'changeset' => _Frame(s: s, skewMs: skewMs, tint: Td.pending, child: _Changeset(s.model)),
    _ => _Frame(s: s, skewMs: skewMs, child: ValueView(s.model.isEmpty ? null : s.model)),
  };
}

class _Frame extends StatelessWidget {
  const _Frame({required this.s, required this.skewMs, required this.child, this.tint = Td.ident});
  final Surface s;
  final int skewMs;
  final Color tint;
  final Widget child;

  @override
  Widget build(BuildContext context) => Hud(
    tint: tint,
    padding: const EdgeInsets.fromLTRB(14, 14, 16, 16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        _Heading(s: s, skewMs: skewMs, tint: tint),
        const SizedBox(height: 12),
        child,
      ],
    ),
  );
}

class _Heading extends StatelessWidget {
  const _Heading({required this.s, required this.skewMs, required this.tint});
  final Surface s;
  final int skewMs;
  final Color tint;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      Row(
        children: [
          KindChip(s.kind, tint: tint),
          const Spacer(),
          if (ago(s.mtimeMs, skewMs) != null)
            Text(ago(s.mtimeMs, skewMs)!, style: TdType.m(10.5, color: Td.inkAt(0.5))),
        ],
      ),
      if (s.title != null) ...[
        const SizedBox(height: 10),
        SelectableText(s.title!, style: TdType.p(20, weight: FontWeight.w700, height: 1.22)),
      ],
      const SizedBox(height: 6),
      Text(_origin(s), style: TdType.m(10, color: Td.inkAt(0.42))),
    ],
  );

  /// Where it came from. The mailbox cannot tell an MCP presentation from a
  /// file an agent dropped — both arrive as a file — so this says only what
  /// the file proves: it is in this pane's mailbox.
  static String _origin(Surface s) {
    final files = jList(s.source?['files']).length;
    final base = s.file == null ? "lifted from a td fence in the agent's reply" : "in this pane's mailbox";
    return files > 0 ? '$base · touches $files file${files == 1 ? '' : 's'}' : base;
  }
}

/// A turn's reply, opened on its plain reading. Registers the agent sent are
/// grouped as the desk groups them — reading, evidence, steps, other — and a
/// group with nothing in it is not drawn.
class ResponseCard extends StatefulWidget {
  const ResponseCard(this.s, {required this.skewMs, this.lead = true, super.key});
  final Surface s;
  final int skewMs;

  /// The newest card of the turn: drawn louder.
  final bool lead;

  @override
  State<ResponseCard> createState() => _ResponseCardState();
}

enum _Group { reading, evidence, steps, other }

class _ResponseCardState extends State<ResponseCard> {
  _Group _group = _Group.reading;
  String _reading = 'layman';

  static const _named = {
    'layman', 'brief', 'tldr', 'technical', 'evidence', 'doubts', 'asks', 'next', 'escalation', //
  };

  @override
  Widget build(BuildContext context) {
    final m = widget.s.model;
    final brief = m['brief'] ?? m['tldr'];
    final readings = <(String, String, Object?)>[
      if (m['layman'] != null) ('layman', 'Plain', m['layman']),
      if (m['technical'] != null) ('technical', 'Technical', m['technical']),
      // The shortest reading goes last: it is the rung a person drops to.
      if (brief != null) ('brief', 'Brief', brief),
    ];
    final hasEvidence = m['evidence'] != null || jList(m['doubts']).isNotEmpty;
    final hasSteps = m['asks'] != null || m['next'] != null;
    final extra = {for (final e in m.entries) if (!_named.contains(e.key)) e.key: e.value};
    final groups = [
      if (readings.isNotEmpty) _Group.reading,
      if (hasEvidence) _Group.evidence,
      if (hasSteps) _Group.steps,
      if (extra.isNotEmpty) _Group.other,
    ];
    final group = groups.contains(_group) ? _group : (groups.isEmpty ? _Group.reading : groups.first);
    final reading = readings.any((r) => r.$1 == _reading) ? _reading : (readings.isEmpty ? '' : readings.first.$1);

    return Hud(
      tint: Td.ident,
      spine: Td.ident,
      glow: widget.lead ? 0.15 : 0,
      padding: const EdgeInsets.fromLTRB(14, 14, 16, 16),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          _Heading(s: widget.s, skewMs: widget.skewMs, tint: Td.ident),
          if (jMap(m['escalation']) case final esc?) ...[
            const SizedBox(height: 12),
            EscalationBox(esc),
          ],
          if (groups.length > 1) ...[
            const SizedBox(height: 14),
            Row(
              children: [
                for (final g in groups)
                  _Tab(
                    label: g.name,
                    on: g == group,
                    onTap: () => setState(() => _group = g),
                  ),
              ],
            ),
          ],
          const SizedBox(height: 10),
          Container(
            width: double.infinity,
            padding: const EdgeInsets.fromLTRB(12, 11, 12, 13),
            decoration: BoxDecoration(
              color: Colors.black.withValues(alpha: 0.22),
              border: Border.all(color: Td.accent.withValues(alpha: 0.22)),
              borderRadius: BorderRadius.circular(6),
            ),
            child: switch (group) {
              _Group.reading => Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (readings.length > 1) ...[
                    Wrap(
                      spacing: 6,
                      children: [
                        for (final r in readings)
                          _Chip(label: r.$2, on: r.$1 == reading, onTap: () => setState(() => _reading = r.$1)),
                      ],
                    ),
                    const SizedBox(height: 12),
                  ],
                  AnimatedSwitcher(
                    duration: const Duration(milliseconds: 180),
                    child: KeyedSubtree(
                      key: ValueKey(reading),
                      child: ValueView(
                        readings.where((r) => r.$1 == reading).firstOrNull?.$3,
                        size: reading == 'technical' ? 14 : 16,
                      ),
                    ),
                  ),
                ],
              ),
              _Group.evidence => Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (m['evidence'] != null) ...[
                    Text('WHAT WAS VERIFIED', style: TdType.kicker(color: Td.settled)),
                    const SizedBox(height: 8),
                    ValueView(m['evidence'], size: 14),
                  ],
                  if (jList(m['doubts']).isNotEmpty) ...[
                    const SizedBox(height: 10),
                    DoubtsBlock(jList(m['doubts'])),
                  ],
                ],
              ),
              _Group.steps => Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  if (m['asks'] != null) ...[
                    Text('NEEDS FROM YOU', style: TdType.kicker(color: Td.decision)),
                    const SizedBox(height: 8),
                    ValueView(m['asks'], size: 14.5),
                    const SizedBox(height: 10),
                  ],
                  if (m['next'] != null) ...[
                    Text("WHAT'S NEXT", style: TdType.kicker(color: Td.accent)),
                    const SizedBox(height: 8),
                    ValueView(m['next'], numbered: true, size: 14.5),
                  ],
                ],
              ),
              _Group.other => Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  for (final e in extra.entries) ...[
                    Text(e.key.replaceAll('_', ' ').toUpperCase(), style: TdType.kicker()),
                    const SizedBox(height: 6),
                    ValueView(e.value, size: 14),
                    const SizedBox(height: 10),
                  ],
                ],
              ),
            },
          ),
          const SizedBox(height: 10),
          WeightLine(widget.s.weight),
        ],
      ),
    );
  }
}

class _Tab extends StatelessWidget {
  const _Tab({required this.label, required this.on, required this.onTap});
  final String label;
  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => GestureDetector(
    onTap: () {
      HapticFeedback.selectionClick();
      onTap();
    },
    child: Container(
      margin: const EdgeInsets.only(right: 14),
      padding: const EdgeInsets.only(bottom: 5),
      decoration: BoxDecoration(
        border: Border(bottom: BorderSide(color: on ? Td.accent : Colors.transparent, width: 2)),
      ),
      child: Text(
        label.toUpperCase(),
        style: TdType.m(10.5, color: on ? Td.bright : Td.inkAt(0.5), weight: FontWeight.w700, spacing: 1.6),
      ),
    ),
  );
}

class _Chip extends StatelessWidget {
  const _Chip({required this.label, required this.on, required this.onTap});
  final String label;
  final bool on;
  final VoidCallback onTap;

  @override
  Widget build(BuildContext context) => GestureDetector(
    onTap: () {
      HapticFeedback.selectionClick();
      onTap();
    },
    child: AnimatedContainer(
      duration: const Duration(milliseconds: 140),
      padding: const EdgeInsets.symmetric(horizontal: 9, vertical: 4),
      decoration: BoxDecoration(
        color: on ? Td.accent.withValues(alpha: 0.18) : Colors.transparent,
        border: Border.all(color: Td.accent.withValues(alpha: on ? 0.8 : 0.22)),
        borderRadius: BorderRadius.circular(12),
      ),
      child: Text(label, style: TdType.m(11, color: on ? Td.bright : Td.ink)),
    ),
  );
}

/// "◆ NEEDS YOU" — the one thing on a card that interrupts. Drawn only when
/// the agent declared a level and something is still unanswered.
class EscalationBox extends StatelessWidget {
  const EscalationBox(this.e, {super.key});
  final Json e;

  @override
  Widget build(BuildContext context) {
    final level = jStr(e['level']);
    if (level != 'blocking' && level != 'wanted') return const SizedBox.shrink();
    final items = jList(e['items']).map(jMap).nonNulls.toList();
    final open = items.where((i) => jBool(i['answered']) != true).length;
    if (items.isNotEmpty && open == 0) return const SizedBox.shrink();
    final blocking = level == 'blocking';
    final tint = Td.decision.withValues(alpha: blocking ? 1 : 0.7);
    return Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(12, 10, 12, 12),
      decoration: BoxDecoration(
        color: Colors.black.withValues(alpha: 0.4),
        border: Border.all(color: tint, width: blocking ? 2 : 1.4),
        borderRadius: BorderRadius.circular(8),
        boxShadow: [BoxShadow(color: tint.withValues(alpha: blocking ? 0.35 : 0.18), blurRadius: 16)],
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text(
            '◆ NEEDS YOU · $open unanswered · ${blocking ? 'BLOCKING' : 'WANTED · WORK CONTINUES'}',
            style: TdType.m(10.5, color: tint, weight: FontWeight.w700, spacing: 1.1),
          ),
          if (jText(e['why']) case final why?) ...[
            const SizedBox(height: 8),
            Prose(why, size: 14.5),
          ],
          for (final i in items) ...[
            const SizedBox(height: 8),
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  jBool(i['answered']) ?? false ? '☑ ' : '☐ ',
                  style: TdType.m(14, color: tint),
                ),
                Expanded(child: Prose(jStr(i['ask']) ?? '', size: 14.5)),
              ],
            ),
          ],
        ],
      ),
    );
  }
}

/// ARTICLES OF DOUBT: every claim the agent is not sure of, with how sure.
class DoubtsBlock extends StatelessWidget {
  const DoubtsBlock(this.doubts, {super.key});
  final List<Object?> doubts;

  static Color _tint(String? c) => switch (c) {
    'measured' => Td.settled,
    'inferred' || 'hunch' => Td.pending,
    _ => Td.inkAt(0.45),
  };

  @override
  Widget build(BuildContext context) {
    final hunches = doubts.where((d) => jStr(jMap(d)?['confidence']) == 'hunch').length;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Text(
          'ARTICLES OF DOUBT · ${doubts.length} doubt${doubts.length == 1 ? '' : 's'}'
          '${hunches > 0 ? ' · $hunches hunch${hunches == 1 ? '' : 'es'}' : ''}',
          style: TdType.kicker(color: Td.pending),
        ),
        const SizedBox(height: 8),
        for (final d in doubts)
          Padding(
            padding: const EdgeInsets.only(bottom: 10),
            child: Builder(
              builder: (context) {
                final m = jMap(d);
                if (m == null) return Prose('$d', size: 14);
                final conf = jStr(m['confidence']);
                return Column(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Expanded(child: Prose(jStr(m['claim']) ?? '', size: 14.5)),
                        const SizedBox(width: 8),
                        KindChip(conf ?? 'undeclared', tint: _tint(conf)),
                      ],
                    ),
                    if (jText(m['why']) case final why?) ...[
                      const SizedBox(height: 3),
                      Prose(why, size: 13, color: Td.inkAt(0.62)),
                    ],
                  ],
                );
              },
            ),
          ),
      ],
    );
  }
}

/// How big the work is and how deep it goes, as the agent weighed it — or a
/// plain statement that nobody said.
class WeightLine extends StatelessWidget {
  const WeightLine(this.w, {super.key});
  final Json? w;

  @override
  Widget build(BuildContext context) {
    final w = this.w;
    if (w == null) {
      return Text(
        'Nobody said how big this is or how deep it goes.',
        style: TdType.m(10, color: Td.inkAt(0.35)),
      );
    }
    final f = jMap(w['foundation']);
    final parts = <(String, String?, Color)>[
      ('effort', jStr(w['effort']), Td.ink),
      ('complexity', jStr(w['complexity']), Td.ink),
      (
        'in',
        f == null ? null : [jStr(f['system']), if (jStr(f['depth']) != null) '(${jStr(f['depth'])})'].nonNulls.join(' '),
        jStr(f?['depth']) == 'bedrock' ? Td.human : Td.ink,
      ),
      (
        'confidence',
        jStr(w['confidence']),
        switch (jStr(w['confidence'])) {
          'measured' => Td.settled,
          'inferred' || 'hunch' => Td.pending,
          _ => Td.inkAt(0.5),
        },
      ),
    ];
    return Wrap(
      spacing: 12,
      runSpacing: 4,
      children: [
        for (final (k, v, c) in parts)
          if (v != null && v.isNotEmpty)
            Text.rich(
              TextSpan(
                children: [
                  TextSpan(text: '$k ', style: TdType.m(10, color: Td.inkAt(0.4))),
                  TextSpan(text: v, style: TdType.m(10, color: c, weight: FontWeight.w700)),
                ],
              ),
            ),
      ],
    );
  }
}

class _Decision extends StatelessWidget {
  const _Decision(this.m);
  final Json m;

  @override
  Widget build(BuildContext context) {
    final options = jList(m['options']).map(jMap).nonNulls.toList();
    final consequences = jList(m['consequences']);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (jText(m['question']) case final q?) ...[Prose(q, size: 16), const SizedBox(height: 12)],
        for (final o in options) ...[
          Container(
            width: double.infinity,
            padding: const EdgeInsets.fromLTRB(12, 10, 12, 11),
            decoration: BoxDecoration(
              color: Colors.black.withValues(alpha: 0.25),
              border: Border(
                left: BorderSide(
                  color: jBool(o['recommended']) ?? false ? Td.accent : Td.faint,
                  width: 3,
                ),
              ),
            ),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        jStr(o['name']) ?? 'option',
                        style: TdType.p(15.5, weight: FontWeight.w700),
                      ),
                    ),
                    if (jBool(o['recommended']) ?? false) const KindChip('recommended', tint: Td.accent),
                  ],
                ),
                if (jText(o['case']) case final c?) ...[const SizedBox(height: 5), Prose(c, size: 13.5)],
                if (jText(o['cost']) case final c?) ...[
                  const SizedBox(height: 4),
                  Text('cost · $c', style: TdType.p(12.5, color: Td.inkAt(0.55))),
                ],
              ],
            ),
          ),
          const SizedBox(height: 8),
        ],
        if (consequences.isNotEmpty) ...[
          const SizedBox(height: 4),
          Text('IF WE DO', style: TdType.kicker()),
          const SizedBox(height: 6),
          ValueView(consequences, size: 13.5),
        ],
        const SizedBox(height: 4),
        Text('Decide on the desk — answering from the phone is the next slice.', style: TdType.m(10, color: Td.inkAt(0.4))),
      ],
    );
  }
}

class _Question extends StatelessWidget {
  const _Question(this.m);
  final Json m;

  @override
  Widget build(BuildContext context) {
    final options = jList(m['options']);
    final answer = jInt(m['answer']);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (jText(m['question']) case final q?) ...[Prose(q, size: 16), const SizedBox(height: 12)],
        Wrap(
          spacing: 8,
          runSpacing: 8,
          children: [
            for (var i = 0; i < options.length; i++)
              Container(
                padding: const EdgeInsets.symmetric(horizontal: 12, vertical: 8),
                decoration: BoxDecoration(
                  border: Border.all(
                    color: answer == i ? Td.settled : Td.inkAt(0.28),
                    width: answer == i ? 1.6 : 1,
                  ),
                  borderRadius: BorderRadius.circular(3),
                ),
                child: Text(
                  jStr(options[i]) ?? jStr(jMap(options[i])?['label']) ?? '?',
                  style: TdType.p(13.5, color: answer == i ? Td.settled : Td.bright),
                ),
              ),
          ],
        ),
        const SizedBox(height: 10),
        Text(
          answer != null ? 'answered' : 'Answer on the desk — answering from the phone is the next slice.',
          style: TdType.m(10, color: Td.inkAt(0.4)),
        ),
      ],
    );
  }
}

class _Markdown extends StatelessWidget {
  const _Markdown(this.body);
  final String body;

  @override
  Widget build(BuildContext context) {
    final out = <Widget>[];
    final lines = body.split('\n');
    var i = 0;
    final para = <String>[];
    void flush() {
      if (para.isEmpty) return;
      out.add(Padding(padding: const EdgeInsets.only(bottom: 10), child: Prose(para.join(' '), size: 14.5)));
      para.clear();
    }

    while (i < lines.length) {
      final l = lines[i];
      if (l.trimLeft().startsWith('```')) {
        flush();
        final code = <String>[];
        i++;
        while (i < lines.length && !lines[i].trimLeft().startsWith('```')) {
          code.add(lines[i]);
          i++;
        }
        out.add(
          Container(
            width: double.infinity,
            margin: const EdgeInsets.only(bottom: 10),
            padding: const EdgeInsets.all(10),
            color: Colors.black.withValues(alpha: 0.45),
            child: SingleChildScrollView(
              scrollDirection: Axis.horizontal,
              child: SelectableText(code.join('\n'), style: TdType.m(11.5)),
            ),
          ),
        );
      } else if (RegExp('^#{1,6} ').hasMatch(l)) {
        flush();
        final level = l.indexOf(' ');
        out.add(
          Padding(
            padding: const EdgeInsets.only(bottom: 8, top: 4),
            child: Text(
              l.substring(level + 1),
              style: TdType.p(level <= 1 ? 19 : (level == 2 ? 17 : 15.5), weight: FontWeight.w700),
            ),
          ),
        );
      } else if (RegExp(r'^\s*[-*] ').hasMatch(l)) {
        flush();
        out.add(
          Padding(
            padding: const EdgeInsets.only(bottom: 5),
            child: Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text('·  ', style: TdType.m(14, color: Td.accent)),
                Expanded(child: Prose(l.replaceFirst(RegExp(r'^\s*[-*] '), ''), size: 14.5)),
              ],
            ),
          ),
        );
      } else if (l.trim().isEmpty) {
        flush();
      } else {
        para.add(l.trim());
      }
      i++;
    }
    flush();
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: out);
  }
}

class _Table extends StatelessWidget {
  const _Table(this.m);
  final Json m;

  @override
  Widget build(BuildContext context) {
    final cols = jList(m['columns']).map((c) => jStr(c) ?? jStr(jMap(c)?['name']) ?? '').toList();
    final rows = jList(m['rows']).map(jList).toList();
    return SingleChildScrollView(
      scrollDirection: Axis.horizontal,
      child: DataTable(
        headingRowHeight: 34,
        dataRowMinHeight: 30,
        dataRowMaxHeight: 60,
        columnSpacing: 18,
        horizontalMargin: 4,
        headingTextStyle: TdType.m(10.5, color: Td.accent, weight: FontWeight.w700, spacing: 1),
        dataTextStyle: TdType.p(13),
        columns: [for (final c in cols) DataColumn(label: Text(c.toUpperCase()))],
        rows: [
          for (final r in rows)
            DataRow(
              cells: [
                for (var i = 0; i < cols.length; i++)
                  DataCell(
                    i < r.length && r[i] != null
                        ? Text('${r[i]}')
                        : Text('unavailable', style: TdType.m(10.5, color: Td.inkAt(0.4))),
                  ),
              ],
            ),
        ],
      ),
    );
  }
}

class _Changeset extends StatelessWidget {
  const _Changeset(this.m);
  final Json m;

  @override
  Widget build(BuildContext context) {
    final hunks = jList(m['hunks']).map(jMap).nonNulls.toList();
    final byFile = <String, (int, int)>{};
    for (final h in hunks) {
      final file = jStr(h['file']) ?? '?';
      var add = 0;
      var del = 0;
      for (final line in (jStr(h['patch']) ?? '').split('\n')) {
        if (line.startsWith('+') && !line.startsWith('+++')) add++;
        if (line.startsWith('-') && !line.startsWith('---')) del++;
      }
      final prev = byFile[file] ?? (0, 0);
      byFile[file] = (prev.$1 + add, prev.$2 + del);
    }
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        if (jText(m['repository']) case final r?) ...[
          Text(tidyPath(r), style: TdType.m(11, color: Td.inkAt(0.6))),
          const SizedBox(height: 8),
        ],
        for (final e in byFile.entries)
          Padding(
            padding: const EdgeInsets.only(bottom: 5),
            child: Row(
              children: [
                Expanded(child: Text(e.key, style: TdType.m(12, color: Td.bright), overflow: TextOverflow.ellipsis)),
                Text('+${e.value.$1}', style: TdType.m(12, color: Td.settled)),
                const SizedBox(width: 8),
                Text('−${e.value.$2}', style: TdType.m(12, color: Td.decision)),
              ],
            ),
          ),
      ],
    );
  }
}
