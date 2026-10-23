// SPDX-FileCopyrightText: 2026 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:air/app.dart';
import 'package:air/core/core.dart';
import 'package:air/features/user/account_deletion_abort_handler.dart';
import 'package:air/features/user/user_cubit.dart';
import 'package:air/l10n/l10n.dart';
import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';

import '../../mocks.dart';

const _notice = "Your Air account couldn't be deleted. You can try again.";

void main() {
  Future<MockUserCubit> pumpInApp(
    WidgetTester tester, {
    required UiUser initial,
    UiUser? update,
  }) async {
    final userCubit = MockUserCubit();
    when(() => userCubit.state).thenReturn(initial);
    if (update != null) {
      when(() => userCubit.stream).thenAnswer((_) => Stream.value(update));
    }
    when(() => userCubit.accountDeletionAbortReported())
        .thenAnswer((_) async {});

    await tester.pumpWidget(
      MaterialApp(
        scaffoldMessengerKey: scaffoldMessengerKey,
        localizationsDelegates: AppLocalizations.localizationsDelegates,
        home: BlocProvider<UserCubit>.value(
          value: userCubit,
          child: const AccountDeletionAbortHandler(child: Scaffold()),
        ),
      ),
    );
    await tester.pumpAndSettle();
    return userCubit;
  }

  testWidgets('reports an abort recorded before the app started', (
    tester,
  ) async {
    final userCubit = await pumpInApp(
      tester,
      initial: MockUiUser(id: 1, accountDeletionAborted: true),
    );

    expect(find.text(_notice), findsOneWidget);
    verify(() => userCubit.accountDeletionAbortReported()).called(1);
  });

  testWidgets('reports an abort in the background', (tester) async {
    final userCubit = await pumpInApp(
      tester,
      initial: MockUiUser(id: 1),
      update: MockUiUser(id: 1, accountDeletionAborted: true),
    );

    expect(find.text(_notice), findsOneWidget);
    verify(() => userCubit.accountDeletionAbortReported()).called(1);
  });

  testWidgets('stays quiet without an abort', (tester) async {
    final userCubit = await pumpInApp(tester, initial: MockUiUser(id: 1));

    expect(find.byType(SnackBar), findsNothing);
    verifyNever(() => userCubit.accountDeletionAbortReported());
  });
}
