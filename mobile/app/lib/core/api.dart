import 'dart:async';
import 'dart:convert';
import 'dart:io';

import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/link.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/pairing.dart';

/// A refusal from the gateway, carrying its own sentence.
class Refused implements Exception {
  const Refused(this.status, this.why);
  final int? status;
  final String why;
  @override
  String toString() => why;
}

class Api {
  Api(this.link, this.token)
    : _dio = Dio(
        BaseOptions(
          baseUrl: link.base,
          connectTimeout: const Duration(seconds: 6),
          receiveTimeout: const Duration(seconds: 20),
          headers: {'Authorization': 'Bearer $token'},
        ),
      );

  final Link link;
  final String token;
  final Dio _dio;

  Future<Json> _get(String path) async {
    try {
      final r = await _dio.get<Map<String, dynamic>>(path);
      return r.data ?? const <String, dynamic>{};
    } on DioException catch (e) {
      throw _refusal(e);
    }
  }

  Refused _refusal(DioException e) {
    final body = jMap(e.response?.data);
    return Refused(
      e.response?.statusCode,
      jStr(body?['error']) ??
          switch (e.type) {
            DioExceptionType.connectionError => 'The laptop stopped answering.',
            DioExceptionType.connectionTimeout ||
            DioExceptionType.receiveTimeout => 'The laptop is taking too long.',
            _ => e.message ?? 'Something went wrong on the way.',
          },
    );
  }

  Future<Hello> hello() async => Hello.from(await _get('/v0/hello'));

  Future<Wall> wall(String session) async =>
      Wall.from(await _get('/v0/sessions/$session/wall'));

  Future<Bench> bench(String session, int pane) async =>
      Bench.from(await _get('/v0/sessions/$session/panes/$pane/bench'));

  /// Start a terminal on the laptop's session host. Answers the pane.
  Future<int> spawn(
    String session, {
    required int cols,
    required int rows,
    String? cwd,
    String? agent,
    String? model,
  }) async {
    try {
      final r = await _dio.post<Map<String, dynamic>>(
        '/v0/sessions/$session/spawn',
        data: {
          'cwd': ?cwd,
          'agent': ?agent,
          'model': ?model,
          'cols': cols,
          'rows': rows,
        },
      );
      final outcome = jMap(r.data?['outcome']);
      final err = jStr(outcome?['err']);
      if (err != null) throw Refused(null, err);
      final pane = jInt(jMap(outcome?['ok'])?['pane']);
      if (pane == null) throw const Refused(null, 'The host started nothing.');
      return pane;
    } on DioException catch (e) {
      throw _refusal(e);
    }
  }

  /// Open a WebSocket on the gateway, authenticated the same way.
  Future<WebSocket> socket(String path, [Map<String, String>? query]) {
    final base = Uri.parse(link.base);
    final uri = base.replace(
      scheme: base.scheme == 'https' ? 'wss' : 'ws',
      path: path,
      queryParameters: query,
    );
    return WebSocket.connect(
      uri.toString(),
      headers: {'Authorization': 'Bearer $token'},
    ).timeout(const Duration(seconds: 8));
  }
}

final apiProvider = Provider<Api?>((ref) {
  final link = ref.watch(linkProvider).valueOrNull;
  final pairing = ref.watch(pairingProvider);
  if (link == null || pairing == null) return null;
  return Api(link, pairing.token);
});

Future<Api> _api(Ref ref) async {
  final api = ref.watch(apiProvider);
  if (api != null) return api;
  await ref.watch(linkProvider.future);
  final again = ref.read(apiProvider);
  if (again == null) throw const Refused(null, 'Not paired.');
  return again;
}

final helloProvider = FutureProvider<Hello>((ref) async {
  return (await _api(ref)).hello();
});

/// The session the phone is looking at: the one the person picked, else the
/// host with the most panes.
final chosenSessionProvider = StateProvider<String?>((_) => null);

final sessionProvider = FutureProvider<String>((ref) async {
  final chosen = ref.watch(chosenSessionProvider);
  if (chosen != null) return chosen;
  final hello = await ref.watch(helloProvider.future);
  if (hello.sessions.isEmpty) {
    throw Refused(null, 'No Terminal Delight session is running on ${hello.machine}.');
  }
  return hello.sessions.first.key;
});

final wallProvider = FutureProvider.family<Wall, String>((ref, session) async {
  return (await _api(ref)).wall(session);
});

final benchProvider = FutureProvider.autoDispose
    .family<Bench, (String, int)>((ref, key) async {
      return (await _api(ref)).bench(key.$1, key.$2);
    });

/// The gateway's live hints, turned into refreshes. Held open while anything
/// on screen is watching it; reconnects with a short backoff.
class Events {
  Events(this.ref);
  final Ref ref;
  WebSocket? _ws;
  Timer? _retry;
  Timer? _wallDebounce;
  final _benchDebounce = <int, Timer>{};
  bool _closed = false;
  int _backoff = 1;

  /// Whether the live feed is open — the dot on the link pill.
  final live = StreamController<bool>.broadcast();
  bool isLive = false;

  Future<void> start() async {
    if (_closed) return;
    final api = ref.read(apiProvider);
    if (api == null) {
      _schedule();
      return;
    }
    try {
      final ws = await api.socket('/v0/events');
      ws.pingInterval = const Duration(seconds: 15);
      _ws = ws;
      _backoff = 1;
      _set(live: true);
      ws.listen(
        _onMessage,
        onDone: _lost,
        onError: (Object _) => _lost(),
        cancelOnError: true,
      );
    } on Object {
      _lost();
    }
  }

  void _set({required bool live}) {
    isLive = live;
    if (!this.live.isClosed) this.live.add(live);
  }

  void _lost() {
    _ws = null;
    _set(live: false);
    _schedule();
  }

  void _schedule() {
    if (_closed) return;
    _retry?.cancel();
    _retry = Timer(Duration(seconds: _backoff), start);
    _backoff = (_backoff * 2).clamp(1, 16);
  }

  void _onMessage(dynamic data) {
    if (data is! String) return;
    final m = jMap(jsonDecode(data));
    if (m == null) return;
    final session = jStr(m['session']);
    switch (jStr(m['type'])) {
      case 'mailbox':
        final pane = jInt(m['pane']);
        if (session != null) _wall(session);
        if (session != null && pane != null) _bench(session, pane);
      case 'panes':
        // Who holds a pane's stream changed: every open bench's "is this on a
        // screen" is now stale, not just the wall's.
        if (session != null) _wall(session);
        ref.invalidate(benchProvider);
      case 'tree':
        if (session != null) _wall(session);
      case 'sessions' || 'resync':
        ref.invalidate(helloProvider);
        final s = ref.read(sessionProvider).valueOrNull;
        if (s != null) _wall(s);
    }
  }

  void _wall(String session) {
    _wallDebounce?.cancel();
    _wallDebounce = Timer(
      const Duration(milliseconds: 250),
      () => ref.invalidate(wallProvider(session)),
    );
  }

  void _bench(String session, int pane) {
    _benchDebounce[pane]?.cancel();
    _benchDebounce[pane] = Timer(
      const Duration(milliseconds: 250),
      () => ref.invalidate(benchProvider((session, pane))),
    );
  }

  void close() {
    _closed = true;
    _retry?.cancel();
    _wallDebounce?.cancel();
    for (final t in _benchDebounce.values) {
      t.cancel();
    }
    unawaited(_ws?.close());
    unawaited(live.close());
  }
}

final eventsProvider = Provider<Events>((ref) {
  // Re-made whenever the route changes, so the feed follows the link.
  ref.watch(apiProvider);
  final e = Events(ref);
  unawaited(e.start());
  ref.onDispose(e.close);
  return e;
});

final liveProvider = StreamProvider<bool>((ref) {
  final e = ref.watch(eventsProvider);
  return e.live.stream;
});
