// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/modal/confirm_dialog.dart';
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
  });
}
