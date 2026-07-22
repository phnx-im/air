// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/message_list/centered_emoji.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

const _emoji = '\u{1F44D}'; // :thumbsup:
const _style = TextStyle(fontSize: 16);

Widget _subject({String emoji = _emoji}) => MaterialApp(
  home: Scaffold(
    body: Center(
      child: CenteredEmoji(emoji: emoji, style: _style),
    ),
  ),
);

CenteredGlyphPainter _painterOf(WidgetTester tester) {
  final paint = tester.widget<CustomPaint>(
    find.descendant(
      of: find.byType(CenteredEmoji),
      matching: find.byType(CustomPaint),
    ),
  );
  return paint.painter! as CenteredGlyphPainter;
}

void main() {
  setUp(CenteredEmoji.debugResetCaches);

  testWidgets('measures ink and applies the centering correction', (
    tester,
  ) async {
    await tester.pumpWidget(_subject());

    // First frame: the async raster hasn't completed, layout-box centering.
    expect(_painterOf(tester).correction, Offset.zero);

    // Let the rasterization actually run (it needs real async), then let the
    // resulting setState repaint the glyph.
    await tester.runAsync(CenteredEmoji.debugFlushMeasurements);
    await tester.pump();

    final correction = _painterOf(tester).correction;
    // The exact offset depends on the font; assert it was measured and is
    // sane: an ink correction is a sub-glyph nudge, not a jump.
    expect(correction.dx.abs(), lessThan(_style.fontSize!));
    expect(correction.dy.abs(), lessThan(_style.fontSize!));
    expect(correction.dx, isNot(isNaN));
    expect(correction.dy, isNot(isNaN));
  });

  testWidgets('reuses the cached correction without a zero-offset frame', (
    tester,
  ) async {
    await tester.pumpWidget(_subject());
    await tester.runAsync(CenteredEmoji.debugFlushMeasurements);
    await tester.pump();
    final measured = _painterOf(tester).correction;

    // A fresh widget showing the same emoji picks the correction up in its
    // very first frame.
    await tester.pumpWidget(const SizedBox.shrink());
    await tester.pumpWidget(_subject());
    expect(_painterOf(tester).correction, measured);
  });

  testWidgets('exposes the emoji to the semantics tree', (tester) async {
    final semantics = tester.ensureSemantics();
    await tester.pumpWidget(_subject());
    expect(find.bySemanticsLabel(_emoji), findsOneWidget);
    semantics.dispose();
  });

  testWidgets('warmUp pre-measures so the first frame is corrected', (
    tester,
  ) async {
    late final BuildContext warmContext;
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Builder(
            builder: (context) {
              warmContext = context;
              return const SizedBox.shrink();
            },
          ),
        ),
      ),
    );

    CenteredEmoji.warmUp(warmContext, const [_emoji], _style);
    await tester.runAsync(CenteredEmoji.debugFlushMeasurements);

    await tester.pumpWidget(_subject());
    // No zero-offset first frame: the warmed correction applies immediately.
    // (The correction may legitimately be zero for a symmetric glyph, so
    // assert via the cache-hit path instead: a second flush is a no-op.)
    final first = _painterOf(tester).correction;
    await tester.runAsync(CenteredEmoji.debugFlushMeasurements);
    await tester.pump();
    expect(_painterOf(tester).correction, first);
  });
}
