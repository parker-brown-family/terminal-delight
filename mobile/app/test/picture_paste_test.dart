import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:xterm/xterm.dart';

/// The one change made to the vendored xterm (third_party/xterm/TD.md): the
/// phone's keyboard is told the terminal takes pictures, and a picture it
/// commits reaches the app. Upstream tells the keyboard "text only" and drops
/// committed content, so a picture tapped in the keyboard's clipboard panel
/// went nowhere. This fails if an upgrade loses the patch.
void main() {
  testWidgets('the keyboard is offered pictures, and one it commits arrives', (tester) async {
    KeyboardInsertedContent? got;
    final focus = FocusNode();
    addTearDown(focus.dispose);
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: TerminalView(
            Terminal(),
            focusNode: focus,
            contentInsertionConfiguration: ContentInsertionConfiguration(
              allowedMimeTypes: const ['image/png', 'image/jpeg'],
              onContentInserted: (c) => got = c,
            ),
          ),
        ),
      ),
    );
    focus.requestFocus();
    await tester.pump();

    final setClient = tester.testTextInput.log.lastWhere((m) => m.method == 'TextInput.setClient');
    final args = setClient.arguments as List<dynamic>;
    final config = args[1] as Map<dynamic, dynamic>;
    expect(config['contentCommitMimeTypes'], ['image/png', 'image/jpeg']);

    // What the engine sends when the keyboard commits a picture.
    await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
      SystemChannels.textInput.name,
      SystemChannels.textInput.codec.encodeMethodCall(
        MethodCall('TextInputClient.performAction', <dynamic>[
          args[0],
          'TextInputAction.commitContent',
          const <String, dynamic>{
            'mimeType': 'image/png',
            'uri': 'content://keyboard/clip/1',
            'data': <int>[0x89, 0x50, 0x4e, 0x47],
          },
        ]),
      ),
      (_) {},
    );
    expect(got?.mimeType, 'image/png');
    expect(got?.data, [0x89, 0x50, 0x4e, 0x47]);
  });

  testWidgets('without a configuration the keyboard is still told text only', (tester) async {
    final focus = FocusNode();
    addTearDown(focus.dispose);
    await tester.pumpWidget(
      MaterialApp(home: Scaffold(body: TerminalView(Terminal(), focusNode: focus))),
    );
    focus.requestFocus();
    await tester.pump();
    final setClient = tester.testTextInput.log.lastWhere((m) => m.method == 'TextInput.setClient');
    final config = (setClient.arguments as List<dynamic>)[1] as Map<dynamic, dynamic>;
    expect(config['contentCommitMimeTypes'], isEmpty);
  });
}
