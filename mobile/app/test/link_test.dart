import 'package:flutter_test/flutter_test.dart';
import 'package:terminal_delight/core/link.dart';

// The routes as the trouble screen names them, from a pairing with a tailnet.
Unreachable _failed(Here here, {bool tailnet = true}) => Unreachable('legion', {
  'USB 127.0.0.1': whyNot('USB', 'refused', here),
  if (tailnet) 'TAILSCALE 100.64.0.1': whyNot('TAILSCALE', 'no answer', here),
  'WI-FI 192.168.1.20': whyNot('WI-FI', 'no answer', here),
}, here);

void main() {
  group('whyNot', () {
    test('a cable carrying no computer says so, not "refused"', () {
      expect(whyNot('USB', 'refused', const Here(cable: false, adb: true)), 'not plugged in');
    });

    test('a cable that is in with debugging off names the setting', () {
      expect(whyNot('USB', 'refused', const Here(cable: true, adb: false)), 'USB debugging off');
    });

    test('a cable that is in and debugging on keeps what the probe saw', () {
      // The laptop end is at fault, and only the probe knows how.
      expect(whyNot('USB', 'refused', const Here(cable: true, adb: true)), 'refused');
    });

    test('Tailscale switched off here says so, not "no answer"', () {
      expect(whyNot('TAILSCALE', 'no answer', const Here(tailnet: false)), 'Tailscale off here');
    });

    test('a phone on cellular says it is not on Wi-Fi', () {
      expect(whyNot('WI-FI', 'no answer', const Here(wifi: false)), 'not on Wi-Fi');
    });

    test('a phone that would not say leaves every probe reason alone', () {
      // Unknown is not "off": nothing is invented when the channel is silent.
      const silent = Here();
      expect(whyNot('USB', 'refused', silent), 'refused');
      expect(whyNot('TAILSCALE', 'no answer', silent), 'no answer');
      expect(whyNot('WI-FI', 'no answer', silent), 'no answer');
    });

    test('a wrong machine answering wins over what the phone sees', () {
      expect(whyNot('TAILSCALE', 'answered as omarchy-bfs', const Here(tailnet: true)), 'answered as omarchy-bfs');
    });
  });

  group('advice', () {
    test('never sends anyone to re-pair, whatever the phone saw', () {
      for (final here in const [
        Here(),
        Here(cable: false, tailnet: false),
        Here(cable: true, adb: true, tailnet: true),
        Here(cable: true, adb: false),
      ]) {
        expect(advice(_failed(here)).join(' '), isNot(contains('pair')));
      }
    });

    test('the 2026-10-21 failure: cable out, Tailscale off, on cellular', () {
      final lines = advice(_failed(const Here(cable: false, adb: true, tailnet: false, wifi: false)));
      expect(lines, hasLength(2));
      expect(lines[0], contains('sees no computer on its USB port'));
      expect(lines[1], contains('switch Tailscale on here'));
    });

    test('a cable that is in points at the laptop, not the cable', () {
      final lines = advice(_failed(const Here(cable: true, adb: true, tailnet: true)));
      expect(lines[0], contains('legion is not answering on it'));
      expect(lines[1], contains('Is legion awake'));
    });

    test('no tailnet in the pairing, no Tailscale line', () {
      final lines = advice(_failed(const Here(cable: false, tailnet: false), tailnet: false));
      expect(lines, hasLength(1));
    });
  });
}
