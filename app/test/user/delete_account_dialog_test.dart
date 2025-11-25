// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/user/delete_account_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('DeleteAccountDialogTest', () {
    Widget buildSubject({bool isConfirmed = false}) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: themeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: DeleteAccountDialog(isConfirmed: isConfirmed),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/delete_account_dialog.png'),
      );
    });

    testWidgets('renders correctly (confirmed)', (tester) async {
      await tester.pumpWidget(buildSubject(isConfirmed: true));

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/delete_account_dialog_confirmed.png'),
      );
    });
  });
}
