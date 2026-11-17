// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/ds/patterns/modal/modal.dart';
import 'package:air/features/you/license_detail_pane.dart';
import 'package:air/features/you/licenses_modal.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _testSize = Size(600, 900);

/// Names that sort the same on every host, and mixed case so the list shows
/// the case-insensitive order.
final _entries = <LicenseEntry>[
  const LicenseEntryWithLineBreaks(['aircommon'], 'AGPL-3.0-or-later'),
  const LicenseEntryWithLineBreaks(['Flutter'], 'BSD-3-Clause'),
  const LicenseEntryWithLineBreaks(['openmls'], 'MIT'),
  const LicenseEntryWithLineBreaks(['openmls'], 'Apache-2.0'),
  const LicenseEntryWithLineBreaks(['zeroize'], 'MIT'),
];

/// A license whose first line is indented far enough for the registry's
/// heuristic to call it centered, plus one indented block.
const _licenseText =
    '                    MIT License\n'
    '\n'
    'Permission is hereby granted, free of charge, to any person obtaining a '
    'copy of this software.\n'
    '\n'
    '   The above notice shall be included in all copies.\n';

Widget _app(BuildContext context, Widget home) => MaterialApp(
  debugShowCheckedModeBanner: false,
  theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
  localizationsDelegates: AppLocalizations.localizationsDelegates,
  home: home,
);

void main() {
  group('LicensesModal', () {
    /// [settle] off for the loading state, whose spinner never stops
    /// animating.
    Future<void> pumpModal(
      WidgetTester tester, {
      Stream<LicenseEntry>? licenses,
      bool settle = true,
    }) async {
      sizeView(tester, _testSize);
      await tester.pumpWidget(
        Builder(
          builder: (context) => _app(
            context,
            LicensesModal(licenses: licenses ?? Stream.fromIterable(_entries)),
          ),
        ),
      );
      if (settle) {
        await tester.pumpAndSettle();
      } else {
        await tester.pump();
      }
    }

    Future<void> search(WidgetTester tester, String query) async {
      await tester.enterText(find.byType(TextField), query);
      await tester.pumpAndSettle();
    }

    Future<void> open(WidgetTester tester, String package) async {
      await tester.tap(find.text(package));
      await tester.pumpAndSettle();
    }

    Future<void> expectGolden(WidgetTester tester, String fileName) =>
        expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/$fileName.png'),
        );

    testWidgets('renders the package list', (tester) async {
      await pumpModal(tester);

      expect(find.text('aircommon'), findsOneWidget);
      expect(find.text('Flutter'), findsOneWidget);
      expect(find.text('openmls'), findsOneWidget);
      expect(find.text('zeroize'), findsOneWidget);
      expect(find.text('4 packages'), findsOneWidget);

      await expectGolden(tester, 'licenses_screen');
    });

    testWidgets('renders the spinner while collecting', (tester) async {
      final pending = StreamController<LicenseEntry>();
      addTearDown(pending.close);
      await pumpModal(tester, licenses: pending.stream, settle: false);

      expect(find.byType(CircularProgressIndicator), findsOneWidget);
      // The count line waits for the list: a zero under a spinner reads as an
      // empty registry.
      expect(find.text('No packages'), findsNothing);
    });

    testWidgets('shows only the packages the query matches', (tester) async {
      await pumpModal(tester);
      await search(tester, 'ml');

      expect(find.text('openmls'), findsOneWidget);
      expect(find.text('aircommon'), findsNothing);
      expect(find.text('1 package'), findsOneWidget);
    });

    testWidgets('reports an empty result', (tester) async {
      await pumpModal(tester);
      await search(tester, 'nothing');

      expect(find.text('No packages found.'), findsOneWidget);
    });

    testWidgets('tapping a package opens its licenses', (tester) async {
      await pumpModal(tester);
      await open(tester, 'openmls');

      expect(find.byType(LicenseDetailPane), findsOneWidget);
      expect(find.byType(Divider), findsOneWidget);
    });

    testWidgets('going back keeps the search', (tester) async {
      await pumpModal(tester);
      await search(tester, 'ml');
      await open(tester, 'openmls');

      await tester.tap(find.byType(DialogHeaderAction));
      await tester.pumpAndSettle();

      expect(find.byType(LicenseDetailPane), findsNothing);
      expect(find.text('1 package'), findsOneWidget);
    });

    testWidgets('renders the license detail', (tester) async {
      await pumpModal(
        tester,
        licenses: Stream.fromIterable(const [
          LicenseEntryWithLineBreaks(['openmls'], _licenseText),
          LicenseEntryWithLineBreaks(['openmls'], 'MIT'),
        ]),
      );
      await open(tester, 'openmls');

      expect(find.text('MIT License'), findsOneWidget);
      expect(find.byType(Divider), findsOneWidget);

      await expectGolden(tester, 'licenses_detail');
    });
  });

  group('showLicenses', () {
    Widget buildHost() => Builder(
      builder: (context) => _app(
        context,
        Builder(
          builder: (context) => Center(
            child: TextButton(
              onPressed: () => showLicenses(context),
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );

    Future<void> openModal(WidgetTester tester) async {
      await tester.pumpWidget(buildHost());
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
    }

    testWidgets('closing returns to the section', (tester) async {
      sizeView(tester, phoneViewSize);
      await openModal(tester);

      await tester.tap(find.byType(DialogHeaderAction));
      await tester.pumpAndSettle();

      expect(find.byType(LicensesModal), findsNothing);
      expect(find.text('Open'), findsOneWidget);
    });

    testWidgets('fills the screen on a phone', (tester) async {
      sizeView(tester, phoneViewSize);
      await openModal(tester);

      expect(find.byType(LicensesModal), findsOneWidget);
      expectModalFillsViewport(tester, phoneViewSize);
    });

    // A route push would cover the window whole, sidebar and window chrome
    // included, so desktop gets a card over what it came from instead.
    testWidgets('presents a card on desktop', (tester) async {
      sizeView(tester, desktopViewSize);
      await openModal(tester);

      expect(find.byType(LicensesModal), findsOneWidget);
      expectModalIsCard(tester, desktopViewSize);
    }, variant: desktopPlatform);
  });
}
