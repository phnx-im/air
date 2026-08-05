// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/ds/patterns/adaptive_modal/adaptive_modal.dart';
import 'package:air/ds/patterns/dialog/app_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _testSize = Size(400, 800);

Widget _host(Widget content) => Builder(
  builder: (context) => MaterialApp(
    debugShowCheckedModeBanner: false,
    theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
    localizationsDelegates: AppLocalizations.localizationsDelegates,
    home: Scaffold(
      body: Builder(
        builder: (context) => Center(
          child: ElevatedButton(
            onPressed: () =>
                showAdaptiveModal(context: context, builder: (_) => content),
            child: const Text('Show Modal'),
          ),
        ),
      ),
    ),
  ),
);

void main() {
  group('AdaptiveDialogContent', () {
    Future<void> showModalAndCapture(
      WidgetTester tester, {
      required AdaptiveDialogContent content,
      required String goldenFile,
    }) async {
      sizeView(tester, _testSize);
      await tester.pumpWidget(_host(content));

      await tester.tap(find.text('Show Modal'));
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile(goldenFile),
      );
    }

    testWidgets('renders primary only', (tester) async {
      await showModalAndCapture(
        tester,
        content: const AdaptiveDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
        ),
        goldenFile: 'goldens/adaptive_modal_sheet_primary_only.png',
      );
    });

    testWidgets('renders primary danger only', (tester) async {
      await showModalAndCapture(
        tester,
        content: const AdaptiveDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          primaryTone: ButtonTone.danger,
        ),
        goldenFile: 'goldens/adaptive_modal_sheet_primary_danger.png',
      );
    });

    testWidgets('renders two buttons', (tester) async {
      await showModalAndCapture(
        tester,
        content: const AdaptiveDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          secondaryActionText: 'Secondary Action',
        ),
        goldenFile: 'goldens/adaptive_modal_sheet_two_buttons.png',
      );
    });

    testWidgets('renders two danger secondary buttons', (tester) async {
      await showModalAndCapture(
        tester,
        content: const AdaptiveDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          primaryType: ButtonType.secondary,
          primaryTone: ButtonTone.danger,
          secondaryActionText: 'Secondary Action',
          secondaryType: ButtonType.secondary,
          secondaryTone: ButtonTone.danger,
        ),
        goldenFile: 'goldens/adaptive_modal_sheet_two_danger_secondary.png',
      );
    });
  });

  group('showAdaptiveModal', () {
    Future<void> open(WidgetTester tester) async {
      await tester.pumpWidget(_host(const Text('modal body')));
      await tester.tap(find.text('Show Modal'));
      await tester.pumpAndSettle();
    }

    testWidgets('presents a bottom sheet on a phone', (tester) async {
      sizeView(tester, phoneViewSize);
      await open(tester);

      expect(find.text('modal body'), findsOneWidget);
      expect(find.byType(AppDialog), findsNothing);
      // Bottom-anchored, where the dialog card the desktop takes is centered.
      expect(
        tester.getCenter(find.text('modal body')).dy,
        greaterThan(phoneViewSize.height / 2),
      );
    });

    testWidgets('presents a dialog card on desktop', (tester) async {
      sizeView(tester, desktopViewSize);
      await open(tester);

      expect(find.byType(AppDialog), findsOneWidget);
      expect(find.text('modal body'), findsOneWidget);
    }, variant: desktopPlatform);

    // The sheet is a touch idiom, so a desktop window narrow enough to sit in
    // the small breakpoint still gets the card.
    testWidgets(
      'presents a dialog card on desktop in a narrow window',
      (tester) async {
        sizeView(tester, phoneViewSize);
        await open(tester);

        expect(find.byType(AppDialog), findsOneWidget);
      },
      variant: desktopPlatform,
    );
  });
}
