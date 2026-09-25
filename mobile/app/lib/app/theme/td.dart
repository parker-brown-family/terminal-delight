import 'package:flutter/material.dart';

/// Terminal Delight's default theme, Hacker (`app/themes/hacker.toml`), and
/// the bench's tint table (`app/src/benchdraw.rs`), as the phone draws them.
///
/// House rule, from Next Run: a widget never writes a colour, it names a role.
abstract final class Td {
  // The theme.
  static const ground = Color(0xFF03100A);
  static const surface = Color(0xFF071A10);
  static const ink = Color(0xFF86EFAC);
  static const accent = Color(0xFF22C55E);
  static const faint = Color(0xFF14401F);
  static const cursor = Color(0xFF4ADE80);
  static const bright = Color(0xFFECFFF4);

  // The bench's tints, by what they mean.
  /// Structure: documents, tables, responses.
  static const ident = Color(0xFF7AA5FF);

  /// In flight, or yours to argue with.
  static const pending = Color(0xFFFBE08A);

  /// Ready, done, settled.
  static const settled = Color(0xFF4ADE80);

  /// What the person said. The desk derives it from the accent's complement.
  static const human = Color(0xFFE49FCB);

  // The attention rail's lanes (`main.rs`), fixed across themes because they
  // are read at a glance from across a room — and on a phone, from a pocket.
  static const decision = Color(0xFFE25050);
  static const failure = Color(0xFFE68A56);

  /// The amber a fresh install's cabinet wears, and Claude's glyph here.
  static const cabinet = Color(0xFFE0913A);

  static Color inkAt(double a) => ink.withValues(alpha: a);

  /// ANSI 0–15 from the Hacker theme, in order.
  static const ansi = <Color>[
    Color(0xFF0A0F0A),
    Color(0xFFFF4444),
    Color(0xFF22C55E),
    Color(0xFFF5C542),
    Color(0xFF3B82F6),
    Color(0xFFC45AB3),
    Color(0xFF67E8F9),
    Color(0xFF86EFAC),
    Color(0xFF14401F),
    Color(0xFFFF7B7B),
    Color(0xFF4ADE80),
    Color(0xFFFBE08A),
    Color(0xFF7AA5FF),
    Color(0xFFE08AD4),
    Color(0xFFA5F3FC),
    Color(0xFFECFFF4),
  ];

  /// A `#rrggbb` from the session file, or null when it is not one. Null is
  /// drawn as "no colour", never as black.
  static Color? parse(String? hex) {
    if (hex == null || hex.length != 7 || !hex.startsWith('#')) return null;
    final v = int.tryParse(hex.substring(1), radix: 16);
    return v == null ? null : Color(0xFF000000 | v);
  }
}

/// The type ramp. Mono for everything a terminal would say, Grotesk for
/// everything a person reads as prose.
abstract final class TdType {
  static const mono = 'TDMono';
  static const prose = 'Grotesk';
  static const hand = 'Caveat';

  static TextStyle m(
    double size, {
    Color color = Td.ink,
    FontWeight weight = FontWeight.w400,
    double spacing = 0,
    double height = 1.3,
  }) => TextStyle(
    fontFamily: mono,
    fontSize: size,
    color: color,
    fontWeight: weight,
    letterSpacing: spacing,
    height: height,
  );

  static TextStyle p(
    double size, {
    Color color = Td.bright,
    FontWeight weight = FontWeight.w400,
    double height = 1.45,
  }) => TextStyle(
    fontFamily: prose,
    fontSize: size,
    color: color,
    fontWeight: weight,
    height: height,
  );

  /// A section's name: small, spaced, capitals. A NAME, never a sentence.
  static TextStyle kicker({Color color = Td.ink}) => TextStyle(
    fontFamily: mono,
    fontSize: 10.5,
    letterSpacing: 2.6,
    fontWeight: FontWeight.w700,
    color: color.withValues(alpha: 0.72),
  );

  static List<Shadow> glow(Color c, {double strength = 1}) => [
    Shadow(color: c.withValues(alpha: 0.55 * strength), blurRadius: 10),
    Shadow(color: c.withValues(alpha: 0.25 * strength), blurRadius: 22),
  ];
}

ThemeData tdTheme() {
  const scheme = ColorScheme.dark(
    primary: Td.accent,
    onPrimary: Td.ground,
    secondary: Td.pending,
    onSecondary: Td.ground,
    error: Td.decision,
    surface: Td.surface,
    onSurface: Td.ink,
  );
  return ThemeData(
    useMaterial3: true,
    colorScheme: scheme,
    scaffoldBackgroundColor: Colors.transparent,
    fontFamily: TdType.mono,
    splashFactory: InkSparkle.splashFactory,
    textSelectionTheme: TextSelectionThemeData(
      cursorColor: Td.cursor,
      selectionColor: Td.accent.withValues(alpha: 0.35),
      selectionHandleColor: Td.accent,
    ),
    snackBarTheme: SnackBarThemeData(
      behavior: SnackBarBehavior.floating,
      backgroundColor: Td.surface,
      contentTextStyle: TdType.m(12.5),
      shape: RoundedRectangleBorder(
        side: BorderSide(color: Td.accent.withValues(alpha: 0.5)),
        borderRadius: BorderRadius.circular(4),
      ),
    ),
    bottomSheetTheme: const BottomSheetThemeData(
      backgroundColor: Colors.transparent,
      modalBackgroundColor: Colors.transparent,
      elevation: 0,
    ),
    progressIndicatorTheme: const ProgressIndicatorThemeData(
      color: Td.accent,
    ),
  );
}
