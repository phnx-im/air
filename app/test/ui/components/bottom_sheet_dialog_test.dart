// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/l10n/l10n.dart';
import 'package:air/ui/components/button/button.dart';
import 'package:air/ui/components/modal/bottom_sheet_modal.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

const _testSize = Size(400, 800);

void main() {
  group('BottomSheetDialogContent', () {
    Future<void> showModalAndCapture(
      WidgetTester tester, {
      required BottomSheetDialogContent content,
      required String goldenFile,
    }) async {
      tester.view.physicalSize = _testSize;
      tester.view.devicePixelRatio = 1.0;
      addTearDown(() {
        tester.view.resetPhysicalSize();
        tester.view.resetDevicePixelRatio();
      });

      await tester.pumpWidget(
        Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: Scaffold(
                body: Builder(
                  builder: (context) => Center(
                    child: ElevatedButton(
                      onPressed: () {
                        showBottomSheetModal(
                          context: context,
                          builder: (_) => content,
                        );
                      },
                      child: const Text('Show Modal'),
                    ),
                  ),
                ),
              ),
            );
          },
        ),
      );

      // Tap the button to show the modal
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
        content: const BottomSheetDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
        ),
        goldenFile: 'goldens/bottom_sheet_dialog_primary_only.png',
      );
    });

    testWidgets('renders primary danger only', (tester) async {
      await showModalAndCapture(
        tester,
        content: const BottomSheetDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          primaryTone: AppButtonTone.danger,
        ),
        goldenFile: 'goldens/bottom_sheet_dialog_primary_danger.png',
      );
    });

    testWidgets('renders two buttons', (tester) async {
      await showModalAndCapture(
        tester,
        content: const BottomSheetDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          secondaryActionText: 'Secondary Action',
        ),
        goldenFile: 'goldens/bottom_sheet_dialog_two_buttons.png',
      );
    });

    testWidgets('renders two danger secondary buttons', (tester) async {
      await showModalAndCapture(
        tester,
        content: const BottomSheetDialogContent(
          title: 'Title',
          description: 'Description text goes here.',
          primaryActionText: 'Primary Action',
          primaryType: AppButtonType.secondary,
          primaryTone: AppButtonTone.danger,
          secondaryActionText: 'Secondary Action',
          secondaryType: AppButtonType.secondary,
          secondaryTone: AppButtonTone.danger,
        ),
        goldenFile: 'goldens/bottom_sheet_dialog_two_danger_secondary.png',
      );
    });
  });
}
