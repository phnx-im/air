// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/ds/foundations/foundations.dart';
import '../../helpers.dart';

void main() {
  group('Button', () {
    String sizeLabel(ButtonSize size) =>
        size == ButtonSize.small ? 'Small' : 'Large';

    List<Widget> buildButtonConfigs() => [
      for (final size in ButtonSize.values) ...[
        Button(
          label: "${sizeLabel(size)} Primary",
          size: size,
          type: ButtonType.primary,
          state: ButtonState.active,
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Primary Inactive",
          size: size,
          type: ButtonType.primary,
          state: ButtonState.inactive,
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Primary Danger",
          size: size,
          type: ButtonType.primary,
          state: ButtonState.active,
          tone: ButtonTone.danger,
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Primary Icon",
          size: size,
          type: ButtonType.primary,
          state: ButtonState.active,
          icon: (size, color) =>
              Container(width: size.width, height: size.height, color: color),
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Secondary",
          size: size,
          type: ButtonType.secondary,
          state: ButtonState.active,
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Secondary Inactive",
          size: size,
          type: ButtonType.secondary,
          state: ButtonState.inactive,
          onPressed: () {},
        ),
        Button(
          label: "${sizeLabel(size)} Secondary Danger",
          size: size,
          type: ButtonType.secondary,
          tone: ButtonTone.danger,
          onPressed: () {},
        ),
        const SizedBox(height: S.s16),
      ],
    ];

    Widget buildSubject(List<Widget> widgets) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: Scaffold(
            body: Padding(
              padding: const EdgeInsets.all(S.s16),
              child: Column(spacing: S.s8, children: widgets),
            ),
          ),
        );
      },
    );

    testWidgets('buttons render correctly', (tester) async {
      await tester.pumpWidget(buildSubject(buildButtonConfigs()));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/buttons.png'),
      );
    });

    testWidgets('buttons render correctly (dark mode)', (tester) async {
      tester.platformDispatcher.platformBrightnessTestValue = Brightness.dark;
      addTearDown(() {
        tester.platformDispatcher.clearPlatformBrightnessTestValue();
      });

      await tester.pumpWidget(buildSubject(buildButtonConfigs()));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/buttons_dark.png'),
      );
    });
  });
}
