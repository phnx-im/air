// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/user/contact_us_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:mocktail/mocktail.dart';

class MockUrlLauncher extends Mock implements UrlLauncher {}

void main() {
  group('ContactUsScreenTest', () {
    late UrlLauncher launcher;

    setUpAll(() {
      registerFallbackValue(Uri());
    });

    setUp(() {
      launcher = MockUrlLauncher();
    });

    Widget buildSubject({String? initialSubject, String? initialBody}) =>
        Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: themeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: ContactUsScreen(
                initialSubject: initialSubject,
                initialBody: initialBody,
                launcher: launcher,
              ),
            );
          },
        );

    testWidgets('empty renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_us_screen_empty.png'),
      );
    });

    testWidgets('input renders correctly', (tester) async {
      await tester.pumpWidget(
        buildSubject(initialSubject: "Other", initialBody: "Hello, World!"),
      );
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_us_screen_input.png'),
      );
    });

    testWidgets('validation renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject(initialBody: "Too short!"));

      when(() => launcher.launchUrl(any())).thenAnswer((_) async {});

      await tester.tap(find.byType(OutlinedButton));

      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/contact_us_screen_validation.png'),
      );
    });

    testWidgets('launcher is called correctly', (tester) async {
      await tester.pumpWidget(
        buildSubject(initialSubject: "Other", initialBody: "Fire! Fire! Fire!"),
      );
      await tester.pumpAndSettle();

      when(() => launcher.launchUrl(any())).thenAnswer((_) async {});

      await tester.tap(find.byType(OutlinedButton));

      verify(
        () => launcher.launchUrl(
          Uri.parse(
            "mailto:help@air.ms?subject=Other&body=Fire!%20Fire!%20Fire!",
          ),
        ),
      ).called(1);
    });
  });
}
