// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ui/components/button/button.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('AppButton', () {
    Widget buildSubject(List<Widget> widgets) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: themeData(MediaQuery.platformBrightnessOf(context)),
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
      final configs = [
        for (final size in AppButtonSize.values) ...[
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.primary,
            state: AppButtonState.active,
            onPressed: () {},
          ),
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.primary,
            state: AppButtonState.inactive,
            onPressed: () {},
          ),
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.primary,
            state: AppButtonState.danger,
            onPressed: () {},
          ),
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.primary,
            state: AppButtonState.active,
            icon: (size, color) =>
                Container(width: size.width, height: size.height, color: color),
            onPressed: () {},
          ),
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.secondary,
            state: AppButtonState.active,
            onPressed: () {},
          ),
          AppButton(
            label: "Label",
            size: size,
            type: AppButtonType.secondary,
            state: AppButtonState.active,
            icon: (size, color) =>
                Container(width: size.width, height: size.height, color: color),
            onPressed: () {},
          ),
          const SizedBox(height: Spacings.s),
        ],
      ];

      await tester.pumpWidget(buildSubject(configs));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/buttons.png'),
      );
    });
  });
}
