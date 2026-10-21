/// Typed reads out of decoded JSON.
///
/// The gateway passes the host's and the mailbox's documents through without
/// reshaping them, so a field can be absent, null, or a different type from
/// the one a newer build writes. Each of these answers `null` for anything
/// that is not what was asked for — absent stays absent, and nothing here
/// turns it into a zero.
typedef Json = Map<String, dynamic>;

String? jStr(Object? v) => v is String ? v : null;

int? jInt(Object? v) => v is int ? v : (v is num ? v.toInt() : null);

bool? jBool(Object? v) => v is bool ? v : null;

Json? jMap(Object? v) => v is Map ? v.cast<String, dynamic>() : null;

List<Object?> jList(Object? v) =>
    v is List ? v.cast<Object?>() : const <Object?>[];

/// A string with something in it, or null.
String? jText(Object? v) {
  final s = jStr(v)?.trim();
  return (s == null || s.isEmpty) ? null : s;
}
