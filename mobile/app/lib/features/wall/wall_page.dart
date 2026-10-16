import 'dart:async';

import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/link.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/core/widgets/ground.dart';
import 'package:terminal_delight/core/widgets/hud.dart';
import 'package:terminal_delight/core/widgets/marks.dart';
import 'package:terminal_delight/features/wall/new_sheet.dart';
import 'package:terminal_delight/features/wall/pane_card.dart';

class WallPage extends ConsumerWidget {
  const WallPage({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    // Held open for as long as the wall is on screen.
    ref.watch(eventsProvider);
    final session = ref.watch(sessionProvider);
    return Ground(
      child: Scaffold(
        body: switch (session) {
          AsyncValue(valueOrNull: final String s) => _Wall(session: s),
          AsyncError(error: final e) => Trouble(error: e),
          _ => const Seeking(),
        },
      ),
    );
  }
}

class _Wall extends ConsumerWidget {
  const _Wall({required this.session});
  final String session;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final wall = ref.watch(wallProvider(session));
    return switch (wall) {
      AsyncValue(valueOrNull: final Wall w) => _WallBody(wall: w),
      AsyncError(error: final e) => Trouble(error: e),
      _ => const Seeking(),
    };
  }
}

/// One section of the wall: a project, a group with no project, the loose
/// tabs, or the panes the desk's layout does not claim.
class _Section {
  _Section(this.title, this.color, {this.hint});
  final String title;
  final Color? color;
  final String? hint;
  final rows = <Object>[]; // a String sub-heading, or a pane id
}

List<_Section> _sections(Wall w) {
  final t = w.tree;
  final live = {for (final p in w.panes) p.pane};
  final out = <_Section>[];
  final placed = <int>{};

  void addTab(_Section s, DeskTab tab) {
    for (final leaf in tab.leaves) {
      if (live.contains(leaf.pane) && placed.add(leaf.pane)) s.rows.add(leaf.pane);
    }
  }

  for (final project in t.projects) {
    final s = _Section(project.name ?? 'PROJECT', project.color);
    for (final tab in t.tabs.where((x) => x.project == project.id && x.group == null)) {
      addTab(s, tab);
    }
    for (final g in t.groups.where((g) => g.project == project.id)) {
      s.rows.add(g.name ?? 'group');
      final before = s.rows.length;
      for (final tab in t.tabs.where((x) => x.group == g.id)) {
        addTab(s, tab);
      }
      if (s.rows.length == before) s.rows.removeLast();
    }
    if (s.rows.isNotEmpty) out.add(s);
  }
  for (final g in t.groups.where((g) => g.project == null)) {
    final s = _Section(g.name ?? 'GROUP', g.color);
    for (final tab in t.tabs.where((x) => x.group == g.id)) {
      addTab(s, tab);
    }
    if (s.rows.isNotEmpty) out.add(s);
  }
  final loose = _Section('LOOSE TABS', null);
  for (final tab in t.tabs.where((x) => x.group == null && x.project == null)) {
    addTab(loose, tab);
  }
  if (loose.rows.isNotEmpty) out.add(loose);

  final host = _Section(
    'HOST ONLY',
    null,
    hint: "Running on the laptop, not in the desk's layout — started from here, or left behind.",
  );
  for (final p in w.panes) {
    if (!placed.contains(p.pane) && !p.ended) host.rows.add(p.pane);
  }
  if (host.rows.isNotEmpty) out.add(host);
  return out;
}

class _WallBody extends ConsumerWidget {
  const _WallBody({required this.wall});
  final Wall wall;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final w = wall;
    final sections = _sections(w);
    final now = [...w.panes.where((p) => p.state == PaneState.needsYou || p.state == PaneState.working)]
      ..sort((a, b) {
        final u = a.state.urgency.compareTo(b.state.urgency);
        if (u != 0) return u;
        return (b.pulse.activityMs ?? 0).compareTo(a.pulse.activityMs ?? 0);
      });
    final hello = ref.watch(helloProvider).valueOrNull;
    final pad = MediaQuery.of(context).padding;
    var i = 0;
    return Stack(
      children: [
        RefreshIndicator(
          color: Td.accent,
          backgroundColor: Td.surface,
          onRefresh: () async {
            ref
              ..invalidate(helloProvider)
              ..invalidate(wallProvider(w.session));
            await ref.read(wallProvider(w.session).future);
          },
          child: CustomScrollView(
            physics: const AlwaysScrollableScrollPhysics(parent: BouncingScrollPhysics()),
            slivers: [
              SliverPadding(
                padding: EdgeInsets.fromLTRB(16, pad.top + 14, 16, 0),
                sliver: SliverToBoxAdapter(
                  child: _Header(wall: w, machine: hello?.machine, sessions: hello?.sessions.length),
                ),
              ),
              SliverPadding(
                padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                sliver: SliverToBoxAdapter(child: _PulseStrip(panes: w.panes)),
              ),
              if (now.isNotEmpty) ...[
                const SliverPadding(
                  padding: EdgeInsets.fromLTRB(16, 18, 16, 10),
                  sliver: SliverToBoxAdapter(child: Kicker('Now', color: Td.pending)),
                ),
                SliverToBoxAdapter(
                  child: SizedBox(
                    height: 132,
                    child: ListView.separated(
                      scrollDirection: Axis.horizontal,
                      padding: const EdgeInsets.symmetric(horizontal: 16),
                      itemCount: now.length,
                      separatorBuilder: (_, _) => const SizedBox(width: 10),
                      itemBuilder: (context, k) => RiseIn(
                        index: k,
                        child: SizedBox(
                          width: 268,
                          child: PaneCard(wall: w, pane: now[k], compact: true),
                        ),
                      ),
                    ),
                  ),
                ),
              ],
              for (final s in sections) ...[
                SliverPadding(
                  padding: const EdgeInsets.fromLTRB(16, 26, 16, 10),
                  sliver: SliverToBoxAdapter(child: _SectionHead(section: s, wall: w)),
                ),
                SliverPadding(
                  padding: const EdgeInsets.symmetric(horizontal: 16),
                  sliver: SliverList.list(
                    children: [
                      for (final row in s.rows)
                        if (row is String)
                          Padding(
                            padding: const EdgeInsets.fromLTRB(4, 6, 0, 8),
                            child: Text(
                              '› ${row.toUpperCase()}',
                              style: TdType.m(10.5, color: Td.inkAt(0.6), weight: FontWeight.w700, spacing: 1.8),
                            ),
                          )
                        else
                          Padding(
                            padding: const EdgeInsets.only(bottom: 10),
                            child: RiseIn(
                              index: i++,
                              child: PaneCard(wall: w, pane: w.pane(row as int)!),
                            ),
                          ),
                    ],
                  ),
                ),
              ],
              SliverToBoxAdapter(child: SizedBox(height: 110 + pad.bottom)),
            ],
          ),
        ),
        // The ground rising under the one button, so it never sits on a line
        // of somebody's card.
        Positioned(
          left: 0,
          right: 0,
          bottom: 0,
          height: 96 + pad.bottom,
          child: IgnorePointer(
            child: DecoratedBox(
              decoration: BoxDecoration(
                gradient: LinearGradient(
                  begin: Alignment.topCenter,
                  end: Alignment.bottomCenter,
                  colors: [Td.ground.withValues(alpha: 0), Td.ground.withValues(alpha: 0.94)],
                ),
              ),
            ),
          ),
        ),
        Positioned(
          left: 0,
          right: 0,
          bottom: 18 + pad.bottom,
          child: Center(
            child: Slab(
              label: 'New',
              icon: '+',
              onPressed: () => showNewSheet(context, wall: w),
            ),
          ),
        ),
      ],
    );
  }
}

class _Header extends StatelessWidget {
  const _Header({required this.wall, this.machine, this.sessions});
  final Wall wall;
  final String? machine;
  final int? sessions;

  @override
  Widget build(BuildContext context) => Column(
    crossAxisAlignment: CrossAxisAlignment.start,
    children: [
      const Row(
        children: [
          Expanded(
            child: Ignition(
              child: FittedBox(
                alignment: Alignment.centerLeft,
                fit: BoxFit.scaleDown,
                child: Wordmark(size: 19, stacked: false),
              ),
            ),
          ),
          SizedBox(width: 10),
          LinkPill(),
        ],
      ),
      const SizedBox(height: 8),
      Row(
        children: [
          Text(
            [
              (machine ?? '…').toUpperCase(),
              'SESSION ${wall.session}',
              '${wall.panes.where((p) => !p.ended).length} PANES',
            ].join('  ·  '),
            style: TdType.m(10.5, color: Td.inkAt(0.6), spacing: 1.4),
          ),
          const Spacer(),
          const _Menu(),
        ],
      ),
    ],
  );
}

class _Menu extends ConsumerWidget {
  const _Menu();

  @override
  Widget build(BuildContext context, WidgetRef ref) => PopupMenuButton<String>(
    color: Td.surface,
    icon: Icon(Icons.more_horiz, color: Td.inkAt(0.7), size: 20),
    padding: EdgeInsets.zero,
    onSelected: (v) async {
      switch (v) {
        case 'route':
          await ref.read(linkProvider.notifier).rediscover();
        case 'forget':
          await ref.read(pairingProvider.notifier).forget();
      }
    },
    itemBuilder: (context) => [
      PopupMenuItem(value: 'route', child: Text('Find the best route', style: TdType.m(13))),
      PopupMenuItem(
        value: 'forget',
        child: Text('Forget this laptop', style: TdType.m(13, color: Td.decision)),
      ),
    ],
  );
}

class _SectionHead extends StatelessWidget {
  const _SectionHead({required this.section, required this.wall});
  final _Section section;
  final Wall wall;

  @override
  Widget build(BuildContext context) {
    final c = section.color ?? Td.inkAt(0.5);
    final panes = section.rows.whereType<int>().map(wall.pane).nonNulls.toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        Row(
          children: [
            Container(
              width: 10,
              height: 10,
              decoration: BoxDecoration(
                color: c,
                boxShadow: [BoxShadow(color: c.withValues(alpha: 0.6), blurRadius: 8)],
              ),
            ),
            const SizedBox(width: 10),
            Flexible(
              child: Text(
                section.title.toUpperCase(),
                overflow: TextOverflow.ellipsis,
                style: TdType.m(13, color: Td.bright, weight: FontWeight.w700, spacing: 2.4),
              ),
            ),
            const SizedBox(width: 10),
            Expanded(child: Container(height: 1, color: c.withValues(alpha: 0.35))),
            const SizedBox(width: 10),
            // One dot per pane, in its state's colour: the section at a glance.
            for (final p in panes)
              Padding(
                padding: const EdgeInsets.only(left: 3),
                child: Container(
                  width: 5,
                  height: 5,
                  decoration: BoxDecoration(
                    color: p.state == PaneState.unknown ? Colors.transparent : p.state.tint,
                    border: Border.all(color: p.state == PaneState.unknown ? Td.inkAt(0.4) : p.state.tint, width: 0.8),
                    shape: BoxShape.circle,
                  ),
                ),
              ),
          ],
        ),
        if (section.hint != null) ...[
          const SizedBox(height: 6),
          Text(section.hint!, style: TdType.p(12.5, color: Td.inkAt(0.5))),
        ],
      ],
    );
  }
}

/// Every pane's state as one bar, widths by count — the whole desk in a line.
class _PulseStrip extends StatelessWidget {
  const _PulseStrip({required this.panes});
  final List<PaneView> panes;

  @override
  Widget build(BuildContext context) {
    final counts = <PaneState, int>{};
    for (final p in panes) {
      counts[p.state] = (counts[p.state] ?? 0) + 1;
    }
    final order = PaneState.values.where((s) => (counts[s] ?? 0) > 0).toList();
    return Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        ClipRRect(
          borderRadius: BorderRadius.circular(2),
          child: SizedBox(
            height: 5,
            child: Row(
              children: [
                for (final s in order)
                  Expanded(
                    flex: counts[s]!,
                    child: Container(
                      margin: const EdgeInsets.only(right: 2),
                      decoration: BoxDecoration(
                        color: s == PaneState.unknown || s == PaneState.ended
                            ? Td.faint
                            : s.tint.withValues(alpha: 0.9),
                        boxShadow: [BoxShadow(color: s.tint.withValues(alpha: 0.5), blurRadius: 6)],
                      ),
                    ),
                  ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 8),
        Wrap(
          spacing: 14,
          runSpacing: 4,
          children: [
            for (final s in order)
              Text.rich(
                TextSpan(
                  children: [
                    TextSpan(
                      text: '${counts[s]} ',
                      style: TdType.m(12, color: s == PaneState.unknown || s == PaneState.ended ? Td.inkAt(0.55) : s.tint, weight: FontWeight.w700),
                    ),
                    TextSpan(text: s.word, style: TdType.m(10, color: Td.inkAt(0.55), spacing: 1.2)),
                  ],
                ),
              ),
          ],
        ),
      ],
    );
  }
}

/// Looking for the laptop.
class Seeking extends StatelessWidget {
  const Seeking({super.key});

  @override
  Widget build(BuildContext context) => Center(
    child: Column(
      mainAxisSize: MainAxisSize.min,
      children: [
        const Ignition(child: Wordmark(size: 28)),
        const SizedBox(height: 26),
        SizedBox(
          width: 160,
          child: LinearProgressIndicator(
            minHeight: 2,
            color: Td.accent,
            backgroundColor: Td.faint.withValues(alpha: 0.6),
          ),
        ),
        const SizedBox(height: 14),
        Text('finding the desk', style: TdType.m(12, color: Td.inkAt(0.6), spacing: 1.2)),
      ],
    ),
  );
}

/// The laptop could not be reached, and why — each route by name.
class Trouble extends ConsumerWidget {
  const Trouble({required this.error, super.key});
  final Object error;

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    final e = error;
    final pad = MediaQuery.of(context).padding;
    return ListView(
      padding: EdgeInsets.fromLTRB(20, pad.top + 60, 20, 40),
      children: [
        Hud(
          tint: Td.decision,
          notch: Td.decision,
          glow: 0.4,
          padding: const EdgeInsets.fromLTRB(16, 16, 16, 18),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Text(
                e is Unreachable ? "CAN'T REACH ${e.machine.toUpperCase()}" : 'SOMETHING STOPPED',
                style: TdType.m(16, color: Td.bright, weight: FontWeight.w700, spacing: 1.5),
              ),
              const SizedBox(height: 12),
              if (e is Unreachable)
                for (final entry in e.tried.entries)
                  Padding(
                    padding: const EdgeInsets.only(bottom: 5),
                    child: Row(
                      children: [
                        Expanded(child: Text(entry.key, style: TdType.m(12))),
                        Text(entry.value, style: TdType.m(12, color: Td.decision)),
                      ],
                    ),
                  )
              else
                Text('$e', style: TdType.p(15)),
              const SizedBox(height: 12),
              Text(
                'Plugged in: run td-mobile-gateway pair on the laptop again. '
                'Away from the desk: switch Tailscale on here. Either way, the gateway must be running.',
                style: TdType.p(14, color: Td.inkAt(0.75)),
              ),
              const SizedBox(height: 18),
              Row(
                children: [
                  Expanded(
                    child: Slab(
                      label: 'Try again',
                      icon: '↻',
                      tint: Td.decision,
                      onPressed: () {
                        ref
                          ..invalidate(helloProvider)
                          ..invalidate(chosenSessionProvider);
                        unawaited(ref.read(linkProvider.notifier).rediscover());
                      },
                    ),
                  ),
                  const SizedBox(width: 10),
                  Ghost(
                    label: 'Unpair',
                    tint: Td.inkAt(0.7),
                    onPressed: () => ref.read(pairingProvider.notifier).forget(),
                  ),
                ],
              ),
            ],
          ),
        ),
      ],
    );
  }
}

/// Opening a pane from anywhere.
void openPane(BuildContext context, String session, int pane, {bool terminal = false}) {
  unawaited(context.push('/pane/$session/$pane${terminal ? '?face=terminal' : ''}'));
}
