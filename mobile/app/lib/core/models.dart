import 'package:flutter/painting.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/json.dart';

/// What a pane is doing, as the gateway read it from the pane's channel
/// records. `unknown` is its own answer: a pane with no hooked agent has
/// nothing to read, and calling it idle would be a guess.
enum PaneState {
  needsYou('NEEDS YOU', Td.decision),
  working('WORKING', Td.pending),
  idle('DONE', Td.settled),
  unknown('UNKNOWN', Td.faint),
  ended('ENDED', Td.faint);

  const PaneState(this.word, this.tint);
  final String word;
  final Color tint;

  static PaneState parse(String? s) => switch (s) {
    'needs-you' => needsYou,
    'working' => working,
    'idle' => idle,
    'ended' => ended,
    _ => unknown,
  };

  /// How loudly a wall should put this pane in front of a person.
  int get urgency => switch (this) {
    needsYou => 0,
    working => 1,
    idle => 2,
    unknown => 3,
    ended => 4,
  };
}

/// Something somebody said, and when.
class Said {
  const Said(this.text, this.atMs, {this.harness = false});
  final String? text;
  final int? atMs;

  /// Submitted by the agent's harness, not typed by a person — a background
  /// task reporting back, a slash command's expansion. Drawn, never as "you".
  final bool harness;

  static Said? from(Object? v) {
    final m = jMap(v);
    if (m == null) return null;
    return Said(jText(m['text']), jInt(m['at_ms']));
  }
}

class Pulse {
  const Pulse({
    required this.state,
    required this.why,
    this.prompt,
    this.reply,
    this.activityMs,
  });

  factory Pulse.from(Object? v) {
    final m = jMap(v) ?? const <String, dynamic>{};
    return Pulse(
      state: PaneState.parse(jStr(m['state'])),
      why: jText(m['why']),
      prompt: Said.from(m['prompt']),
      reply: Said.from(m['reply']),
      activityMs: jInt(m['activity_ms']),
    );
  }

  final PaneState state;
  final String? why;
  final Said? prompt;
  final Said? reply;
  final int? activityMs;
}

/// What is in the foreground, as the host's watcher classified it. `null` kind
/// means the watcher has not looked yet.
class PaneMode {
  const PaneMode(this.kind, [this.program]);

  factory PaneMode.from(Object? v) {
    if (v is String) return PaneMode(v);
    final m = jMap(v);
    if (m != null && m['other'] is String) {
      return PaneMode('other', m['other'] as String);
    }
    return const PaneMode(null);
  }

  final String? kind;
  final String? program;

  bool get isAgent => kind == 'claude' || kind == 'codex';

  String get label => switch (kind) {
    'claude' => 'claude',
    'codex' => 'codex',
    'shell' => 'shell',
    'remote' => 'remote',
    'other' => program ?? 'program',
    _ => 'not yet read',
  };

  String get glyph => switch (kind) {
    'claude' => '✳',
    'codex' => '◎',
    'shell' => r'$',
    'remote' => '⇄',
    'other' => '▣',
    _ => '·',
  };

  Color get tint => switch (kind) {
    'claude' => Td.cabinet,
    'codex' => Td.ident,
    'shell' => Td.ink,
    'remote' => Td.human,
    'other' => Td.pending,
    _ => Td.faint,
  };
}

/// The newest response card a pane's agent presented, cut down for a wall.
class Latest {
  const Latest({this.title, this.brief, this.layman, this.mtimeMs, this.escalation});

  static Latest? from(Object? v) {
    final m = jMap(v);
    if (m == null) return null;
    return Latest(
      title: jText(m['title']),
      brief: jText(m['brief']),
      layman: jText(m['layman']),
      mtimeMs: jInt(m['mtime_ms']),
      escalation: jMap(m['escalation']),
    );
  }

  final String? title;
  final String? brief;
  final String? layman;
  final int? mtimeMs;
  final Json? escalation;
}

class PaneView {
  const PaneView({
    required this.pane,
    required this.mode,
    required this.attached,
    required this.ended,
    required this.pulse,
    this.cwd,
    this.resume,
    this.cols,
    this.rows,
    this.latest,
    this.surfaces,
  });

  factory PaneView.from(Json m) {
    final geom = jMap(m['geom']);
    return PaneView(
      pane: jInt(m['pane']) ?? -1,
      mode: PaneMode.from(m['mode']),
      attached: jBool(m['attached']) ?? false,
      ended: jBool(m['ended']) ?? false,
      pulse: Pulse.from(m['pulse']),
      cwd: jText(m['cwd']),
      resume: jText(m['resume']),
      cols: jInt(geom?['cols']),
      rows: jInt(geom?['rows']),
      latest: Latest.from(m['latest']),
      surfaces: jInt(m['surfaces']),
    );
  }

  final int pane;
  final PaneMode mode;

  /// A window holds this pane's byte stream. Taking it freezes the desk's copy.
  final bool attached;
  final bool ended;
  final Pulse pulse;
  final String? cwd;
  final String? resume;
  final int? cols;
  final int? rows;
  final Latest? latest;
  final int? surfaces;

  PaneState get state => ended ? PaneState.ended : pulse.state;
}

class Note {
  const Note(this.text, {this.pinned});
  final String text;
  final bool? pinned;
}

class Leaf {
  const Leaf({required this.pane, this.name, this.note});
  final int pane;
  final String? name;
  final Note? note;
}

class DeskTab {
  const DeskTab({
    required this.index,
    required this.leaves,
    this.name,
    this.group,
    this.project,
    this.active = false,
  });
  final int index;
  final String? name;
  final int? group;
  final int? project;
  final bool active;
  final List<Leaf> leaves;
}

class Shelf {
  const Shelf({required this.id, this.name, this.color, this.project});
  final int id;
  final String? name;
  final Color? color;

  /// For a group: the project it sits in, when it sits in one.
  final int? project;
}

/// The desk's left bar: project › group › tab, in the session file's order.
class DeskTree {
  const DeskTree({
    required this.available,
    this.projects = const [],
    this.groups = const [],
    this.tabs = const [],
  });

  factory DeskTree.from(Object? v) {
    final m = jMap(v);
    if (m == null || jBool(m['available']) != true) {
      return const DeskTree(available: false);
    }
    Shelf shelf(Object? o) {
      final s = jMap(o) ?? const <String, dynamic>{};
      return Shelf(
        id: jInt(s['id']) ?? -1,
        name: jText(s['name']),
        color: Td.parse(jStr(s['color'])),
        project: jInt(s['project']),
      );
    }

    return DeskTree(
      available: true,
      projects: jList(m['projects']).map(shelf).toList(),
      groups: jList(m['groups']).map(shelf).toList(),
      tabs: jList(m['tabs']).map((o) {
        final t = jMap(o) ?? const <String, dynamic>{};
        return DeskTab(
          index: jInt(t['index']) ?? 0,
          name: jText(t['name']),
          group: jInt(t['group']),
          project: jInt(t['project']),
          active: jBool(t['active']) ?? false,
          leaves: jList(t['leaves']).map((l) {
            final lm = jMap(l) ?? const <String, dynamic>{};
            final note = jMap(lm['note']);
            final noteText = jText(note?['text']);
            return Leaf(
              pane: jInt(lm['pane']) ?? -1,
              name: jText(lm['name']),
              note: noteText == null
                  ? null
                  : Note(noteText, pinned: jBool(note?['pinned'])),
            );
          }).toList(),
        );
      }).toList(),
    );
  }

  final bool available;
  final List<Shelf> projects;
  final List<Shelf> groups;
  final List<DeskTab> tabs;
}

class Wall {
  const Wall({
    required this.session,
    required this.tree,
    required this.panes,
    required this.skewMs,
  });

  factory Wall.from(Json m) {
    final server = jInt(m['server_ms']);
    return Wall(
      session: jStr(m['session']) ?? '?',
      tree: DeskTree.from(m['tree']),
      panes: jList(m['panes'])
          .map((p) => PaneView.from(jMap(p) ?? const <String, dynamic>{}))
          .where((p) => p.pane >= 0)
          .toList(),
      skewMs: server == null
          ? 0
          : server - DateTime.now().millisecondsSinceEpoch,
    );
  }

  final String session;
  final DeskTree tree;
  final List<PaneView> panes;

  /// The laptop's clock minus the phone's, so "five minutes ago" is measured
  /// on one clock.
  final int skewMs;

  PaneView? pane(int id) {
    for (final p in panes) {
      if (p.pane == id) return p;
    }
    return null;
  }

  /// Where a pane sits on the desk: its tab and its leaf, or null for a pane
  /// the desk's layout does not claim (started from the phone, or orphaned).
  (DeskTab, Leaf)? place(int pane) {
    for (final t in tree.tabs) {
      for (final l in t.leaves) {
        if (l.pane == pane) return (t, l);
      }
    }
    return null;
  }

  Shelf? project(int? id) {
    if (id == null) return null;
    for (final p in tree.projects) {
      if (p.id == id) return p;
    }
    return null;
  }

  Shelf? group(int? id) {
    if (id == null) return null;
    for (final g in tree.groups) {
      if (g.id == id) return g;
    }
    return null;
  }

  /// The colour the desk would show for this pane's tab: its group's, else
  /// its project's, else its group's project's.
  Color? colourOf(int pane) {
    final place = this.place(pane);
    if (place == null) return null;
    final tab = place.$1;
    final g = group(tab.group);
    return g?.color ?? project(tab.project)?.color ?? project(g?.project)?.color;
  }

  String titleOf(int pane) {
    final place = this.place(pane);
    if (place == null) {
      // Not on the desk: named by where it is, since nobody has named it.
      final dir = this.pane(pane)?.cwd?.split('/').where((s) => s.isNotEmpty).lastOrNull;
      return dir == null ? 'pane $pane' : '$dir · $pane';
    }
    final (tab, leaf) = place;
    final base = leaf.name ?? tab.name ?? 'tab ${tab.index + 1}';
    if (tab.leaves.length > 1) {
      final i = tab.leaves.indexWhere((l) => l.pane == pane);
      return '$base · ${i + 1}/${tab.leaves.length}';
    }
    return base;
  }
}

class Surface {
  const Surface({
    required this.id,
    required this.doc,
    this.file,
    this.mtimeMs,
  });

  factory Surface.from(Json m) => Surface(
    id: jStr(m['id']) ?? '?',
    file: jStr(m['file']),
    mtimeMs: jInt(m['mtime_ms']),
    doc: jMap(m['doc']) ?? const <String, dynamic>{},
  );

  final String id;
  final String? file;
  final int? mtimeMs;
  final Json doc;

  String get kind => jStr(doc['kind']) ?? 'unclassified';
  String? get title => jText(doc['title']);
  Json get model => jMap(doc['model']) ?? const <String, dynamic>{};
  Json? get weight => jMap(doc['weight']);
  Json? get source => jMap(doc['source']);
}

/// One line of the channel: inbound from the agent's hooks, or outbound from
/// the window.
class Record {
  const Record(this.raw);
  final Json raw;
  String? get type => jStr(raw['type']);
  String? get side => jStr(raw['side']);
  int? get atMs => jInt(raw['at_ms']);
  String? get text => jText(raw['text']);
  bool get harness => jBool(raw['harness']) ?? false;
}

class Bench {
  const Bench({
    required this.session,
    required this.pane,
    required this.pulse,
    required this.surfaces,
    required this.timeline,
    required this.skewMs,
    this.info,
  });

  factory Bench.from(Json m) {
    final server = jInt(m['server_ms']);
    final info = jMap(m['info']);
    return Bench(
      session: jStr(m['session']) ?? '?',
      pane: jInt(m['pane']) ?? -1,
      info: info == null ? null : PaneView.from({...info, 'pulse': m['pulse']}),
      pulse: Pulse.from(m['pulse']),
      surfaces: jList(m['surfaces'])
          .map((s) => Surface.from(jMap(s) ?? const <String, dynamic>{}))
          .toList(),
      timeline: jList(m['timeline'])
          .map((r) => Record(jMap(r) ?? const <String, dynamic>{}))
          .toList(),
      skewMs: server == null
          ? 0
          : server - DateTime.now().millisecondsSinceEpoch,
    );
  }

  final String session;
  final int pane;

  /// The host's row for this pane; null when the host no longer lists it.
  final PaneView? info;
  final Pulse pulse;
  final List<Surface> surfaces;
  final List<Record> timeline;
  final int skewMs;
}

class SessionInfo {
  const SessionInfo({required this.key, required this.panes, this.attended});
  final String key;
  final int panes;
  final bool? attended;
}

class Hello {
  const Hello({required this.machine, required this.sessions, this.tailnet});

  factory Hello.from(Json m) => Hello(
    machine: jStr(m['machine']) ?? 'this machine',
    tailnet: jText(m['tailnet']),
    sessions: jList(m['sessions']).map((s) {
      final sm = jMap(s) ?? const <String, dynamic>{};
      return SessionInfo(
        key: jStr(sm['key']) ?? '?',
        panes: jInt(sm['panes']) ?? 0,
        attended: jBool(sm['attended']),
      );
    }).toList(),
  );

  final String machine;
  final String? tailnet;
  final List<SessionInfo> sessions;
}

/// "4m", "2h", "3d" — or null when there is nothing to measure from.
String? ago(int? atMs, int skewMs) {
  if (atMs == null) return null;
  final now = DateTime.now().millisecondsSinceEpoch + skewMs;
  final s = ((now - atMs) / 1000).floor();
  if (s < 45) return 'now';
  if (s < 3600) return '${(s / 60).round()}m';
  if (s < 86400) return '${(s / 3600).floor()}h';
  return '${(s / 86400).floor()}d';
}

/// `/home/parker/Work/x` → `~/Work/x`.
String tidyPath(String? p) {
  if (p == null) return '—';
  const home = '/home/';
  if (p.startsWith(home)) {
    final rest = p.substring(home.length);
    final slash = rest.indexOf('/');
    return slash < 0 ? '~' : '~${rest.substring(slash)}';
  }
  return p;
}
