// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/developer/developer.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../helpers.dart';
import '../mocks.dart';

final clientRecords = [
  UiClientRecord(
    userId: 1.userId(),
    createdAt: DateTime.parse("2023-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 1.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    userId: 2.userId(),
    createdAt: DateTime.parse("2024-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 2.userId(), displayName: "alice"),
    isFinished: true,
  ),
  UiClientRecord(
    userId: 3.userId(),
    createdAt: DateTime.parse("2025-01-01T00:00:00.000Z"),
    userProfile: UiUserProfile(userId: 3.userId(), displayName: "bob"),
    isFinished: false,
  ),
];

void main() {
  group('DeveloperSettingsScreen', () {
    late MockUser user;
    late MockLoadableUserCubit loadableUserCubit;

    setUp(() async {
      user = MockUser();
      loadableUserCubit = MockLoadableUserCubit();

      when(() => user.userId).thenReturn(1.userId());
      when(() => loadableUserCubit.state).thenReturn(LoadableUser.loaded(user));
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<LoadableUserCubit>.value(value: loadableUserCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(MediaQuery.platformBrightnessOf(context)),
            home: ChangeUserScreenView(
              clientRecords: Future.value(clientRecords),
            ),
          );
        },
      ),
    );

    testWidgets('renders correctly', (tester) async {
      await tester.pumpWidget(buildSubject());
      await tester.pumpAndSettle();

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/change_user_screen.png'),
      );
    });
  });
}
