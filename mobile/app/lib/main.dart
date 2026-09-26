import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/app.dart';
import 'package:terminal_delight/core/pairing.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await SystemChrome.setEnabledSystemUIMode(SystemUiMode.edgeToEdge);
  SystemChrome.setSystemUIOverlayStyle(
    const SystemUiOverlayStyle(
      statusBarColor: Colors.transparent,
      systemNavigationBarColor: Colors.transparent,
      statusBarIconBrightness: Brightness.light,
      systemNavigationBarIconBrightness: Brightness.light,
    ),
  );
  // Read before the first frame, so a paired phone opens on the wall rather
  // than flashing the pairing screen on its way there.
  const store = Store();
  final pairing = await store.readPairing();
  final font = await store.readFont();
  runApp(
    ProviderScope(
      overrides: [
        initialPairingProvider.overrideWithValue(pairing),
        initialFontProvider.overrideWithValue(font),
      ],
      child: const TdApp(),
    ),
  );
}
