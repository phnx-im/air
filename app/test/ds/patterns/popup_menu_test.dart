// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/components/menu/menu.dart';
import 'package:air/ds/patterns/popup_menu/popup_menu.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('showOverlayMenu', () {
    Future<FocusNode> focusField(WidgetTester tester) async {
      final focus = FocusNode();
      addTearDown(focus.dispose);
      await tester.pumpWidget(
        MaterialApp(
          theme: testLightTheme,
          home: Scaffold(body: TextField(focusNode: focus)),
        ),
      );
      await tester.showKeyboard(find.byType(TextField));
      return focus;
    }

    Future<void> openMenu(WidgetTester tester) async {
      unawaited(
        showOverlayMenu(
          context: tester.element(find.byType(TextField)),
          anchor: Rect.zero,
          items: [MenuItem(label: 'Item', onPressed: () {})],
        ),
      );
      await tester.pumpAndSettle();
      expect(find.byType(Menu), findsOneWidget);
    }

    testWidgets('takes the focus and closes on Escape', (tester) async {
      final focus = await focusField(tester);

      await openMenu(tester);
      expect(focus.hasFocus, isFalse);

      await tester.sendKeyEvent(LogicalKeyboardKey.escape);
      await tester.pumpAndSettle();
      expect(find.byType(Menu), findsNothing);
    });

    testWidgets('leaves the focus with the soft keyboard up', (tester) async {
      final focus = await focusField(tester);
      raiseKeyboard(tester, 336);
      await tester.pump();

      await openMenu(tester);
      expect(focus.hasFocus, isTrue);
      expect(tester.testTextInput.isVisible, isTrue);
    });
  });
}
