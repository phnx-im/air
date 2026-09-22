// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/corner_dot/corner_dot_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:air/ds/patterns/chat_header_bar/chat_header_bar.dart';
import 'package:air/ds/patterns/chat_header_bar/chat_header_bar_tokens.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _name = 'Alice Anderson';

void main() {
  group('ChatHeaderBar', () {
    Widget buildSubject({
      String? subtitle,
      VoidCallback? onTap,
      VoidCallback? onBack,
      bool backEmphasized = false,
    }) => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testLightTheme,
      home: Scaffold(
        body: ChatHeaderBar(
          tokens: ChatHeaderBarTokens.phone,
          name: _name,
          subtitle: subtitle,
          avatar: const SizedBox.square(dimension: S.s32),
          onTap: onTap,
          onBack: onBack,
          backEmphasized: backEmphasized,
        ),
      ),
    );

    /// The pill's decoration, found via the name it wraps.
    BoxDecoration pillDecoration(WidgetTester tester) {
      final pill = find
          .ancestor(of: find.text(_name), matching: find.byType(Container))
          .first;
      return tester.widget<Container>(pill).decoration! as BoxDecoration;
    }

    testWidgets('pill always carries its surface', (tester) async {
      await tester.pumpWidget(buildSubject());

      expect(
        pillDecoration(tester).color,
        lightSemanticPalette.backgroundElevated.primary,
      );
    });

    testWidgets('onTap fires from the pill but not from dead bar space', (
      tester,
    ) async {
      var taps = 0;
      await tester.pumpWidget(buildSubject(onTap: () => taps++));

      await tester.tap(find.text(_name));
      expect(taps, 1);

      // The trailing slot is an empty spacer, so a tap in it hits nothing.
      const tokens = ChatHeaderBarTokens.phone;
      final bar = tester.getRect(find.byType(ChatHeaderBar));
      final slotCenter =
          bar.right - ChatHeaderBarTokens.paddingRight - tokens.slotSize / 2;
      await tester.tapAt(Offset(slotCenter, bar.center.dy));
      expect(taps, 1);
    });

    testWidgets('back button renders and fires only when handled', (
      tester,
    ) async {
      await tester.pumpWidget(buildSubject());
      expect(find.byType(ButtonIcon), findsNothing);

      var backs = 0;
      await tester.pumpWidget(buildSubject(onBack: () => backs++));
      expect(find.byType(ButtonIcon), findsOneWidget);

      await tester.tap(find.byType(ButtonIcon));
      expect(backs, 1);
    });

    testWidgets('back button carries the corner dot only when emphasized', (
      tester,
    ) async {
      /// The dot is the only circular box in the bar sized off CornerDotTokens.
      Finder dot() => find.byWidgetPredicate(
        (widget) =>
            widget is Container &&
            widget.decoration is BoxDecoration &&
            (widget.decoration! as BoxDecoration).shape == .circle,
      );

      await tester.pumpWidget(buildSubject(onBack: () {}));
      expect(dot(), findsNothing);

      await tester.pumpWidget(
        buildSubject(onBack: () {}, backEmphasized: true),
      );
      expect(dot(), findsOneWidget);
      expect(
        tester.getSize(dot()),
        const Size(CornerDotTokens.size, CornerDotTokens.size),
      );
    });

    testWidgets('subtitle renders only when provided', (tester) async {
      await tester.pumpWidget(buildSubject());
      expect(find.text('Verified contact'), findsNothing);

      await tester.pumpWidget(buildSubject(subtitle: 'Verified contact'));
      expect(find.text('Verified contact'), findsOneWidget);
    });
  });
}
