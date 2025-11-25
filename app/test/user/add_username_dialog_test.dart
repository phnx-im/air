// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/user/add_username_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('AddUsernameDialogTest', () {
    Widget buildSubject({bool inProgress = false}) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: themeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: AddUsernameDialog(inProgress: inProgress),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      final buttonFinder = find.byType(OutlinedButton);
      expect(buttonFinder, findsNWidgets(2));

      final Size size1 = tester.getSize(buttonFinder.first);
      final Size size2 = tester.getSize(buttonFinder.last);
      expect(size1, size2);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/add_username_dialog.png'),
      );
    });

    testWidgets('renders correctly (adding)', (tester) async {
      await tester.pumpWidget(buildSubject(inProgress: true));

      final buttonFinder = find.byType(OutlinedButton);
      expect(buttonFinder, findsNWidgets(2));

      final Size size1 = tester.getSize(buttonFinder.first);
      final Size size2 = tester.getSize(buttonFinder.last);
      expect(size1, size2);

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/add_username_dialog_adding.png'),
      );
    });
  });
}
