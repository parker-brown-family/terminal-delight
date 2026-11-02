import 'dart:async';
import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/widgets/ground.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/features/bench/prose.dart';
import 'package:terminal_delight/features/bench/surface_cards.dart';

/// One exchange: what the person said, and everything the agent made or said
/// back before the person spoke again.
class Turn {
  Turn(this.prompt, this.startMs);
  final Said? prompt;
  final int startMs;
  final responses = <Surface>[];
  Said? reply;
}

/// Cut the channel and the surfaces into turns, by the person's prompts. A
/// surface belongs to the turn whose prompt came before it.
List<Turn> turnsOf(Bench b) {
  final prompts = b.timeline
      .where((r) => r.side == 'in' && r.type == 'prompt' && r.atMs != null)
      .map((r) => Said(r.text, r.atMs, harness: r.harness))
      .toList();
  final turns = <Turn>[Turn(null, 0), for (final p in prompts) Turn(p, p.atMs!)];
  Turn at(int t) {
    for (var i = turns.length - 1; i >= 0; i--) {
      if (turns[i].startMs <= t) return turns[i];
    }
    return turns.first;
  }

  for (final s in b.surfaces.where((s) => s.kind == 'response')) {
    at(s.mtimeMs ?? 0).responses.add(s);
  }
  for (final r in b.timeline.where((r) => r.side == 'in' && r.type == 'reply' && r.atMs != null)) {
    final turn = at(r.atMs!);
    final (rest, fenced) = liftFences(r.text ?? '');
    for (final (i, doc) in fenced.indexed) {
      turn.responses.add(Surface(id: 'fence:${r.atMs}:$i', doc: doc, mtimeMs: r.atMs));
    }
    turn.reply = rest.trim().isEmpty ? null : Said(rest.trim(), r.atMs);
  }
  for (final t in turns) {
    t.responses.sort((a, b) => (b.mtimeMs ?? 0).compareTo(a.mtimeMs ?? 0));
  }
  return turns.where((t) => t.prompt != null || t.responses.isNotEmpty || t.reply != null).toList();
}

/// An agent with no file and no MCP connection prints its card in its reply,
/// inside a ```td fence — the briefing's last resort. The desk lifts those out
/// (`app/src/derive.rs`), and so does the phone: each fence that parses as a
/// document becomes a card, and the prose around it stays the reply. Only a
/// reply is read this way; a fence the person typed is not the agent
/// presenting anything.
(String, List<Json>) liftFences(String text) {
  final docs = <Json>[];
  final re = RegExp(r'```td[ \t]*\r?\n([\s\S]*?)\r?\n```', multiLine: true);
  final rest = text.replaceAllMapped(re, (m) {
    try {
      final doc = jMap(jsonDecode(m.group(1)!));
      if (doc != null && jStr(doc['kind']) != null) {
        docs.add(doc);
        return '';
      }
    } on FormatException {
      // Not a document: leave it in the prose where it was written.
    }
    return m.group(0)!;
  });
  return (rest, docs);
}

class WorkbenchFace extends ConsumerWidget {
  const WorkbenchFace({required this.session, required this.pane, super.key});
  final String session;
  final int pane;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final bench = ref.watch(benchProvider((session, pane)));
    final b = bench.valueOrNull;
    if (b == null) {
      return bench.hasError
          ? Center(child: Text('${bench.error}', style: TdType.m(12, color: Td.decision)))
          : const Center(child: CircularProgressIndicator(strokeWidth: 1.5));
    }
    final turns = turnsOf(b);
    final current = turns.isEmpty ? null : turns.last;
    final earlier = turns.length > 1 ? turns.sublist(0, turns.length - 1).reversed.take(14).toList() : <Turn>[];
    final others = b.surfaces.where((s) => s.kind != 'response').toList();
    final state = b.info?.state ?? b.pulse.state;
    final bottom = MediaQuery.of(context).padding.bottom;
    var k = 0;

    return RefreshIndicator(
      color: Td.accent,
      backgroundColor: Td.surface,
      onRefresh: () => ref.refresh(benchProvider((session, pane)).future),
      child: ListView(
        physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
        padding: EdgeInsets.fromLTRB(14, 14, 14, 40 + bottom),
        children: [
          if (state == PaneState.needsYou)
            RiseIn(
              index: k++,
              child: Padding(
                padding: const EdgeInsets.only(bottom: 14),
                child: _NeedsYou(why: b.pulse.why),
              ),
            ),
          if (current == null && others.isEmpty)
            _Empty(state: state)
          else if (current != null) ...[
            const Padding(
              padding: EdgeInsets.only(bottom: 10),
              child: Kicker('This turn', color: Td.accent),
            ),
            if (current.prompt != null)
              RiseIn(index: k++, child: YouSaid(current.prompt!, skewMs: b.skewMs)),
            if (state == PaneState.working)
              RiseIn(index: k++, child: _InFlight(sinceMs: current.prompt?.atMs, skewMs: b.skewMs)),
            for (final (i, s) in current.responses.indexed)
              RiseIn(
                index: k++,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: s.kind == 'response' ? ResponseCard(s, skewMs: b.skewMs, lead: i == 0) : SurfaceCard(s, skewMs: b.skewMs),
                ),
              ),
            if (current.responses.isEmpty && current.reply?.text != null && state != PaneState.working)
              RiseIn(index: k++, child: _Reply(current.reply!, skewMs: b.skewMs)),
          ],
          if (others.isNotEmpty) ...[
            const Padding(
              padding: EdgeInsets.only(top: 14, bottom: 10),
              child: Kicker('On the bench'),
            ),
            for (final s in others)
              RiseIn(
                index: k++,
                child: Padding(
                  padding: const EdgeInsets.only(bottom: 12),
                  child: SurfaceCard(s, skewMs: b.skewMs),
                ),
              ),
          ],
          if (earlier.isNotEmpty) ...[
            const Padding(
              padding: EdgeInsets.only(top: 14, bottom: 10),
              child: Kicker('Earlier'),
            ),
            for (final t in earlier) _EarlierTurn(t, skewMs: b.skewMs),
          ],
        ],
      ),
    );
  }
}

/// The person's own words, in the person's colour.
class YouSaid extends StatelessWidget {
  const YouSaid(this.said, {required this.skewMs, this.lines, super.key});
  final Said said;
  final int skewMs;
  final int? lines;

  @override
  Widget build(BuildContext context) {
    if (said.harness) return _HarnessSaid(said, skewMs: skewMs);
    return _person(context);
  }

  Widget _person(BuildContext context) => Padding(
    padding: const EdgeInsets.only(bottom: 12, left: 28),
    child: Container(
      width: double.infinity,
      padding: const EdgeInsets.fromLTRB(12, 9, 12, 11),
      decoration: BoxDecoration(
        color: Td.human.withValues(alpha: 0.07),
        border: Border(right: BorderSide(color: Td.human.withValues(alpha: 0.8), width: 3)),
      ),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.end,
        children: [
          Text(
            ['YOU', ?ago(said.atMs, skewMs)].join(' · '),
            style: TdType.m(9.5, color: Td.human, weight: FontWeight.w700, spacing: 1.4),
          ),
          const SizedBox(height: 5),
          if (lines == null)
            SelectableText(said.text ?? '', style: TdType.p(14.5, color: Td.bright.withValues(alpha: 0.9)))
          else
            Text(
              said.text ?? '',
              maxLines: lines,
              overflow: TextOverflow.ellipsis,
              textAlign: TextAlign.right,
              style: TdType.p(14, color: Td.bright.withValues(alpha: 0.8)),
            ),
        ],
      ),
    ),
  );
}

/// A turn the harness opened by itself. Said what it was, in a line, in the
/// quietest ink on the bench — and never in the person's colour.
class _HarnessSaid extends StatelessWidget {
  const _HarnessSaid(this.said, {required this.skewMs});
  final Said said;
  final int skewMs;

  @override
  Widget build(BuildContext context) {
    final t = said.text ?? '';
    final what = t.contains('<task-notification>')
        ? 'a background task reported back'
        : t.startsWith('<agent-message') || t.startsWith('<teammate-message')
        ? 'another agent reported back'
        : t.startsWith('<cross-session-message')
        ? 'another session wrote in'
        : t.contains('<command-name>')
        ? 'a slash command ran'
        : t.contains('<bash-')
        ? 'a shell escape ran'
        : 'the harness spoke';
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Row(
        children: [
          Text('⚙  ', style: TdType.m(11, color: Td.inkAt(0.45))),
          Expanded(
            child: Text(
              what.toUpperCase(),
              style: TdType.m(9.5, color: Td.inkAt(0.5), weight: FontWeight.w700, spacing: 1.4),
            ),
          ),
          Text(ago(said.atMs, skewMs) ?? '', style: TdType.m(9.5, color: Td.inkAt(0.4))),
        ],
      ),
    );
  }
}

class _InFlight extends StatefulWidget {
  const _InFlight({required this.sinceMs, required this.skewMs});
  final int? sinceMs;
  final int skewMs;

  @override
  State<_InFlight> createState() => _InFlightState();
}

class _InFlightState extends State<_InFlight> {
  Timer? _t;

  @override
  void initState() {
    super.initState();
    _t = Timer.periodic(const Duration(seconds: 1), (_) => setState(() {}));
  }

  @override
  void dispose() {
    _t?.cancel();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final since = widget.sinceMs;
    var clock = '';
    if (since != null) {
      final s = ((DateTime.now().millisecondsSinceEpoch + widget.skewMs - since) / 1000).floor().clamp(0, 1 << 30);
      clock = s < 60 ? '${s}s' : '${s ~/ 60}m ${(s % 60).toString().padLeft(2, '0')}s';
    }
    return Padding(
      padding: const EdgeInsets.only(bottom: 12),
      child: Breathe(
        period: 1400,
        builder: (context, v) => Hud(
          tint: Td.pending,
          glow: 0.2 + 0.3 * v,
          padding: const EdgeInsets.fromLTRB(12, 12, 14, 12),
          child: Row(
            children: [
              SizedBox(
                width: 16,
                height: 16,
                child: CircularProgressIndicator(strokeWidth: 1.6, color: Td.pending.withValues(alpha: 0.9)),
              ),
              const SizedBox(width: 12),
              Expanded(
                child: Text('IN FLIGHT', style: TdType.m(12, color: Td.pending, weight: FontWeight.w700, spacing: 2)),
              ),
              Text(clock, style: TdType.m(12, color: Td.pending)),
            ],
          ),
        ),
      ),
    );
  }
}

class _NeedsYou extends StatelessWidget {
  const _NeedsYou({required this.why});
  final String? why;

  @override
  Widget build(BuildContext context) => Breathe(
    builder: (context, v) => Hud(
      tint: Td.decision,
      notch: Td.decision,
      glow: 0.4 + 0.6 * v,
      padding: const EdgeInsets.fromLTRB(12, 12, 16, 14),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('◆ NEEDS YOU', style: TdType.m(12, color: Td.decision, weight: FontWeight.w700, spacing: 2)),
          const SizedBox(height: 8),
          Prose(why ?? 'The agent is waiting on you.'),
          const SizedBox(height: 10),
          Text(
            'Answer it in the terminal face — or on the desk.',
            style: TdType.m(10.5, color: Td.inkAt(0.5)),
          ),
        ],
      ),
    ),
  );
}

/// A reply the agent gave without presenting a card: its hooks carried the
/// words, so they are exact, just not shaped.
class _Reply extends StatelessWidget {
  const _Reply(this.reply, {required this.skewMs});
  final Said reply;
  final int skewMs;

  @override
  Widget build(BuildContext context) => Hud(
    tint: Td.ident,
    padding: const EdgeInsets.fromLTRB(14, 14, 16, 16),
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            const KindChip('reply'),
            const Spacer(),
            Text(ago(reply.atMs, skewMs) ?? '', style: TdType.m(10.5, color: Td.inkAt(0.5))),
          ],
        ),
        const SizedBox(height: 6),
        Text(
          "arrived through this agent's own hooks · no card was presented",
          style: TdType.m(10, color: Td.inkAt(0.42)),
        ),
        const SizedBox(height: 12),
        Prose(reply.text ?? '', size: 15),
      ],
    ),
  );
}

class _EarlierTurn extends StatefulWidget {
  const _EarlierTurn(this.t, {required this.skewMs});
  final Turn t;
  final int skewMs;

  @override
  State<_EarlierTurn> createState() => _EarlierTurnState();
}

class _EarlierTurnState extends State<_EarlierTurn> {
  bool _open = false;

  @override
  Widget build(BuildContext context) {
    final t = widget.t;
    final head = t.responses.firstOrNull?.title ?? t.reply?.text ?? 'no reply on record';
    return Padding(
      padding: const EdgeInsets.only(bottom: 8),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Hud(
            tint: Td.inkAt(0.4),
            spine: Td.ident.withValues(alpha: 0.5),
            padding: const EdgeInsets.fromLTRB(12, 10, 14, 11),
            onTap: () => setState(() => _open = !_open),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                if (t.prompt?.text != null && !t.prompt!.harness)
                  Text.rich(
                    TextSpan(
                      children: [
                        TextSpan(text: 'YOU ▸ ', style: TdType.m(9.5, color: Td.human, weight: FontWeight.w700)),
                        TextSpan(text: t.prompt!.text, style: TdType.p(13, color: Td.inkAt(0.75))),
                      ],
                    ),
                    maxLines: 2,
                    overflow: TextOverflow.ellipsis,
                  ),
                const SizedBox(height: 5),
                Row(
                  crossAxisAlignment: CrossAxisAlignment.start,
                  children: [
                    Expanded(
                      child: Text(
                        head,
                        maxLines: 2,
                        overflow: TextOverflow.ellipsis,
                        style: TdType.p(14.5, weight: FontWeight.w500),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Text(
                      '${ago(t.prompt?.atMs ?? t.responses.firstOrNull?.mtimeMs, widget.skewMs) ?? ''} ${_open ? '▴' : '▾'}',
                      style: TdType.m(10.5, color: Td.inkAt(0.5)),
                    ),
                  ],
                ),
              ],
            ),
          ),
          AnimatedSize(
            duration: const Duration(milliseconds: 220),
            curve: Curves.easeOutCubic,
            child: !_open
                ? const SizedBox(width: double.infinity)
                : Padding(
                    padding: const EdgeInsets.only(top: 8),
                    child: Column(
                      children: [
                        if (t.prompt != null) YouSaid(t.prompt!, skewMs: widget.skewMs),
                        for (final s in t.responses)
                          Padding(
                            padding: const EdgeInsets.only(bottom: 10),
                            child: s.kind == 'response' ? ResponseCard(s, skewMs: widget.skewMs, lead: false) : SurfaceCard(s, skewMs: widget.skewMs),
                          ),
                        if (t.responses.isEmpty && t.reply != null) _Reply(t.reply!, skewMs: widget.skewMs),
                      ],
                    ),
                  ),
          ),
        ],
      ),
    );
  }
}

class _Empty extends StatelessWidget {
  const _Empty({required this.state});
  final PaneState state;

  @override
  Widget build(BuildContext context) => Padding(
    padding: const EdgeInsets.only(top: 40),
    child: Hud(
      tint: Td.inkAt(0.4),
      padding: const EdgeInsets.fromLTRB(14, 16, 16, 18),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Text('NOTHING ON THIS BENCH', style: TdType.m(12, weight: FontWeight.w700, spacing: 1.8)),
          const SizedBox(height: 10),
          Prose(
            state == PaneState.unknown
                ? 'This pane has no hooked agent, so there are no turns to read — only its screen. '
                      'The terminal face shows that.'
                : 'No turns on record yet. When the agent answers, its card lands here.',
            size: 14.5,
            color: Td.inkAt(0.75),
          ),
        ],
      ),
    ),
  );
}
