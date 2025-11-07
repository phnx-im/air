// SPDX-FileCopyrightText: 2025 Phoenix R&D GmbH <hello@phnx.im>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:air/core/core.dart';
import 'package:air/l10n/l10n.dart';
import 'package:air/theme/theme.dart';
import 'package:air/user/user.dart';

import '../helpers.dart';
import '../mocks.dart';

void main() {
  group('UserSettingsScreenTest', () {
    late MockUserCubit userCubit;
    late MockUsersCubit contactsCubit;
    late MockUserSettingsCubit userSettingsCubit;

    setUp(() async {
      userCubit = MockUserCubit();
      contactsCubit = MockUsersCubit();
      userSettingsCubit = MockUserSettingsCubit();

      when(() => contactsCubit.state).thenReturn(
        MockUsersState(
          profiles: [UiUserProfile(userId: 1.userId(), displayName: "ellie")],
        ),
      );
      when(() => userSettingsCubit.state).thenReturn(const UserSettings());
    });

    Widget buildSubject() => MultiBlocProvider(
      providers: [
        BlocProvider<UserCubit>.value(value: userCubit),
        BlocProvider<UsersCubit>.value(value: contactsCubit),
        BlocProvider<UserSettingsCubit>.value(value: userSettingsCubit),
      ],
      child: Builder(
        builder: (context) {
          return MaterialApp(
            debugShowCheckedModeBanner: false,
            theme: themeData(MediaQuery.platformBrightnessOf(context)),
            localizationsDelegates: AppLocalizations.localizationsDelegates,
            home: const UserSettingsScreen(),
          );
        },
      ),
    );

    testWidgets('renders correctly (no handles)', (tester) async {
      when(
        () => userCubit.state,
      ).thenReturn(MockUiUser(id: 1, userHandles: []));

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_no_handles.png'),
      );
    });

    testWidgets('renders correctly (some handles)', (tester) async {
      when(() => userCubit.state).thenReturn(
        MockUiUser(
          id: 1,
          userHandles: [
            const UiUserHandle(plaintext: "ellie"),
            const UiUserHandle(plaintext: "firefly"),
          ],
        ),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_some_handles.png'),
      );
    });

    testWidgets('renders correctly (all handles)', (tester) async {
      when(() => userCubit.state).thenReturn(
        MockUiUser(
          id: 1,
          userHandles: [
            const UiUserHandle(plaintext: "ellie"),
            const UiUserHandle(plaintext: "firefly"),
            const UiUserHandle(plaintext: "kiddo"),
            const UiUserHandle(plaintext: "ells"),
            const UiUserHandle(plaintext: "wolf"),
          ],
        ),
      );

      await tester.pumpWidget(buildSubject());

      await expectLater(
        find.byType(MaterialApp),
        matchesGoldenFile('goldens/user_settings_screen_all_handles.png'),
      );
    });
  });
}
