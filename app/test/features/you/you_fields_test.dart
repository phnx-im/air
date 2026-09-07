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
      // The palette follows the platform brightness, and the two tiers only
      // differ in dark: in light both resolve to the same shade.
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

    testWidgets('lifts off the detail pane in the two-pane layout', (
      tester,
    ) async {
      sizeView(tester, desktopViewSize);
      await tester.pumpWidget(buildSubject());

      expect(
        fillOf(tester),
        darkSemanticPalette.backgroundElevated.secondary,
        reason: 'a module on the detail pane sits one elevation step above it',
      );
      // The pane is base.quinary, the same dark shade as base.secondary, so a
      // base-tier module would be invisible on it.
      expect(fillOf(tester), isNot(darkSemanticPalette.backgroundBase.quinary));
    });

    testWidgets('keeps the base tier on the phone', (tester) async {
      sizeView(tester, phoneViewSize);
      await tester.pumpWidget(buildSubject());

      expect(fillOf(tester), darkSemanticPalette.backgroundBase.secondary);
      expect(fillOf(tester), isNot(darkSemanticPalette.backgroundBase.primary));
    });
  });
}
