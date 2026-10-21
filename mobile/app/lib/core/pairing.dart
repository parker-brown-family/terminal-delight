import 'dart:convert';

import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_secure_storage/flutter_secure_storage.dart';
import 'package:terminal_delight/core/json.dart';

/// One way the phone can reach the laptop.
class NetRoute {
  const NetRoute(this.name, this.base);
  final String name;
  final String base;
}

/// What the gateway handed over on the USB cable: the token, and every address
/// the laptop might be reached at later.
class Pairing {
  const Pairing({
    required this.token,
    required this.machine,
    required this.port,
    this.tailnetIp,
    this.tailnetName,
    this.lan = const [],
  });

  factory Pairing.fromJson(Json m) => Pairing(
    token: jStr(m['token']) ?? '',
    machine: jStr(m['machine']) ?? 'this machine',
    port: jInt(m['port']) ?? 7717,
    tailnetIp: jText(m['tailnet_ip']),
    tailnetName: jText(m['tailnet_name']),
    lan: jList(m['lan']).whereType<String>().toList(),
  );

  /// `tdmobile://pair?token=…&machine=…&port=…&tailnet_ip=…&lan=…`, or null
  /// when the link is not one of ours.
  static Pairing? fromUri(Uri uri) {
    if (uri.scheme != 'tdmobile' || uri.host != 'pair') return null;
    final q = uri.queryParametersAll;
    final token = q['token']?.first;
    if (token == null || token.length != 64) return null;
    return Pairing(
      token: token,
      machine: q['machine']?.first ?? 'this machine',
      port: int.tryParse(q['port']?.first ?? '') ?? 7717,
      tailnetIp: q['tailnet_ip']?.first,
      tailnetName: q['tailnet_name']?.first,
      lan: q['lan'] ?? const [],
    );
  }

  final String token;
  final String machine;
  final int port;
  final String? tailnetIp;
  final String? tailnetName;
  final List<String> lan;

  Json toJson() => {
    'token': token,
    'machine': machine,
    'port': port,
    'tailnet_ip': tailnetIp,
    'tailnet_name': tailnetName,
    'lan': lan,
  };

  /// In the order they are preferred: the cable is fastest and private, the
  /// tailnet works from anywhere, the LAN only when the gateway allows it.
  List<NetRoute> get routes => [
    NetRoute('USB', 'http://127.0.0.1:$port'),
    if (tailnetIp != null) NetRoute('TAILSCALE', 'http://$tailnetIp:$port'),
    for (final ip in lan) NetRoute('WI-FI', 'http://$ip:$port'),
  ];

  /// Enough of the token to compare against `td-mobile-gateway token` by eye,
  /// and not enough to use.
  String get fingerprint => '${token.substring(0, 4)}…${token.substring(60)}';
}

class Store {
  const Store([this._s = const FlutterSecureStorage(
    aOptions: AndroidOptions(encryptedSharedPreferences: true),
  )]);
  final FlutterSecureStorage _s;

  static const _pairing = 'td_pairing';
  static const _font = 'td_term_font';

  Future<Pairing?> readPairing() async {
    try {
      final raw = await _s.read(key: _pairing);
      if (raw == null) return null;
      final p = Pairing.fromJson(jMap(jsonDecode(raw)) ?? const {});
      return p.token.length == 64 ? p : null;
    } on Object {
      return null;
    }
  }

  Future<void> writePairing(Pairing? p) => p == null
      ? _s.delete(key: _pairing)
      : _s.write(key: _pairing, value: jsonEncode(p.toJson()));

  Future<double?> readFont() async {
    try {
      return double.tryParse(await _s.read(key: _font) ?? '');
    } on Object {
      return null;
    }
  }

  Future<void> writeFont(double size) =>
      _s.write(key: _font, value: size.toStringAsFixed(1));
}

final storeProvider = Provider<Store>((_) => const Store());

/// Read in `main()` before the first frame, so a paired phone never flashes
/// the pairing screen.
final initialPairingProvider = Provider<Pairing?>((_) => null);
final initialFontProvider = Provider<double?>((_) => null);

class PairingNotifier extends Notifier<Pairing?> {
  @override
  Pairing? build() => ref.read(initialPairingProvider);

  Future<void> accept(Pairing p) async {
    await ref.read(storeProvider).writePairing(p);
    state = p;
  }

  Future<void> forget() async {
    await ref.read(storeProvider).writePairing(null);
    state = null;
  }
}

final pairingProvider = NotifierProvider<PairingNotifier, Pairing?>(
  PairingNotifier.new,
);

/// A pairing link that has arrived and not yet been answered.
final proposedPairingProvider = StateProvider<Pairing?>((_) => null);
