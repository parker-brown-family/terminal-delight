import 'package:flutter_test/flutter_test.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/features/bench/workbench_face.dart';

// Synthetic, in the gateway's shapes. The live wall carries a person's real
// prompts, so it is never a fixture.
const _wall = <String, dynamic>{
  'session': '1',
  'server_ms': 1790000000000,
  'tree': {
    'available': true,
    'projects': [
      {'id': 12, 'name': 'TERMINAL DELIGHT', 'color': '#5cc1cc'},
    ],
    'groups': [
      {'id': 4, 'name': 'BFS', 'color': '#7c5430', 'project': null},
    ],
    'tabs': [
      {
        'index': 0,
        'name': 'Research',
        'project': 12,
        'leaves': [
          {'pane': 97, 'note': null},
          {'pane': 131, 'note': {'text': 'watch this', 'pinned': false}},
        ],
      },
      {
        'index': 1,
        'name': 'MONETIZATION',
        'group': 4,
        'leaves': [
          {'pane': 2, 'name': 'money'},
        ],
      },
    ],
  },
  'panes': [
    {
      'pane': 97,
      'mode': 'claude',
      'attached': true,
      'ended': false,
      'cwd': '/home/parker/Work/terminal-delight',
      'geom': {'cols': 90, 'rows': 22},
      'pulse': {'state': 'working', 'why': 'on the turn you gave it'},
    },
    {
      'pane': 131,
      'mode': {'other': 'vim'},
      'attached': false,
      'ended': false,
      'geom': {'cols': 90, 'rows': 22},
      'pulse': {'state': 'something-new'},
    },
    {
      'pane': 2,
      'mode': null,
      'attached': true,
      'ended': true,
      'geom': {'cols': 90, 'rows': 22},
      'pulse': {'state': 'idle'},
    },
    {
      'pane': 140,
      'mode': 'shell',
      'attached': false,
      'ended': false,
      'geom': {'cols': 50, 'rows': 40},
      'pulse': {'state': 'unknown'},
    },
  ],
};

void main() {
  group('wall', () {
    final w = Wall.from(_wall);

    test('reads every pane, and absent stays absent', () {
      expect(w.panes, hasLength(4));
      expect(w.pane(131)!.mode.kind, 'other');
      expect(w.pane(131)!.mode.program, 'vim');
      expect(w.pane(2)!.mode.kind, isNull, reason: 'not yet read is not a shell');
      expect(w.pane(140)!.cwd, isNull);
    });

    test('a state this build does not know reads as unknown, never idle', () {
      expect(w.pane(131)!.state, PaneState.unknown);
    });

    test('an ended pane is ended whatever its channel last said', () {
      expect(w.pane(2)!.state, PaneState.ended);
    });

    test('a pane is titled by its tab, and a split says which leaf', () {
      expect(w.titleOf(97), 'Research · 1/2');
      expect(w.titleOf(131), 'Research · 2/2');
      expect(w.titleOf(2), 'money');
      expect(w.titleOf(140), 'pane 140', reason: 'not on the desk');
    });

    test('colour comes from the group, else the project', () {
      expect(w.colourOf(2)!.toARGB32(), 0xFF7C5430);
      expect(w.colourOf(97)!.toARGB32(), 0xFF5CC1CC);
      expect(w.colourOf(140), isNull);
    });

    test('a sticky note travels with its leaf', () {
      expect(w.place(131)!.$2.note!.text, 'watch this');
      expect(w.place(97)!.$2.note, isNull);
    });
  });

  group('pairing link', () {
    final token = 'a' * 64;

    test('reads every route the gateway offered, cable first', () {
      final p = Pairing.fromUri(
        Uri.parse(
          'tdmobile://pair?token=$token&machine=legion&port=7717'
          '&tailnet_ip=100.1.2.3&lan=192.168.1.5&lan=192.168.1.6',
        ),
      )!;
      expect(p.machine, 'legion');
      expect(p.routes.map((r) => r.name), ['USB', 'TAILSCALE', 'WI-FI', 'WI-FI']);
      expect(p.routes.first.base, 'http://127.0.0.1:7717');
    });

    test('refuses a link that is not ours, or a short token', () {
      expect(Pairing.fromUri(Uri.parse('https://pair?token=$token')), isNull);
      expect(Pairing.fromUri(Uri.parse('tdmobile://pair?token=abc')), isNull);
      expect(Pairing.fromUri(Uri.parse('tdmobile://other?token=$token')), isNull);
    });

    test('round-trips through storage', () {
      final p = Pairing.fromUri(Uri.parse('tdmobile://pair?token=$token&machine=legion'))!;
      final back = Pairing.fromJson(p.toJson());
      expect(back.token, token);
      expect(back.tailnetIp, isNull);
    });
  });

  group('turns', () {
    test('a surface belongs to the turn whose prompt came before it', () {
      final b = Bench.from({
        'session': '1',
        'pane': 97,
        'pulse': {'state': 'idle'},
        'surfaces': [
          {'id': 'a', 'mtime_ms': 150, 'doc': {'kind': 'response', 'title': 'first answer'}},
          {'id': 'b', 'mtime_ms': 350, 'doc': {'kind': 'response', 'title': 'second answer'}},
          {'id': 'c', 'mtime_ms': 360, 'doc': {'kind': 'decision', 'title': 'not a turn'}},
        ],
        'timeline': [
          {'side': 'in', 'type': 'prompt', 'at_ms': 100, 'text': 'one'},
          {'side': 'in', 'type': 'reply', 'at_ms': 200, 'text': 'said one'},
          {'side': 'in', 'type': 'prompt', 'at_ms': 300, 'text': 'two'},
          {'side': 'out', 'type': 'say', 'at_ms': 299, 'text': 'two'},
        ],
      });
      final turns = turnsOf(b);
      expect(turns, hasLength(2));
      expect(turns[0].prompt!.text, 'one');
      expect(turns[0].responses.single.title, 'first answer');
      expect(turns[0].reply!.text, 'said one');
      expect(turns[1].prompt!.text, 'two');
      expect(turns[1].responses.single.title, 'second answer');
    });
  });

  group('a td fence in a reply', () {
    test('becomes a card, and the prose around it stays the reply', () {
      const reply = "I'm running on **legion**.\n\n```td\n"
          '{"td":"0.4","kind":"response","title":"Machine name","model":{"layman":"legion"}}\n```';
      final (rest, docs) = liftFences(reply);
      expect(rest.trim(), "I'm running on **legion**.");
      expect(docs.single['title'], 'Machine name');
    });

    test('a fence that is not a document stays in the prose', () {
      const reply = '```td\nnot json\n```';
      final (rest, docs) = liftFences(reply);
      expect(docs, isEmpty);
      expect(rest, reply);
    });

    test('lands in its turn as a card instead of a reply', () {
      final b = Bench.from({
        'session': '1',
        'pane': 133,
        'pulse': {'state': 'idle'},
        'surfaces': <Object>[],
        'timeline': [
          {'side': 'in', 'type': 'prompt', 'at_ms': 100, 'text': 'which machine'},
          {
            'side': 'in',
            'type': 'reply',
            'at_ms': 200,
            'text': '```td\n{"kind":"response","title":"legion","model":{"layman":"legion"}}\n```',
          },
        ],
      });
      final t = turnsOf(b).last;
      expect(t.responses.single.title, 'legion');
      expect(t.reply, isNull, reason: 'nothing was left but the fence');
    });
  });

  test('ago says nothing when there is nothing to measure', () {
    expect(ago(null, 0), isNull);
    expect(ago(DateTime.now().millisecondsSinceEpoch - 5 * 60000, 0), '5m');
  });

  test('tidyPath folds home', () {
    expect(tidyPath('/home/parker/Work/x'), '~/Work/x');
    expect(tidyPath('/home/parker'), '~');
    expect(tidyPath(null), '—');
  });
}
