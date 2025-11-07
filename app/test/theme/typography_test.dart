// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('Typography', () {
    Widget buildSubject(Widget widget) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: themeData(MediaQuery.platformBrightnessOf(context)),
          home: Scaffold(
            body: SafeArea(
              child: Padding(
                padding: const EdgeInsets.symmetric(vertical: Spacings.xs),
                child: Center(child: widget),
              ),
            ),
          ),
        );
      },
    );

    testWidgets('font styles', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          Builder(
            builder: (context) {
              return Column(
                children: [
                  Text(
                    "Display Large",
                    style: Theme.of(context).textTheme.displayLarge,
                  ),
                  Text(
                    "Display Medium",
                    style: Theme.of(context).textTheme.displayMedium,
                  ),
                  Text(
                    "Display Small",
                    style: Theme.of(context).textTheme.displaySmall,
                  ),
                  Text(
                    "Headline Large",
                    style: Theme.of(context).textTheme.headlineLarge,
                  ),
                  Text(
                    "Headline Medium",
                    style: Theme.of(context).textTheme.headlineMedium,
                  ),
                  Text(
                    "Headline Small",
                    style: Theme.of(context).textTheme.headlineSmall,
                  ),
                  Text(
                    "Title Large",
                    style: Theme.of(context).textTheme.titleLarge,
                  ),
                  Text(
                    "Title Medium",
                    style: Theme.of(context).textTheme.titleMedium,
                  ),
                  Text(
                    "Title Small",
                    style: Theme.of(context).textTheme.titleSmall,
                  ),
                  Text(
                    "Body Large",
                    style: Theme.of(context).textTheme.bodyLarge,
                  ),
                  Text(
                    "Body Medium",
                    style: Theme.of(context).textTheme.bodyMedium,
                  ),
                  Text(
                    "Body Small",
                    style: Theme.of(context).textTheme.bodySmall,
                  ),
                  Text(
                    "Label Large",
                    style: Theme.of(context).textTheme.labelLarge,
                  ),
                  Text(
                    "Label Medium",
                    style: Theme.of(context).textTheme.labelMedium,
                  ),
                  Text(
                    "Label Small",
                    style: Theme.of(context).textTheme.labelSmall,
                  ),
                ],
              );
            },
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/typography_font_styles.png'),
      );
    });
  });
}
