// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/list_header/list_header.dart';
import 'package:air/ds/patterns/list_header/list_header_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('ListHeader', () {
    Widget buildSubject({
      required ListHeaderTokens tokens,
      double scrollOffset = 0,
      VoidCallback? onAction,
    }) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: ListHeader(
          tokens: tokens,
          title: 'Chats',
          scrollOffset: ValueNotifier<double>(scrollOffset),
          leading: ListHeaderAction(
            tokens: tokens,
            onAction: onAction == null ? null : (_) => onAction(),
          ),
        ),
      ),
    );

    /// The pill is the only decorated box inside the header carrying a fill.
    BoxDecoration pillDecoration(WidgetTester tester) {
      final box = tester.widget<DecoratedBox>(
        find
            .ancestor(
              of: find.text('Chats'),
              matching: find.byType(DecoratedBox),
            )
            .first,
      );
      return box.decoration as BoxDecoration;
    }

    testWidgets('hides the title pill while the list rests at the top', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject(tokens: ListHeaderTokens.phone));

      final decoration = pillDecoration(tester);
      expect(decoration.color!.a, 0);
      expect(decoration.border!.top.color.a, 0);
      // The label itself always shows -- only its pill is held back.
      expect(find.text('Chats'), findsOneWidget);
    });

    testWidgets('reveals the title pill part way through the ramp', (
      tester,
    ) async {
      await tester.pumpWidget(
        buildSubject(
          tokens: ListHeaderTokens.phone,
          scrollOffset: ListHeaderTokens.pillRevealDistance / 2,
        ),
      );

      final alpha = pillDecoration(tester).color!.a;
      expect(alpha, greaterThan(0));
      expect(alpha, lessThan(1));
    });

    testWidgets('fully reveals the title pill past the ramp', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          tokens: ListHeaderTokens.phone,
          scrollOffset: ListHeaderTokens.pillRevealDistance * 2,
        ),
      );

      expect(pillDecoration(tester).color!.a, 1);
    });

    testWidgets('drops the title in the two-pane layout', (tester) async {
      await tester.pumpWidget(buildSubject(tokens: ListHeaderTokens.desktop));

      expect(find.text('Chats'), findsNothing);
    });

    testWidgets('reports a tap on the action once', (tester) async {
      var taps = 0;
      await tester.pumpWidget(
        buildSubject(tokens: ListHeaderTokens.phone, onAction: () => taps++),
      );

      await tester.tap(find.byType(ListHeaderAction));
      await tester.pumpAndSettle();

      expect(taps, 1);
    });

    testWidgets('hands the action its own context', (tester) async {
      BuildContext? reported;
      await tester.pumpWidget(
        MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          home: Scaffold(
            body: ListHeader(
              tokens: ListHeaderTokens.phone,
              title: 'Chats',
              leading: ListHeaderAction(
                tokens: ListHeaderTokens.phone,
                onAction: (buttonContext) => reported = buttonContext,
              ),
            ),
          ),
        ),
      );

      await tester.tap(find.byType(ListHeaderAction));
      await tester.pumpAndSettle();

      // A menu anchors off this context, so it has to resolve to the button's
      // box rather than to the bar around it.
      final box = reported!.findRenderObject()! as RenderBox;
      expect(box.size.width, ListHeaderTokens.phone.actionSize);
    });

    testWidgets('centers the title between the reserved slots', (tester) async {
      const tokens = ListHeaderTokens.phone;
      await tester.pumpWidget(buildSubject(tokens: tokens));

      // Both slots reserve the same width even though only the leading one is
      // filled, so the title sits centered on the bar's padded content box --
      // which the bar's asymmetric padding puts just off the raw center.
      final bar = tester.getRect(find.byType(ListHeader));
      final contentCenter =
          (bar.left +
              ListHeaderTokens.paddingLeft +
              bar.right -
              tokens.paddingRight) /
          2;
      final title = tester.getRect(find.text('Chats'));
      expect(title.center.dx, moreOrLessEquals(contentCenter, epsilon: 0.5));
    });
  });
}
