// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/components/button/button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import '../../helpers.dart';

void main() {
  group('AppButton', () {
    String sizeLabel(AppButtonSize size) =>
        size == AppButtonSize.small ? 'Small' : 'Large';

    List<Widget> buildButtonConfigs() => [
      for (final size in AppButtonSize.values) ...[
        AppButton(
          label: "${sizeLabel(size)} Primary",
          size: size,
          type: AppButtonType.primary,
          state: AppButtonState.active,
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Primary Inactive",
          size: size,
          type: AppButtonType.primary,
          state: AppButtonState.inactive,
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Primary Danger",
          size: size,
          type: AppButtonType.primary,
          state: AppButtonState.active,
          tone: AppButtonTone.danger,
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Primary Icon",
          size: size,
          type: AppButtonType.primary,
          state: AppButtonState.active,
          icon: (size, color) =>
              Container(width: size.width, height: size.height, color: color),
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Secondary",
          size: size,
          type: AppButtonType.secondary,
          state: AppButtonState.active,
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Secondary Inactive",
          size: size,
          type: AppButtonType.secondary,
          state: AppButtonState.inactive,
          onPressed: () {},
        ),
        AppButton(
          label: "${sizeLabel(size)} Secondary Danger",
          size: size,
          type: AppButtonType.secondary,
          tone: AppButtonTone.danger,
          onPressed: () {},
        ),
        const SizedBox(height: Spacings.s),
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
              padding: const EdgeInsets.all(Spacings.s),
              child: Column(spacing: Spacings.xxs, children: widgets),
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
