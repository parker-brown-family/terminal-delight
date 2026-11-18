import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:terminal_delight/app/theme/td.dart';
import 'package:terminal_delight/core/api.dart';
import 'package:terminal_delight/core/models.dart';
import 'package:terminal_delight/core/widgets/ground.dart';
import 'package:terminal_delight/core/widgets/marks.dart';
import 'package:terminal_delight/features/bench/term_face.dart';
import 'package:terminal_delight/features/bench/workbench_face.dart';

/// One pane, with the two faces the desk gives it: the WORKBENCH (what the
/// agent made and said) and the TERMINAL (the screen itself). The terminal is
/// built only when first asked for and kept alive after, so flipping back and
/// forth does not re-attach.
class BenchPage extends ConsumerStatefulWidget {
  const BenchPage({required this.session, required this.pane, this.openTerminal = false, super.key});
  final String session;
  final int pane;
  final bool openTerminal;

  @override
  ConsumerState<BenchPage> createState() => _BenchPageState();
}

class _BenchPageState extends ConsumerState<BenchPage> {
  late int _face = widget.openTerminal ? 1 : 0;
  late bool _termBuilt = widget.openTerminal;

  void _flip(int face) {
    if (face == _face) return;
    HapticFeedback.selectionClick();
    setState(() {
      _face = face;
      if (face == 1) _termBuilt = true;
    });
    if (face == 0) FocusScope.of(context).unfocus();
  }

  @override
  Widget build(BuildContext context) {
    ref.watch(eventsProvider);
    final wall = ref.watch(wallProvider(widget.session)).valueOrNull;
    final bench = ref.watch(benchProvider((widget.session, widget.pane))).valueOrNull;
    // The wall is refreshed on every pane-table change, so its reading of
    // who holds the stream is the fresher one.
    final info = wall?.pane(widget.pane) ?? bench?.info;
    final title = wall?.titleOf(widget.pane) ?? 'pane ${widget.pane}';
    final state = info?.state ?? bench?.pulse.state ?? PaneState.unknown;
    final notch = wall?.colourOf(widget.pane);
    final pad = MediaQuery.of(context).padding;

    return Ground(
      tint: _face == 1 ? Td.accent : Td.ident,
      child: Scaffold(
        resizeToAvoidBottomInset: true,
        body: Column(
          children: [
            SizedBox(height: pad.top + 6),
            Padding(
              padding: const EdgeInsets.fromLTRB(4, 0, 14, 0),
              child: Row(
                children: [
                  IconButton(
                    onPressed: () => Navigator.of(context).maybePop(),
                    icon: const Icon(Icons.chevron_left, color: Td.ink, size: 28),
                  ),
                  if (info != null) ModeGlyph(info.mode, size: 17),
                  const SizedBox(width: 8),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: [
                        Text(
                          title,
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TdType.m(15.5, color: Td.bright, weight: FontWeight.w700, spacing: 0.5),
                        ),
                        Text(
                          '${tidyPath(info?.cwd)}  ·  ${info?.mode.label ?? '…'}',
                          maxLines: 1,
                          overflow: TextOverflow.ellipsis,
                          style: TdType.m(10.5, color: Td.inkAt(0.5)),
                        ),
                      ],
                    ),
                  ),
                  const SizedBox(width: 8),
                  StateChip(state),
                  if (notch != null) ...[
                    const SizedBox(width: 10),
                    Container(width: 4, height: 26, color: notch),
                  ],
                ],
              ),
            ),
            Padding(
              padding: const EdgeInsets.fromLTRB(14, 8, 14, 6),
              child: _FaceSwitch(face: _face, onFlip: _flip),
            ),
            Expanded(
              child: IndexedStack(
                index: _face,
                children: [
                  WorkbenchFace(session: widget.session, pane: widget.pane),
                  if (_termBuilt)
                    Padding(
                      padding: EdgeInsets.only(bottom: MediaQuery.of(context).viewInsets.bottom > 0 ? 0 : pad.bottom),
                      child: TermFace(session: widget.session, pane: widget.pane, info: info),
                    )
                  else
                    const SizedBox.shrink(),
                ],
              ),
            ),
          ],
        ),
      ),
    );
  }
}

/// The desk's TERM / BENCH toggle, as two segments with a lit slider.
class _FaceSwitch extends StatelessWidget {
  const _FaceSwitch({required this.face, required this.onFlip});
  final int face;
  final ValueChanged<int> onFlip;

  @override
  Widget build(BuildContext context) {
    const labels = ['▤  WORKBENCH', '▮  TERMINAL'];
    final tints = [Td.ident, Td.accent];
    return LayoutBuilder(
      builder: (context, box) {
        final w = box.maxWidth / 2;
        return Container(
          height: 38,
          decoration: BoxDecoration(
            color: Colors.black.withValues(alpha: 0.35),
            border: Border.all(color: Td.inkAt(0.18)),
            borderRadius: BorderRadius.circular(3),
          ),
          child: Stack(
            children: [
              AnimatedPositioned(
                duration: const Duration(milliseconds: 240),
                curve: const Cubic(0.2, 0.8, 0.2, 1),
                left: face * w,
                top: 0,
                bottom: 0,
                width: w,
                child: AnimatedContainer(
                  duration: const Duration(milliseconds: 240),
                  decoration: BoxDecoration(
                    color: tints[face].withValues(alpha: 0.16),
                    border: Border(bottom: BorderSide(color: tints[face], width: 2)),
                    boxShadow: [BoxShadow(color: tints[face].withValues(alpha: 0.3), blurRadius: 14)],
                  ),
                ),
              ),
              Row(
                children: [
                  for (var i = 0; i < 2; i++)
                    Expanded(
                      child: GestureDetector(
                        behavior: HitTestBehavior.opaque,
                        onTap: () => onFlip(i),
                        child: Center(
                          child: AnimatedDefaultTextStyle(
                            duration: const Duration(milliseconds: 200),
                            style: TdType.m(
                              11.5,
                              color: i == face ? Td.bright : Td.inkAt(0.5),
                              weight: FontWeight.w700,
                              spacing: 2,
                            ),
                            child: Text(labels[i]),
                          ),
                        ),
                      ),
                    ),
                ],
              ),
            ],
          ),
        );
      },
    );
  }
}
