// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'dart:async';

import 'package:air/features/you/license_detail_screen.dart';
import 'package:air/features/you/license_packages.dart';
import 'package:air/features/you/licenses_screen.dart';
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
  group('LicensesScreenView', () {
    /// [settle] off for the loading state, whose spinner never stops
    /// animating.
    Future<void> pumpScreen(
      WidgetTester tester, {
      Stream<LicenseEntry>? licenses,
      bool settle = true,
    }) async {
      sizeView(tester, _testSize);
      await tester.pumpWidget(
        Builder(
          builder: (context) => _app(
            context,
            LicensesScreenView(
              licenses: licenses ?? Stream.fromIterable(_entries),
            ),
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

    Future<void> expectGolden(WidgetTester tester, String fileName) =>
        expectLater(
          find.byType(MaterialApp),
          matchesGoldenFile('goldens/$fileName.png'),
        );

    testWidgets('renders the package list', (tester) async {
      await pumpScreen(tester);

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
      await pumpScreen(tester, licenses: pending.stream, settle: false);

      expect(find.byType(CircularProgressIndicator), findsOneWidget);
      // The count line waits for the list: a zero under a spinner reads as an
      // empty registry.
      expect(find.text('No packages'), findsNothing);
    });

    testWidgets('shows only the packages the query matches', (tester) async {
      await pumpScreen(tester);
      await search(tester, 'ml');

      expect(find.text('openmls'), findsOneWidget);
      expect(find.text('aircommon'), findsNothing);
      expect(find.text('1 package'), findsOneWidget);
    });

    testWidgets('reports an empty result', (tester) async {
      await pumpScreen(tester);
      await search(tester, 'nothing');

      expect(find.text('No packages found.'), findsOneWidget);
    });

    testWidgets('tapping a package opens its licenses', (tester) async {
      await pumpScreen(tester);

      await tester.tap(find.text('openmls'));
      await tester.pumpAndSettle();

      expect(find.byType(LicenseDetailScreenView), findsOneWidget);
      expect(find.byType(Divider), findsOneWidget);
    });
  });

  group('LicenseDetailScreenView', () {
    testWidgets('renders the license detail', (tester) async {
      sizeView(tester, _testSize);
      await tester.pumpWidget(
        Builder(
          builder: (context) => _app(
            context,
            const LicenseDetailScreenView(
              package: LicensePackage(
                name: 'openmls',
                entries: [
                  LicenseEntryWithLineBreaks(['openmls'], _licenseText),
                  LicenseEntryWithLineBreaks(['openmls'], 'MIT'),
                ],
              ),
            ),
          ),
        ),
      );
      await tester.pumpAndSettle();

      expect(find.text('MIT License'), findsOneWidget);
      expect(find.byType(Divider), findsOneWidget);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/licenses_detail.png'),
      );
    });
  });
}
