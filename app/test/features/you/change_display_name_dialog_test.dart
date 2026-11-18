// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/features/you/change_display_name_dialog.dart';
import 'package:material_ui/material_ui.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';

import '../../helpers.dart';

void main() {
  group('ChangeDisplayNameDialogTest', () {
    Widget buildSubject() => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: appLocalizationsDelegates,
          home: const ChangeDisplayNameDialog(displayName: "Ellie"),
        );
      },
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/change_display_name_dialog.png'),
      );
    });
  });
}
