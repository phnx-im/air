// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/message_input/message_input.dart';
import 'package:air/ds/patterns/message_input/message_input_quote.dart';
import 'package:air/ds/patterns/message_input/message_input_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('MessageInputQuote', () {
    const preview = 'Sounds good, see you then';

    Widget buildSubject({VoidCallback? onRemove}) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: Align(
          alignment: Alignment.bottomCenter,
          child: MessageInput(
            tokens: MessageInputTokens.phone,
            field: const TextField(
              decoration: InputDecoration(
                isCollapsed: true,
                contentPadding: EdgeInsets.zero,
              ),
            ),
            aboveField: [
              MessageInputQuote(
                preview: preview,
                senderName: 'Alex',
                onRemove: onRemove,
              ),
            ],
            leadingIcon: AppIconType.plus,
            onLeading: (_) {},
            sendIcon: AppIconType.arrowUp,
            onSend: () {},
            onScrollBack: () {},
          ),
        ),
      ),
    );

    Finder buttonWith(AppIconType icon) =>
        find.byWidgetPredicate((w) => w is ButtonIcon && w.icon == icon);

    Finder fieldBox() => find
        .ancestor(of: find.byType(TextField), matching: find.byType(Container))
        .first;

    /// The quote's fill, found by the color only it wears: the accent-rule
    /// container beside it decorates a border, not a fill.
    Finder quoteBlock() => find.byWidgetPredicate((w) {
      if (w is! Container) return false;
      final decoration = w.decoration;
      return decoration is BoxDecoration &&
          decoration.color == lightSemanticPalette.fill.secondary;
    });

    testWidgets('the staged quote holds the field off on its own', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      final textBottom = tester.getRect(find.text(preview)).bottom;
      // The chrome ancestor wraps the quote too, so its top is above the
      // field's own top once aboveField is non-empty. The field itself is
      // what carries the gap, so that's what has to be measured.
      final fieldTop = tester.getRect(find.byType(TextField)).top;

      // The field zeroes its own top inset while the quote is staged, so the
      // whole gap is the block's: its bottom inset plus the gap under it.
      expect(
        fieldTop - textBottom,
        moreOrLessEquals(
          MessageInputQuoteTokens.padding.bottom +
              MessageInputQuoteTokens.gapBelow,
        ),
      );
    });

    testWidgets('the fill stops short of the field', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      final fillBottom = tester.getRect(quoteBlock()).bottom;
      final fieldTop = tester.getRect(find.byType(TextField)).top;

      expect(
        fieldTop - fillBottom,
        moreOrLessEquals(MessageInputQuoteTokens.gapBelow),
      );
    });

    testWidgets('the quote spans the field', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      final chromeWidth = tester.getSize(fieldBox()).width;
      final quoteWidth = tester.getSize(quoteBlock()).width;

      expect(quoteWidth, chromeWidth - 2 * MessageInputQuoteTokens.gapSide);
    });

    testWidgets('dismissing the quote reaches the host', (tester) async {
      var removed = 0;
      await tester.pumpWidget(buildSubject(onRemove: () => removed++));
      await tester.pumpAndSettle();
      await tester.tap(buttonWith(AppIconType.x));

      expect(removed, 1);

      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      expect(buttonWith(AppIconType.x), findsNothing);
    });
  });
}
