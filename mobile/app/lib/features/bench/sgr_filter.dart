/// Rewrites the SGR sequences xterm.dart 4.0 misreads, before it reads them.
///
/// Measured on a Claude Code screen that came out underlined and dimmed from
/// top to bottom. The cause was the first of these; the other two are the
/// same parser's neighbouring gaps:
///
/// - **A private prefix is thrown away.** Claude Code turns on
///   modifyOtherKeys with `ESC[>4;2m`; the parser hands that to SGR as `4;2`,
///   underline and faint, for everything after it. Any `m`-final sequence
///   with a `<`, `=`, `>` or `?` prefix is dropped.
/// - **SGR 58/59 (underline colour) is not a case it knows.** It skips the 58
///   and then reads the colour's own numbers as SGR codes, so `58;2;4;…`
///   switches on faint and underline. The filter drops 58 with its arguments,
///   and 59.
/// - **Colon subparameters are silently concatenated.** `4:0` (underline off)
///   parses as 40, `38:2::255:0:0` as garbage. The filter rewrites the forms
///   terminals actually send into their semicolon spellings.
///
/// Everything else passes through byte for byte. A sequence split across two
/// chunks is held until its final byte arrives.
class SgrFilter {
  String _held = '';

  /// Hold at most this much of an unfinished sequence; anything longer is not
  /// an SGR and is let through as it is.
  static const _maxHeld = 512;

  String feed(String chunk) {
    final s = _held + chunk;
    _held = '';
    final out = StringBuffer();
    var i = 0;
    while (i < s.length) {
      final esc = s.indexOf('\x1b[', i);
      if (esc < 0) {
        // A lone ESC at the very end may be the start of a CSI.
        if (s.endsWith('\x1b')) {
          out.write(s.substring(i, s.length - 1));
          _held = '\x1b';
        } else {
          out.write(s.substring(i));
        }
        break;
      }
      out.write(s.substring(i, esc));
      // Find the final byte: the first char in 0x40–0x7E after the params.
      var j = esc + 2;
      while (j < s.length) {
        final c = s.codeUnitAt(j);
        if (c >= 0x40 && c <= 0x7e) break;
        j++;
      }
      if (j >= s.length) {
        final rest = s.substring(esc);
        if (rest.length > _maxHeld) {
          out.write(rest);
        } else {
          _held = rest;
        }
        break;
      }
      final params = s.substring(esc + 2, j);
      if (s[j] == 'm' && params.isNotEmpty && '<=>?'.contains(params[0])) {
        // A private sequence that happens to end in `m`, which the parser
        // hands to SGR with its prefix thrown away. Dropped: the phone's
        // keyboard sends plain keys either way.
      } else if (s[j] == 'm' && _plain(params)) {
        final fixed = rewrite(params);
        if (fixed != null) out.write('\x1b[${fixed}m');
      } else {
        out.write(s.substring(esc, j + 1));
      }
      i = j + 1;
    }
    return out.toString();
  }

  /// Digits, `;` and `:` only — no private-mode prefix, no intermediates.
  static bool _plain(String p) {
    for (final c in p.codeUnits) {
      if (!((c >= 0x30 && c <= 0x39) || c == 0x3b || c == 0x3a)) return false;
    }
    return true;
  }

  /// The semicolon spelling of an SGR parameter string, or null when nothing
  /// is left to send.
  static String? rewrite(String params) {
    if (params.isEmpty) return '';
    final groups = params.split(';');
    final out = <String>[];
    var i = 0;
    while (i < groups.length) {
      final g = groups[i];
      if (g.contains(':')) {
        final sub = g.split(':');
        final head = sub.first;
        switch (head) {
          case '4':
            // 4:0 is off; 4:1–4:5 are the underline styles, all drawn as one.
            out.add(sub.length > 1 && sub[1] == '0' ? '24' : '4');
          case '38' || '48':
            if (sub.length > 1 && sub[1] == '2' && sub.length >= 5) {
              // 38:2:r:g:b, or 38:2:<colourspace>:r:g:b — the last three.
              final rgb = sub.sublist(sub.length - 3);
              out.add('$head;2;${rgb.map((v) => v.isEmpty ? '0' : v).join(';')}');
            } else if (sub.length > 2 && sub[1] == '5') {
              out.add('$head;5;${sub[2]}');
            }
          case '58':
            break;
          default:
            if (head.isNotEmpty) out.add(head);
        }
        i++;
        continue;
      }
      switch (g) {
        case '38' || '48':
          // Keep the colour's arguments with it, so they are never read as
          // codes of their own.
          final mode = i + 1 < groups.length ? groups[i + 1] : '';
          final n = mode == '2' ? 5 : (mode == '5' ? 3 : 1);
          out.add(groups.sublist(i, (i + n).clamp(0, groups.length)).join(';'));
          i += n;
        case '58':
          final mode = i + 1 < groups.length ? groups[i + 1] : '';
          i += mode == '2' ? 5 : (mode == '5' ? 3 : 1);
        case '59':
          i++;
        default:
          out.add(g);
          i++;
      }
    }
    return out.isEmpty ? null : out.join(';');
  }
}
