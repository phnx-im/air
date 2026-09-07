// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/features/developer/change_user_modal.dart';
import 'package:air/features/user/user_session_cubit.dart';
import 'package:air/features/user/users_cubit.dart';

import '../../helpers.dart';
import '../../mocks.dart';

final clientRecords = [
  UiClientRecord(
    clientRecordId: 1.clientRecordId(),
    userId: 1.userId(),
    createdAt: DateTime.parse("2023-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 1.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    clientRecordId: 2.clientRecordId(),
    userId: 2.userId(),
    createdAt: DateTime.parse("2024-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 2.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    clientRecordId: 3.clientRecordId(),
    userId: 3.userId(),
    createdAt: DateTime.parse("2025-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 3.userId(), displayName: "bob"),
    isFinished: false,
  ),
];

void main() {
  group('ChangeUserModal', () {
    late MockUser user;
    late MockUserSessionCubit userSessionCubit;
    late MockUsersCubit usersCubit;

    setUp(() async {
      user = MockUser();
      usersCubit = MockUsersCubit();
      userSessionCubit = MockUserSessionCubit();

      when(() => user.userId).thenReturn(1.userId());
      when(() => user.clientRecordId).thenReturn(1.clientRecordId());
      when(
        () => userSessionCubit.state,
      ).thenReturn(UserSessionState(user: user));
      when(() => usersCubit.state).thenReturn(
        MockUsersState(
          defaultUserId: 1.userId(),
          profiles: clientRecords.map((record) => record.userProfile).toList(),
        ),
      );
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<UserSessionCubit>.value(value: userSessionCubit),
        BlocProvider<UsersCubit>.value(value: usersCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: testThemeData(MediaQuery.platformBrightnessOf(context)),
            home: ChangeUserModal(clientRecords: Future.value(clientRecords)),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/change_user_modal.png'),
      );
    });
  });
}
