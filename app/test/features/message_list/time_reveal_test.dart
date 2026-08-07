// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/message_list/swipe_to_reply.dart';
import 'package:air/features/message_list/time_reveal.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('TimeRevealRow', () {
    final bubbleKey = GlobalKey();
    late TimeRevealController reveal;
    late int replies;

    /// One row laid out the way a message row is: full width, with the bubble
    /// on [side] behind the row's own padding. An own row runs to the trailing
    /// padding, an incoming one stops short of it.
    ///
    /// Without [revealable] there is no scope over it, which is the layout the
    /// column has to leave alone.
    Widget harness({
      bool revealable = true,
      Alignment side = Alignment.centerRight,
      double bubbleWidth = 200,
    }) {
      replies = 0;
      final row = Builder(
        builder: (context) {
          final controller = TimeRevealScope.maybeOf(context);
          if (controller != null) reveal = controller;
          return TimeRevealRow(
            timestamp: DateTime(2026, 1, 1, 14, 32),
            bubbleKey: bubbleKey,
            child: SwipeToReplyScope(
              onReply: () => replies++,
              child: Padding(
                padding: const EdgeInsets.symmetric(horizontal: S.s16),
                child: Align(
                  alignment: side,
                  heightFactor: 1,
                  child: Container(
                    key: bubbleKey,
                    width: bubbleWidth,
                    height: 40,
                    color: const Color(0xFF000000),
                  ),
                ),
              ),
            ),
          );
        },
      );

      return MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testLightTheme,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(body: revealable ? TimeRevealScope(child: row) : row),
      );
    }

    Future<TestGesture> dragFrom(WidgetTester tester) =>
        tester.startGesture(tester.getCenter(find.byType(TimeRevealRow)));

    testWidgets('a leftward drag keeps driving the column once it is out', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(harness());

      final gesture = await dragFrom(tester);
      await gesture.moveBy(const Offset(-15, 0));
      await tester.pump();
      final firstFrame = reveal.progress.value;
      expect(firstFrame, greaterThan(0));

      // The column came out, so every row rebuilt under the finger. The same
      // distance again has to move it just as far.
      await gesture.moveBy(const Offset(-15, 0));
      await tester.pump();
      expect(
        reveal.progress.value,
        moreOrLessEquals(firstFrame * 2, epsilon: 0.001),
      );

      await gesture.up();
      await tester.pumpAndSettle();
      expect(reveal.progress.value, 0);
      expect(replies, 0);
    });

    /// Drives the column all the way out and hands back the two boxes that have
    /// to keep out of each other's way.
    Future<(Rect label, Rect bubble)> reveal_(WidgetTester tester) async {
      final gesture = await dragFrom(tester);
      await gesture.moveBy(const Offset(-15, 0));
      await tester.pump();
      await gesture.moveBy(const Offset(-200, 0));
      await tester.pump();
      expect(reveal.progress.value, 1);
      addTearDown(() async {
        await gesture.up();
        await tester.pumpAndSettle();
      });
      return (
        tester.getRect(find.byType(Text)),
        tester.getRect(find.byKey(bubbleKey)),
      );
    }

    testWidgets('the label clears an own bubble at the trailing edge', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(harness());

      final (label, bubble) = await reveal_(tester);
      expect(label.left - bubble.right, greaterThanOrEqualTo(S.s16));
      expect(phoneViewSize.width - label.right, moreOrLessEquals(S.s16));
    });

    testWidgets('the label clears an incoming bubble that runs the row', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(
        harness(side: Alignment.centerLeft, bubbleWidth: 340),
      );

      final (label, bubble) = await reveal_(tester);
      expect(label.left - bubble.right, greaterThanOrEqualTo(S.s16));
    });

    testWidgets('a bubble that stops short of the column holds still', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(
        harness(side: Alignment.centerLeft, bubbleWidth: 100),
      );
      final resting = tester.getRect(find.byKey(bubbleKey));

      final (label, bubble) = await reveal_(tester);
      expect(bubble, resting);
      expect(label.left - bubble.right, greaterThanOrEqualTo(S.s16));
    });

    testWidgets('the column takes its time going back however far it came', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(harness());

      final gesture = await dragFrom(tester);
      await gesture.moveBy(const Offset(-20, 0));
      await tester.pump();
      await gesture.up();

      // A third of the way out returns over the same span as a full column,
      // rather than a third of it.
      await tester.pump();
      await tester.pump(Effect.duration(MotionPreset.short));
      expect(reveal.progress.value, greaterThan(0));
      await tester.pumpAndSettle();
      expect(reveal.progress.value, 0);
    });

    testWidgets('the row rests where it would without a column', (
      tester,
    ) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(harness(revealable: false));
      final plain = tester.getRect(find.byKey(bubbleKey));

      await tester.pumpWidget(harness());
      expect(tester.getRect(find.byKey(bubbleKey)), plain);
    });

    testWidgets('a rightward drag still replies', (tester) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(harness());

      final gesture = await dragFrom(tester);
      await gesture.moveBy(const Offset(40, 0));
      await tester.pump();
      await gesture.moveBy(const Offset(80, 0));
      await tester.pump();
      expect(reveal.progress.value, 0);

      await gesture.up();
      await tester.pumpAndSettle();
      expect(replies, 1);
    });
  });
}
