// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/foundations/foundations.dart';
import 'package:air/features/you/you_fields.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('FieldContainer', () {
    setUp(() {
      // The palette follows the platform brightness, so pin it to compare
      // against one palette.
      TestWidgetsFlutterBinding.ensureInitialized()
              .platformDispatcher
              .platformBrightnessTestValue =
          .dark;
      addTearDown(
        TestWidgetsFlutterBinding.ensureInitialized()
            .platformDispatcher
            .clearPlatformBrightnessTestValue,
      );
    });

    Widget buildSubject() => MaterialApp(
      debugShowCheckedModeBanner: false,
      theme: testThemeData(.dark),
      home: const Scaffold(body: FieldContainer(child: Text('handle'))),
    );

    Color fillOf(WidgetTester tester) {
      final container = tester.widget<Container>(
        find.descendant(
          of: find.byType(FieldContainer),
          matching: find.byType(Container),
        ),
      );
      return (container.decoration! as BoxDecoration).color!;
    }

    testWidgets('paints one translucent tile fill at both breakpoints', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(buildSubject());
      final onDetailPane = fillOf(tester);

      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(buildSubject());
      final onPhone = fillOf(tester);

      expect(onDetailPane, darkSemanticPalette.fill.tertiary);
      expect(onPhone, onDetailPane);
      // Translucent, so the module lifts off the detail pane and the phone
      // screen alike without a tier per breakpoint.
      expect(onDetailPane.a, lessThan(1.0));
    });
  });
}
