// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/patterns/confirm_dialog/confirm_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('ConfirmDialog', () {
    Widget buildSubject(ConfirmDialog dialog) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: dialog,
        );
      },
    );

    testWidgets('renders confirm only', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          const ConfirmDialog(
            title: 'Encryption and device linking',
            message:
                'On Air, your messages are always end-to-end encrypted. '
                'Nobody else, even Air, can read them.',
            confirm: 'Okay',
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/confirm_dialog_confirm_only.png'),
      );
    });

    testWidgets('renders with cancel', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          const ConfirmDialog(
            title: 'Unlink device',
            message:
                'The device will no longer be able to send or receive '
                'messages.',
            cancel: 'Cancel',
            confirm: 'Continue',
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/confirm_dialog_with_cancel.png'),
      );
    });

    testWidgets('renders destructive', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          const ConfirmDialog(
            title: 'Unlink device',
            message:
                'The device will no longer be able to send or receive '
                'messages. All of your account\'s data will be deleted from '
                'the device.',
            cancel: 'Cancel',
            confirm: 'Unlink',
            destructive: true,
          ),
        ),
      );

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/confirm_dialog_destructive.png'),
      );
    });

    testWidgets('a dialog shown by onConfirm stays open', (tester) async {
      await tester.pumpWidget(
        Builder(
          builder: (context) => MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: Scaffold(
              body: Builder(
                builder: (context) => ElevatedButton(
                  onPressed: () => showDialog(
                    context: context,
                    builder: (_) => ConfirmDialog(
                      title: 'Unlink device',
                      confirm: 'Unlink',
                      onConfirm: () => showErrorDialog(
                        context,
                        message: (_) => 'Unlinking failed.',
                      ),
                    ),
                  ),
                  child: const Text('Open'),
                ),
              ),
            ),
          ),
        ),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();

      await tester.tap(find.text('Unlink'));
      await tester.pumpAndSettle();

      expect(find.text('Unlink device'), findsNothing);
      expect(find.text('Unlinking failed.'), findsOneWidget);
    });
  });

  group('showErrorDialog', () {
    Widget buildHost({String Function(AppLocalizations)? title}) => Builder(
      builder: (context) => MaterialApp(
        debugShowCheckedModeBanner: false,
        theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: Scaffold(
          body: Builder(
            builder: (context) => ElevatedButton(
              onPressed: () => showErrorDialog(
                context,
                title: title,
                message: (_) => 'Try again later.',
              ),
              child: const Text('Open'),
            ),
          ),
        ),
      ),
    );

    Future<void> open(WidgetTester tester, {String? title}) async {
      await tester.pumpWidget(
        buildHost(title: title == null ? null : (_) => title),
      );
      await tester.tap(find.text('Open'));
      await tester.pumpAndSettle();
    }

    testWidgets('shows the default title, message and confirm', (tester) async {
      await open(tester);

      expect(find.byType(ConfirmDialog), findsOneWidget);
      expect(find.text('Something went wrong'), findsOneWidget);
      expect(find.text('Try again later.'), findsOneWidget);
      expect(find.text('Okay'), findsOneWidget);
    });

    testWidgets('a custom title replaces the default', (tester) async {
      await open(tester, title: 'Renaming failed.');

      expect(find.text('Renaming failed.'), findsOneWidget);
      expect(find.text('Something went wrong'), findsNothing);
    });

    testWidgets('confirming dismisses the dialog', (tester) async {
      await open(tester);

      await tester.tap(find.text('Okay'));
      await tester.pumpAndSettle();

      expect(find.byType(ConfirmDialog), findsNothing);
    });
  });
}
