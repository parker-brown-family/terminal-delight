import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/core/json.dart';
import 'package:terminal_delight/core/pairing.dart';

/// The route the phone is using right now, and how far away the laptop is.
class Link {
  const Link({required this.route, required this.base, required this.rttMs});
  final String route;
  final String base;
  final int rttMs;
}

/// Why no route answered — each one, by name, so the screen can say which.
class Unreachable implements Exception {
  const Unreachable(this.machine, this.tried);
  final String machine;
  final Map<String, String> tried;
  @override
  String toString() => 'Could not reach $machine';
}

/// Probe one route's `/v0/ping` and check it is OUR machine answering, not
/// merely something answering. Null when it is not.
Future<(int, String?)?> probe(NetRoute r, String machine) async {
  final dio = Dio(
    BaseOptions(
      connectTimeout: const Duration(milliseconds: 1600),
      receiveTimeout: const Duration(milliseconds: 1600),
    ),
  );
  final t = Stopwatch()..start();
  try {
    final res = await dio.get<Map<String, dynamic>>('${r.base}/v0/ping');
    final d = res.data ?? const <String, dynamic>{};
    if (jStr(d['gateway']) != 'terminal-delight') return (0, 'not a gateway');
    if (jStr(d['machine']) != machine) {
      return (0, 'answered as ${jStr(d['machine'])}');
    }
    return (t.elapsedMilliseconds, null);
  } on DioException catch (e) {
    return (
      0,
      switch (e.type) {
        DioExceptionType.connectionTimeout => 'no answer',
        DioExceptionType.connectionError => 'refused',
        _ => e.message ?? 'failed',
      },
    );
  } finally {
    dio.close(force: true);
  }
}

class LinkNotifier extends AsyncNotifier<Link?> {
  @override
  Future<Link?> build() async {
    final pairing = ref.watch(pairingProvider);
    if (pairing == null) return null;
    final routes = pairing.routes;
    // All at once; the first in preference order that answers wins, so a
    // plugged-in phone uses the cable even when the tailnet answered first.
    final results = await Future.wait(
      routes.map((r) => probe(r, pairing.machine)),
    );
    final tried = <String, String>{};
    for (var i = 0; i < routes.length; i++) {
      final res = results[i];
      if (res != null && res.$2 == null) {
        return Link(route: routes[i].name, base: routes[i].base, rttMs: res.$1);
      }
      tried['${routes[i].name} ${Uri.parse(routes[i].base).host}'] =
          res?.$2 ?? 'failed';
    }
    throw Unreachable(pairing.machine, tried);
  }

  Future<void> rediscover() async {
    state = const AsyncLoading<Link?>().copyWithPrevious(state);
    ref.invalidateSelf();
    await future;
  }
}

final linkProvider = AsyncNotifierProvider<LinkNotifier, Link?>(
  LinkNotifier.new,
);
