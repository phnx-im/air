// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/ds/components/modal/edit_dialog.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import '../../helpers.dart';

void main() {
  group('EditDialog', () {
    Widget buildSubject(EditDialog dialog) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
          localizationsDelegates: AppLocalizations.localizationsDelegates,
          home: dialog,
        );
      },
    );

    testWidgets('renders basic', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          EditDialog(
            title: 'Change device name',
            cancel: 'Cancel',
            confirm: 'Change',
            initialValue: 'Linux',
            validator: (value) => value.trim().isNotEmpty,
            onSubmit: (_) {},
          ),
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/edit_dialog_basic.png'),
      );
    });

    testWidgets('renders with character counter', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          EditDialog(
            title: 'Change device name',
            cancel: 'Cancel',
            confirm: 'Change',
            initialValue: 'Linux',
            maxLength: 30,
            validator: (value) => value.trim().isNotEmpty,
            onSubmit: (_) {},
          ),
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/edit_dialog_with_counter.png'),
      );
    });

    testWidgets('renders with description', (tester) async {
      await tester.pumpWidget(
        buildSubject(
          EditDialog(
            title: 'Change device name',
            description: 'This name is only visible to you.',
            cancel: 'Cancel',
            confirm: 'Change',
            initialValue: 'Linux',
            validator: (value) => value.trim().isNotEmpty,
            onSubmit: (_) {},
          ),
        ),
      );
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/edit_dialog_with_description.png'),
      );
    });
  });
}
