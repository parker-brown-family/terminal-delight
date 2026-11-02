import 'package:flutter_test/flutter_test.dart';
import 'package:terminal_delight/features/bench/sgr_filter.dart';

void main() {
  String run(List<String> chunks) {
    final f = SgrFilter();
    return chunks.map(f.feed).join();
  }

  test("Claude Code's modifyOtherKeys is not read as underline and faint", () {
    // The bytes that underlined a whole Claude Code screen on the S21 FE.
    expect(SgrFilter().feed('\x1b[>4;2mClaude'), 'Claude');
    expect(SgrFilter().feed('\x1b[>4m\x1b[?2004h'), '\x1b[?2004h');
    expect(run(['a\x1b[>4', ';2mb']), 'ab');
  });

  test('underline colour is dropped with its arguments', () {
    // 58;2;4;… read raw would switch on faint (2) and underline (4).
    expect(SgrFilter.rewrite('1;58;2;4;200;100;38;5;208'), '1;38;5;208');
    expect(SgrFilter.rewrite('58;5;4'), isNull);
    expect(SgrFilter.rewrite('59'), isNull);
  });

  test('colon forms become semicolon forms', () {
    expect(SgrFilter.rewrite('4:0'), '24');
    expect(SgrFilter.rewrite('4:3'), '4');
    expect(SgrFilter.rewrite('38:2::255:128:0'), '38;2;255;128;0');
    expect(SgrFilter.rewrite('38:2:255:128:0'), '38;2;255;128;0');
    expect(SgrFilter.rewrite('48:5:17'), '48;5;17');
    expect(SgrFilter.rewrite('58:2::1:2:3;1'), '1');
  });

  test('a colour argument that looks like a code stays an argument', () {
    expect(SgrFilter.rewrite('38;2;4;4;4'), '38;2;4;4;4');
    expect(SgrFilter.rewrite('48;5;4;1'), '48;5;4;1');
  });

  test('a reset stays a reset', () {
    expect(SgrFilter().feed('\x1b[m'), '\x1b[m');
    expect(SgrFilter().feed('\x1b[0m'), '\x1b[0m');
  });

  test('everything that is not an SGR passes through untouched', () {
    const s = 'ab\x1b[?1049h\x1b[2J\x1b[10;20Hx\x1b]0;title\x07y';
    expect(SgrFilter().feed(s), s);
  });

  test('a sequence split across chunks is held until it completes', () {
    expect(run(['hi \x1b[4', ':0m there']), 'hi \x1b[24m there');
    expect(run(['a\x1b', '[58;2;1;2;3mb']), 'ab');
    expect(run(['x\x1b[', '1m']), 'x\x1b[1m');
  });
}
