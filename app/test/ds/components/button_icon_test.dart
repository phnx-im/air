// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button_icon/button_icon.dart';
import 'package:air/ds/components/button_icon/button_icon_tokens.dart';
import 'package:air/ds/foundations/foundations.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('ButtonIcon', () {
    const size = ButtonIconSize.s32;
    const hitTargetSize = 48.0;

    Widget buildSubject({VoidCallback? onPressed, double? hitTarget}) =>
        MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testLightTheme,
          home: Scaffold(
            body: Center(
              child: ButtonIcon(
                variant: ButtonIconVariant.solid,
                icon: AppIconType.plus,
                size: size,
                hitTargetSize: hitTarget,
                onPressed: onPressed,
              ),
            ),
          ),
        );

    testWidgets('fires once for a tap on the visible circle', (tester) async {
      var taps = 0;
      await tester.pumpWidget(
        buildSubject(onPressed: () => taps++, hitTarget: hitTargetSize),
      );

      await tester.tap(find.byType(ButtonIcon));
      await tester.pumpAndSettle();

      // The ring around the circle carries its own detector, so a tap in the
      // middle must not be handled by both.
      expect(taps, 1);
    });

    testWidgets('fires once for a tap on the surrounding hit target', (
      tester,
    ) async {
      var taps = 0;
      await tester.pumpWidget(
        buildSubject(onPressed: () => taps++, hitTarget: hitTargetSize),
      );

      // Inside the hit target, outside the circle.
      final rect = tester.getRect(find.byType(ButtonIcon));
      await tester.tapAt(Offset(rect.left + 2, rect.center.dy));
      await tester.pumpAndSettle();

      expect(taps, 1);
    });

    testWidgets('takes the whole hit target as its footprint', (tester) async {
      await tester.pumpWidget(
        buildSubject(onPressed: () {}, hitTarget: hitTargetSize),
      );

      expect(
        tester.getSize(find.byType(ButtonIcon)),
        const Size.square(hitTargetSize),
      );
    });

    testWidgets('is its visible size without a hit target', (tester) async {
      await tester.pumpWidget(buildSubject(onPressed: () {}));

      expect(tester.getSize(find.byType(ButtonIcon)), const Size.square(size));
    });

    testWidgets('fades the glyph without a handler', (tester) async {
      await tester.pumpWidget(buildSubject(onPressed: () {}));
      final enabled = tester.widget<AppIcon>(find.byType(AppIcon)).color!.a;

      await tester.pumpWidget(buildSubject());
      final disabled = tester.widget<AppIcon>(find.byType(AppIcon)).color!.a;

      expect(
        disabled,
        moreOrLessEquals(
          enabled * ButtonIconTokens.disabledOpacity,
          epsilon: 0.001,
        ),
      );
    });

    testWidgets('pairs the glyph size with the button size', (tester) async {
      await tester.pumpWidget(buildSubject(onPressed: () {}));

      final glyph = tester.widget<AppIcon>(find.byType(AppIcon));
      expect(glyph.size, ButtonIconSize.glyphFor(size));
    });
  });
}
