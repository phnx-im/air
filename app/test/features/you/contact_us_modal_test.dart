// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/button/button.dart';
import 'package:air/features/you/contact_us_modal.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:mocktail/mocktail.dart';
import '../../helpers.dart';

class MockUrlLauncher extends Mock implements UrlLauncher {}

void main() {
  group('ContactUsModalTest', () {
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
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: ContactUsModal(
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

      await tester.tap(find.byType(Button));

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

      await tester.tap(find.byType(Button));

      verify(
        () => launcher.launchUrl(
          Uri.parse(
            "mailto:help@air.ms?subject=Other&body=Fire!%20Fire!%20Fire!",
          ),
        ),
      ).called(1);
    });
  });

  group('showContactUs', () {
    Widget buildHost() => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: Builder(
            builder: (context) => ElevatedButton(
              onPressed: () => showContactUs(context),
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );

    Future<void> open(WidgetTester tester) async {
      await tester.pumpWidget(buildHost());
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
    }

    testWidgets('fills the screen on a phone', (tester) async {
      sizeView(tester, phoneViewSize);
      await open(tester);

      expect(find.byType(ContactUsModal), findsOneWidget);
      expectModalFillsViewport(tester, phoneViewSize);
    });

    // A route push would cover the window whole, sidebar and window chrome
    // included, so desktop gets a card over what it came from instead.
    testWidgets('presents a card on desktop', (tester) async {
      sizeView(tester, desktopViewSize);
      await open(tester);

      expect(find.byType(ContactUsModal), findsOneWidget);
      expectModalIsCard(tester, desktopViewSize);
    }, variant: desktopPlatform);

    // A desktop window narrow enough to sit in the small tier still gets the
    // card: nothing reserves the window's own chrome on a full-bleed surface.
    testWidgets('presents a card in a narrow desktop window', (tester) async {
      sizeView(tester, phoneViewSize);
      await open(tester);

      expectModalIsCard(tester, phoneViewSize);
    }, variant: desktopPlatform);

    // No scaffold resizes the modal for the keyboard, so the scroll area has
    // to carry the room it takes or the compose button stays under it.
    testWidgets('scrolls clear of a raised keyboard', (tester) async {
      // Short enough that the form only clears the keyboard by scrolling.
      const viewSize = Size(400, 600);
      const keyboard = 300.0;
      sizeView(tester, viewSize);
      tester.view.viewInsets = FakeViewPadding(
        bottom: keyboard * tester.view.devicePixelRatio,
      );
      addTearDown(tester.view.resetViewInsets);

      await open(tester);
      await tester.ensureVisible(find.byType(Button));
      await tester.pumpAndSettle();

      expect(
        tester.getRect(find.byType(Button)).bottom,
        lessThanOrEqualTo(viewSize.height - keyboard),
      );
    });

    // A window too short for the form scrolls it inside the card, which is
    // what the card overflowing instead would report as an error here.
    testWidgets('scrolls the form in a short window', (tester) async {
      sizeView(tester, const Size(1200, 400));
      await open(tester);

      expect(find.byType(ContactUsModal), findsOneWidget);
      expect(find.text('Compose email'), findsOneWidget);
    }, variant: desktopPlatform);
  });
}
