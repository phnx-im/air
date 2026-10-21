// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/core/core.dart';
import 'package:air/ds/components/button/button.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/features/you/delete_account_dialog.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:air/l10n/l10n.dart';
import 'package:mocktail/mocktail.dart';

import '../../helpers.dart';
import '../../mocks.dart';

void main() {
  group('DeleteAccountDialogTest', () {
    Widget buildSubject({bool isConfirmed = false}) => Builder(
      builder: (context) {
        return MaterialApp(
          debugShowCheckedModeBanner: false,
          theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
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

    Future<void> tapDelete(WidgetTester tester, MockUserCubit userCubit) async {
      await tester.pumpWidget(
        BlocProvider<UserCubit>.value(
          value: userCubit,
          child: buildSubject(isConfirmed: true),
        ),
      );
      await tester.tap(find.byType(Button).last);
      await tester.pumpAndSettle();
    }

    When<Future<UiAccountDeletionStatus>> whenDeleteAccount(
      MockUserCubit userCubit,
    ) => when(
      () => userCubit.deleteAccount(
        confirmationText: any(named: 'confirmationText'),
      ),
    );

    testWidgets('a failed deletion is reported in a dialog', (tester) async {
      final userCubit = MockUserCubit();
      whenDeleteAccount(userCubit).thenThrow(Exception('offline'));

      await tapDelete(tester, userCubit);

      expect(find.text('Something went wrong'), findsOneWidget);
      expect(find.text('Wait a moment, then try again.'), findsOneWidget);
    });

    testWidgets('a pending deletion is reported in a dialog', (tester) async {
      final userCubit = MockUserCubit();
      whenDeleteAccount(userCubit)
          .thenAnswer((_) async => UiAccountDeletionStatus.pending);

      await tapDelete(tester, userCubit);

      expect(find.text('Something went wrong'), findsOneWidget);
      expect(find.text('Wait a moment, then try again.'), findsOneWidget);
    });

    testWidgets('a completed deletion shows no dialog', (tester) async {
      final userCubit = MockUserCubit();
      whenDeleteAccount(userCubit)
          .thenAnswer((_) async => UiAccountDeletionStatus.completed);

      await tapDelete(tester, userCubit);

      expect(find.text('Something went wrong'), findsNothing);
    });
  });
}
