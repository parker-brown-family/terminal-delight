import 'dart:async';

import 'package:dio/dio.dart';
import 'package:flutter/services.dart';
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

/// What this phone can see of its own end of each route. Each is null when
/// the phone would not say, which is not the same as "no".
class Here {
  const Here({this.cable, this.adb, this.tailnet, this.wifi});

  /// A computer is on the other end of the USB cable, not merely power.
  final bool? cable;

  /// USB debugging is on, which the cable route rides on.
  final bool? adb;

  /// Tailscale is up on this phone and holds a tailnet address.
  final bool? tailnet;

  /// The phone is on a Wi-Fi network at all.
  final bool? wifi;
}

const _here = MethodChannel('td/here');

Future<Here> readHere() async {
  try {
    final m = await _here.invokeMapMethod<String, bool>('here') ?? const {};
    return Here(cable: m['cable'], adb: m['adb'], tailnet: m['tailnet'], wifi: m['wifi']);
  } on Object {
    return const Here();
  }
}

/// Why no route answered — each one, by name, so the screen can say which.
class Unreachable implements Exception {
  const Unreachable(this.machine, this.tried, [this.here = const Here()]);
  final String machine;
  final Map<String, String> tried;
  final Here here;
  @override
  String toString() => 'Could not reach $machine';
}

/// A failed route's reason, sharpened by what the phone sees of its own end:
/// a cable that carries no computer says so, rather than "refused", and a
/// tailnet that is switched off here says so, rather than "no answer".
String whyNot(String route, String probed, Here here) => switch (route) {
  'USB' when here.cable == false => 'not plugged in',
  'USB' when here.adb == false => 'USB debugging off',
  'TAILSCALE' when here.tailnet == false => 'Tailscale off here',
  'WI-FI' when here.wifi == false => 'not on Wi-Fi',
  _ => probed,
};

/// What the person can do about it, one line per route they can mend, in the
/// routes' order of preference. Built from what the phone saw, so it never
/// sends anyone to the laptop for something the phone already knows.
List<String> advice(Unreachable e) {
  final h = e.here;
  final m = e.machine;
  final tailnet = e.tried.keys.any((k) => k.startsWith('TAILSCALE'));
  return [
    switch (h.cable) {
      false =>
        'This phone sees no computer on its USB port. Plug it into $m; if it is '
            'plugged in, that cable or port is carrying charge only.',
      true when h.adb == false => 'The cable is in, but USB debugging is off. Turn it on in Developer options.',
      true =>
        'The cable is in, but $m is not answering on it. The gateway puts this route back '
            'a few seconds after a replug; if it stays like this, check the gateway is running on $m.',
      null => 'Plugged in: the gateway must be running on $m. It puts the cable route back by itself.',
    },
    if (tailnet)
      switch (h.tailnet) {
        false =>
          'Away from the desk: switch Tailscale on here. Android keeps it on if you '
              'turn on Always-on VPN for Tailscale.',
        true => 'Tailscale is on here, but $m did not answer on it. Is $m awake and on the tailnet?',
        null => 'Away from the desk: Tailscale must be on, here and on $m.',
      },
  ];
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
    return _find(pairing);
  }

  Future<Link> _find(Pairing pairing) async {
    final routes = pairing.routes;
    // All at once; the first in preference order that answers wins, so a
    // plugged-in phone uses the cable even when the tailnet answered first.
    final results = await Future.wait(
      routes.map((r) => probe(r, pairing.machine)),
    );
    for (var i = 0; i < routes.length; i++) {
      final res = results[i];
      if (res != null && res.$2 == null) {
        return Link(route: routes[i].name, base: routes[i].base, rttMs: res.$1);
      }
    }
    final here = await readHere();
    throw Unreachable(pairing.machine, {
      for (var i = 0; i < routes.length; i++)
        '${routes[i].name} ${Uri.parse(routes[i].base).host}': whyNot(routes[i].name, results[i]?.$2 ?? 'failed', here),
    }, here);
  }

  Future<void> rediscover() async {
    state = const AsyncLoading<Link?>().copyWithPrevious(state);
    ref.invalidateSelf();
    await future;
  }

  /// Look again without passing through a loading state, so the trouble
  /// screen stays up and its rows change in place as routes come and go.
  Future<void> lookQuietly() async {
    final pairing = ref.read(pairingProvider);
    if (pairing == null) return;
    final next = await AsyncValue.guard<Link?>(() => _find(pairing));
    if (ref.read(pairingProvider) == pairing) state = next;
  }
}

final linkProvider = AsyncNotifierProvider<LinkNotifier, Link?>(
  LinkNotifier.new,
);
