// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/api/types.dart';
import 'package:air/user/remove_username_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('RemoveUsernameDialogTest', () {
    Widget buildSubject() => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: themeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: const RemoveUsernameDialog(
            username: UiUserHandle(plaintext: "ellie"),
          ),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/remove_username_dialog.png'),
      );
    });
  });
}
