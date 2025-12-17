// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/user/update_required_screen.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';

void main() {
  group('ContactUsScreenTest', () {
    Widget buildSubject({String? initialSubject, String? initialBody}) =>
        Builder(
          builder: (context) {
            return MaterialApp(
              debugShowCheckedModeBanner: false,
              theme: themeData(MediaQuery.platformBrightnessOf(context)),
              localizationsDelegates: AppLocalizations.localizationsDelegates,
              home: const UpdateRequiredView(showUpdateButton: true),
            );
          },
        );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/update_required_screen.png'),
      );
    });
  });
}
