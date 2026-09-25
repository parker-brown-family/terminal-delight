import 'dart:async';

import 'package:app_links/app_links.dart';
import 'package:flutter/material.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:go_router/go_router.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/link.dart';
import 'package:terminal_delight/core/pairing.dart';
import 'package:terminal_delight/features/bench/bench_page.dart';
import 'package:terminal_delight/features/pair/pair_page.dart';
import 'package:terminal_delight/features/wall/wall_page.dart';

final rootNavigatorKey = GlobalKey<NavigatorState>();

class _PairingListenable extends ChangeNotifier {
  _PairingListenable(Ref ref) {
    ref.listen(pairingProvider, (_, _) => notifyListeners());
  }
}

final routerProvider = Provider<GoRouter>((ref) {
  final listenable = _PairingListenable(ref);
  ref.onDispose(listenable.dispose);
  return GoRouter(
    navigatorKey: rootNavigatorKey,
    initialLocation: '/',
    refreshListenable: listenable,
    redirect: (context, state) {
      final paired = ref.read(pairingProvider) != null;
      final atPair = state.matchedLocation == '/pair';
      if (!paired && !atPair) return '/pair';
      if (paired && atPair) return '/';
      return null;
    },
    routes: [
      GoRoute(
        path: '/pair',
        pageBuilder: (context, state) =>
            const NoTransitionPage(child: PairPage()),
      ),
      GoRoute(
        path: '/',
        pageBuilder: (context, state) =>
            const NoTransitionPage(child: WallPage()),
        routes: [
          GoRoute(
            path: 'pane/:session/:pane',
            pageBuilder: (context, state) => CustomTransitionPage(
              key: state.pageKey,
              transitionDuration: const Duration(milliseconds: 320),
              reverseTransitionDuration: const Duration(milliseconds: 220),
              child: BenchPage(
                session: state.pathParameters['session']!,
                pane: int.parse(state.pathParameters['pane']!),
                openTerminal: state.uri.queryParameters['face'] == 'terminal',
              ),
              transitionsBuilder: (context, animation, _, child) {
                final t = CurvedAnimation(
                  parent: animation,
                  curve: const Cubic(0.2, 0.8, 0.2, 1),
                );
                return FadeTransition(
                  opacity: t,
                  child: SlideTransition(
                    position: Tween(
                      begin: const Offset(0.08, 0),
                      end: Offset.zero,
                    ).animate(t),
                    child: child,
                  ),
                );
              },
            ),
          ),
        ],
      ),
    ],
  );
});

class TdApp extends ConsumerStatefulWidget {
  const TdApp({super.key});

  @override
  ConsumerState<TdApp> createState() => _TdAppState();
}

class _TdAppState extends ConsumerState<TdApp> {
  final _links = AppLinks();
  StreamSubscription<Uri>? _sub;
  AppLifecycleListener? _life;

  @override
  void initState() {
    super.initState();
    // Both the link that launched the app and any that arrive while it runs.
    _sub = _links.uriLinkStream.listen(_propose);
    // Back from the pocket: the phone may have left the cable or the Wi-Fi
    // while the app slept, so if the live feed is down, look for a route.
    _life = AppLifecycleListener(
      onResume: () {
        if (ref.read(pairingProvider) == null) return;
        if (!ref.read(eventsProvider).isLive) {
          unawaited(ref.read(linkProvider.notifier).rediscover().catchError((Object _) {}));
        }
      },
    );
  }

  void _propose(Uri uri) {
    final p = Pairing.fromUri(uri);
    if (p != null) ref.read(proposedPairingProvider.notifier).state = p;
  }

  @override
  void dispose() {
    unawaited(_sub?.cancel());
    _life?.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    ref.listen(proposedPairingProvider, (_, next) {
      final ctx = rootNavigatorKey.currentContext;
      if (next == null || ctx == null) return;
      unawaited(showPairingSheet(ctx, next));
    });
    return MaterialApp.router(
      title: 'Terminal Delight',
      debugShowCheckedModeBanner: false,
      theme: tdTheme(),
      routerConfig: ref.watch(routerProvider),
    );
  }
}
